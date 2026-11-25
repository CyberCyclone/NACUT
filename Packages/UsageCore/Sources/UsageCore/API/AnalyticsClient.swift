import Foundation

/// Client for Anthropic's Usage & Cost Analytics API (beta).
///
/// Works with a Console Admin API key (`sk-ant-admin…`) or a Claude Enterprise
/// scoped key with `read:analytics`.
public struct AnalyticsClient: Sendable {
    public typealias Transport = @Sendable (URLRequest) async throws -> (Data, HTTPURLResponse)

    public enum APIError: Error, Sendable, Equatable {
        case http(Int, String)
        case cursorExpired
        case decoding(String)
    }

    public var key: String
    public var baseURL: URL
    var transport: Transport

    public static let betaHeader = "analytics-usage-cost-2026-08-24"
    public static let apiVersion = "2026-10-21"

    public init(key: String, baseURL: URL = URL(string: "https://api.anthropic.com")!, transport: Transport? = nil) {
        self.key = key
        self.baseURL = baseURL
        self.transport = transport ?? { req in
            let (data, resp) = try await URLSession.shared.data(for: req)
            guard let http = resp as? HTTPURLResponse else { throw URLError(.badServerResponse) }
            return (data, http)
        }
    }

    // MARK: Public calls

    /// Quick check: one cheap request over the last 7 days.
    public func testConnection(now: Date = .now) async -> ConnectionStatus {
        let start = now.addingTimeInterval(-7 * 86_400)
        do {
            let page: WirePage<WireCostRow> = try await get(
                path: "/v1/organizations/analytics/cost",
                query: [("starting_at", Self.rfc3339(start)), ("ending_at", Self.rfc3339(now)), ("limit", "1")])
            if let r = page.data_refreshed_at.flatMap(LogScanner.parseDate) { return .connected(refreshedAt: r) }
            return page.data.isEmpty ? .noDataYet : .connected(refreshedAt: nil)
        } catch {
            return Self.status(for: error)
        }
    }

    /// Hourly cost by model from `start` to `end`, split into ≤31-day requests.
    public func hourlyCost(from start: Date, to end: Date, actorId: String? = nil) async throws -> APICostData {
        var rows: [APICostRow] = []
        var refreshed: Date?
        var chunkStart = Self.floorHour(max(start, Self.earliest(now: end)))
        while chunkStart < end {
            let chunkEnd = min(chunkStart.addingTimeInterval(31 * 86_400), end)
            var query: [(String, String)] = [
                ("starting_at", Self.rfc3339(chunkStart)),
                ("ending_at", Self.rfc3339(chunkEnd)),
                ("time_bucket_width", "1h"),
                ("group_by[]", "model"),
                ("order_by", "time"),
                ("order", "asc"),
                ("limit", "1000"),
            ]
            if let actorId { query.append(("actor_ids[]", actorId)) }
            let (chunkRows, r) = try await paged(query: query)
            rows += chunkRows
            if let r { refreshed = max(refreshed ?? r, r) }
            chunkStart = chunkEnd
        }
        return APICostData(rows: rows, refreshedAt: refreshed, coverageStart: start, coverageEnd: end)
    }

    /// Who spent what over the last 30 days, for picking "only my usage".
    public func actors(now: Date = .now) async throws -> [APIActor] {
        let start = now.addingTimeInterval(-30 * 86_400)
        var out: [String: APIActor] = [:]
        var page: String?
        repeat {
            var q: [(String, String)] = [
                ("starting_at", Self.rfc3339(start)), ("ending_at", Self.rfc3339(now)),
                ("group_by[]", "actor"), ("limit", "100"),
            ]
            if let page { q.append(("page", page)) }
            let p: WirePage<WireCostRow> = try await get(path: "/v1/organizations/analytics/cost", query: q)
            for row in p.data {
                guard let actor = Self.actor(from: row.group?.actor) else { continue }
                var a = out[actor.id] ?? actor
                a.cost += Self.usd(row.cost.amount)
                out[actor.id] = a
            }
            page = p.has_more == true ? p.next_page : nil
        } while page != nil
        return out.values.sorted { $0.cost > $1.cost }
    }

    // MARK: Paging

    private func paged(query: [(String, String)]) async throws -> ([APICostRow], Date?) {
        var attempts = 0
        while true {
            do {
                var rows: [APICostRow] = []
                var refreshed: Date?
                var page: String?
                repeat {
                    var q = query
                    if let page { q.append(("page", page)) }
                    let p: WirePage<WireCostRow> = try await get(path: "/v1/organizations/analytics/cost", query: q)
                    refreshed = p.data_refreshed_at.flatMap(LogScanner.parseDate)
                    for row in p.data {
                        guard let s = LogScanner.parseDate(row.starting_at),
                              let e = LogScanner.parseDate(row.ending_at) else { continue }
                        rows.append(APICostRow(
                            start: s, end: e, model: row.group?.model,
                            amount: Self.usd(row.cost.amount), listAmount: Self.usd(row.cost.list_amount)))
                    }
                    page = p.has_more == true ? p.next_page : nil
                } while page != nil
                return (rows, refreshed)
            } catch APIError.cursorExpired where attempts < 2 {
                attempts += 1  // data refreshed mid-paging: start over from the first page
            }
        }
    }

    private func get<T: Decodable>(path: String, query: [(String, String)]) async throws -> T {
        var comps = URLComponents(url: baseURL.appending(path: path), resolvingAgainstBaseURL: false)!
        comps.percentEncodedQueryItems = query.map {
            URLQueryItem(
                name: $0.0.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed.subtracting(.init(charactersIn: "&=+"))) ?? $0.0,
                value: $0.1.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed.subtracting(.init(charactersIn: "&=+:"))))
        }
        var req = URLRequest(url: comps.url!)
        req.timeoutInterval = 30
        req.setValue(key, forHTTPHeaderField: "x-api-key")
        req.setValue(Self.apiVersion, forHTTPHeaderField: "anthropic-version")
        req.setValue(Self.betaHeader, forHTTPHeaderField: "anthropic-beta")
        req.setValue("NACUT/1.0", forHTTPHeaderField: "User-Agent")
        let (data, resp) = try await transport(req)
        switch resp.statusCode {
        case 200..<300:
            do { return try JSONDecoder().decode(T.self, from: data) }
            catch { throw APIError.decoding(String(describing: error)) }
        case 410:
            throw APIError.cursorExpired
        default:
            let msg = (try? JSONDecoder().decode(WireError.self, from: data))?.error?.message
                ?? String(data: data.prefix(300), encoding: .utf8) ?? ""
            throw APIError.http(resp.statusCode, msg)
        }
    }

    // MARK: Helpers

    public static func status(for error: Error) -> ConnectionStatus {
        switch error {
        case APIError.http(401, let m): return .unauthorized(m)
        case APIError.http(403, let m): return .forbidden(m)
        case APIError.http(404, let m): return .unavailable(m.isEmpty ? "(404)" : m)
        case APIError.http(let code, let m): return .failed("HTTP \(code) \(m)")
        case APIError.decoding(let m): return .failed("Unexpected response: \(m.prefix(160))")
        case let e as URLError: return .failed(e.localizedDescription)
        default: return .failed(String(describing: error))
        }
    }

    /// Amounts are decimal strings in cents.
    static func usd(_ cents: String?) -> Double {
        guard let cents, let v = Double(cents) else { return 0 }
        return v / 100
    }

    static func actor(from value: JSONValue?) -> APIActor? {
        guard let v = value, case .object = v else { return nil }
        let type = v["type"]?.string ?? "unknown"
        let id = v["user_id"]?.string ?? v["service_account_id"]?.string ?? v["api_key_id"]?.string
            ?? v["id"]?.string ?? v["email_address"]?.string
        guard let id else { return nil }
        let display = v["email_address"]?.string ?? v["email"]?.string ?? v["name"]?.string ?? id
        return APIActor(id: id, type: type, display: display, cost: 0)
    }

    /// The API serves at most 365 days back and nothing before 2026-01-01.
    static func earliest(now: Date) -> Date {
        let floor = ISO8601DateFormatter().date(from: "2026-01-01T00:00:00Z")!
        return max(floor, now.addingTimeInterval(-364 * 86_400))
    }

    static func floorHour(_ d: Date) -> Date {
        Date(timeIntervalSince1970: (d.timeIntervalSince1970 / 3600).rounded(.down) * 3600)
    }

    static func rfc3339(_ d: Date) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f.string(from: d)
    }
}

import Foundation

/// Where a day's number came from.
public enum CostSource: String, Codable, Sendable {
    case logs       // computed from local transcripts at list price
    case api        // billed amount from the Usage & Cost API
    case estimated  // local logs scaled by the API calibration ratio
}

public struct DayCost: Codable, Sendable, Hashable {
    public var cost: Double
    /// Cost per normalized model id.
    public var byModel: [String: Double]
    public var messages: Int
    public var tokens: Int
    public var source: CostSource

    public init(cost: Double = 0, byModel: [String: Double] = [:], messages: Int = 0, tokens: Int = 0, source: CostSource = .logs) {
        self.cost = cost
        self.byModel = byModel
        self.messages = messages
        self.tokens = tokens
        self.source = source
    }
}

/// Daily cost keyed by local start-of-day.
public typealias DailySeries = [Date: DayCost]

public struct ModelStat: Identifiable, Sendable, Hashable {
    public var id: String          // normalized model id
    public var name: String
    public var cost: Double
    public var messages: Int
    public var input: Int
    public var output: Int
    public var cacheWrite: Int
    public var cacheRead: Int
    public var fastCost: Double
    public var estimated: Bool
    public var share: Double
    public var tokens: Int { input + output + cacheWrite + cacheRead }
    public var costPerMessage: Double { messages > 0 ? cost / Double(messages) : 0 }
}

public struct ProjectStat: Identifiable, Sendable, Hashable {
    public var id: String { path }
    public var path: String
    public var name: String
    public var cost: Double
    public var messages: Int
    public var sessions: Int
    public var lastActive: Date
    public var share: Double
}

public struct SessionStat: Identifiable, Sendable, Hashable {
    public var id: String
    public var title: String
    public var project: String
    public var start: Date
    public var end: Date
    public var cost: Double
    public var messages: Int
    public var models: [String]
    public var entrypoint: String
    public var duration: TimeInterval { end.timeIntervalSince(start) }
}

public struct NamedValue: Identifiable, Sendable, Hashable {
    public var id: String { name }
    public var name: String
    public var value: Double
    public var tokens: Int = 0

    public init(name: String, value: Double, tokens: Int = 0) {
        self.name = name
        self.value = value
        self.tokens = tokens
    }
}

public enum TokenClass: String, CaseIterable, Sendable {
    case input = "Input"
    case output = "Output"
    case cacheWrite = "Cache write"
    case cacheRead = "Cache read"
}

/// In-memory index over priced records.
public struct UsageStore: Sendable {
    public let records: [UsageRecord]       // sorted by timestamp
    public let calendar: Calendar
    public let priceBook: PriceBook
    public let logSeries: DailySeries

    public init(records: [UsageRecord], priceBook: PriceBook, calendar: Calendar = .current) {
        let priced = priceBook.priced(records).sorted { $0.timestamp < $1.timestamp }
        self.records = priced
        self.calendar = calendar
        self.priceBook = priceBook
        var series: DailySeries = [:]
        for r in priced {
            let day = calendar.startOfDay(for: r.timestamp)
            var d = series[day] ?? DayCost()
            d.cost += r.cost
            d.byModel[PriceBook.normalize(r.model), default: 0] += r.cost
            d.messages += 1
            d.tokens += r.totalTokens
            series[day] = d
        }
        self.logSeries = series
    }

    public static let empty = UsageStore(records: [], priceBook: .bundled)

    public var firstDate: Date? { records.first?.timestamp }
    public var lastDate: Date? { records.last?.timestamp }

    // MARK: Range queries

    private func lowerBound(_ date: Date) -> Int {
        var lo = 0, hi = records.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if records[mid].timestamp < date { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    public func records(in interval: DateInterval) -> ArraySlice<UsageRecord> {
        let a = lowerBound(interval.start)
        let b = lowerBound(interval.end)
        return records[a..<max(a, b)]
    }

    public func cost(in interval: DateInterval) -> Double {
        records(in: interval).reduce(0) { $0 + $1.cost }
    }

    public func modelStats(in interval: DateInterval) -> [ModelStat] {
        var map: [String: ModelStat] = [:]
        var total = 0.0
        for r in records(in: interval) {
            let key = PriceBook.normalize(r.model)
            var s = map[key] ?? ModelStat(
                id: key, name: priceBook.displayName(for: r.model), cost: 0, messages: 0,
                input: 0, output: 0, cacheWrite: 0, cacheRead: 0, fastCost: 0, estimated: false, share: 0)
            s.cost += r.cost
            s.messages += 1
            s.input += r.input
            s.output += r.output
            s.cacheWrite += r.cacheWrite
            s.cacheRead += r.cacheRead
            if r.fast { s.fastCost += r.cost }
            s.estimated = s.estimated || r.estimated
            map[key] = s
            total += r.cost
        }
        return map.values.map { var s = $0; s.share = total > 0 ? s.cost / total : 0; return s }
            .sorted { $0.cost > $1.cost }
    }

    public func projectStats(in interval: DateInterval) -> [ProjectStat] {
        struct Acc { var cost = 0.0; var messages = 0; var sessions = Set<String>(); var last = Date.distantPast }
        var map: [String: Acc] = [:]
        var total = 0.0
        for r in records(in: interval) {
            var a = map[r.projectPath] ?? Acc()
            a.cost += r.cost
            a.messages += 1
            a.sessions.insert(r.sessionId)
            a.last = max(a.last, r.timestamp)
            map[r.projectPath] = a
            total += r.cost
        }
        return map.map { path, a in
            ProjectStat(
                path: path, name: path.isEmpty ? "Unknown" : (path as NSString).lastPathComponent,
                cost: a.cost, messages: a.messages, sessions: a.sessions.count, lastActive: a.last,
                share: total > 0 ? a.cost / total : 0)
        }.sorted { $0.cost > $1.cost }
    }

    public func sessions(in interval: DateInterval, limit: Int = 10) -> [SessionStat] {
        var map: [String: SessionStat] = [:]
        for r in records(in: interval) {
            let key = r.sessionId.isEmpty ? r.id : r.sessionId
            let name = priceBook.displayName(for: r.model)
            if var s = map[key] {
                s.cost += r.cost
                s.messages += 1
                s.start = min(s.start, r.timestamp)
                s.end = max(s.end, r.timestamp)
                if !s.models.contains(name) { s.models.append(name) }
                if s.title == SessionStat.untitled, let slug = r.slug { s.title = SessionStat.prettify(slug) }
                map[key] = s
            } else {
                map[key] = SessionStat(
                    id: key, title: r.slug.map(SessionStat.prettify) ?? SessionStat.untitled,
                    project: r.projectName, start: r.timestamp, end: r.timestamp, cost: r.cost,
                    messages: 1, models: [name], entrypoint: Entrypoint.displayName(r.entrypoint))
            }
        }
        return Array(map.values.sorted { $0.cost > $1.cost }.prefix(limit))
    }

    public func entrypointStats(in interval: DateInterval) -> [NamedValue] {
        var map: [String: Double] = [:]
        for r in records(in: interval) { map[Entrypoint.displayName(r.entrypoint), default: 0] += r.cost }
        return map.map { NamedValue(name: $0.key, value: $0.value) }.sorted { $0.value > $1.value }
    }

    /// (main agent cost, subagent cost)
    public func agentSplit(in interval: DateInterval) -> (main: Double, sub: Double) {
        var main = 0.0, sub = 0.0
        for r in records(in: interval) { if r.isSubagent { sub += r.cost } else { main += r.cost } }
        return (main, sub)
    }

    public func tokenClasses(in interval: DateInterval) -> [NamedValue] {
        var tok = [Int](repeating: 0, count: 4)
        var usd = [Double](repeating: 0, count: 4)
        for r in records(in: interval) {
            let (rates, _) = priceBook.rates(for: r)
            let m = (r.fast ? priceBook.table.fastMultiplier : 1) / 1_000_000
            tok[0] += r.input; usd[0] += Double(r.input) * rates.input * m
            tok[1] += r.output; usd[1] += Double(r.output) * rates.output * m
            tok[2] += r.cacheWrite
            usd[2] += (Double(r.cacheWrite5m) * rates.write5m + Double(r.cacheWrite1h) * rates.write1h) * m
            tok[3] += r.cacheRead; usd[3] += Double(r.cacheRead) * rates.read * m
        }
        return TokenClass.allCases.enumerated().map { i, c in NamedValue(name: c.rawValue, value: usd[i], tokens: tok[i]) }
    }

    public func cacheSavings(in interval: DateInterval) -> Double {
        records(in: interval).reduce(0) { $0 + priceBook.cacheSavings(of: $1) }
    }

    /// 7 × 24 grid of cost, rows Monday…Sunday, columns hour 0…23.
    public func weekdayHourHeatmap(in interval: DateInterval) -> [[Double]] {
        var grid = [[Double]](repeating: [Double](repeating: 0, count: 24), count: 7)
        for r in records(in: interval) {
            let c = calendar.dateComponents([.weekday, .hour], from: r.timestamp)
            let row = ((c.weekday ?? 1) + 5) % 7  // Sunday=1 → 6, Monday=2 → 0
            grid[row][c.hour ?? 0] += r.cost
        }
        return grid
    }

    /// Cost per hour of one day, split by model display name.
    public func hourly(on day: Date) -> [(hour: Int, model: String, cost: Double)] {
        let start = calendar.startOfDay(for: day)
        let end = calendar.date(byAdding: .day, value: 1, to: start)!
        var map: [String: Double] = [:]
        for r in records(in: DateInterval(start: start, end: end)) {
            let h = calendar.component(.hour, from: r.timestamp)
            map["\(h)|\(priceBook.displayName(for: r.model))", default: 0] += r.cost
        }
        return map.map { k, v in
            let parts = k.split(separator: "|", maxSplits: 1)
            return (Int(parts[0]) ?? 0, String(parts[1]), v)
        }.sorted { $0.hour < $1.hour }
    }

    /// Busiest hour of day across an interval.
    public func busiestHour(in interval: DateInterval) -> Int? {
        var hours = [Double](repeating: 0, count: 24)
        for r in records(in: interval) { hours[calendar.component(.hour, from: r.timestamp)] += r.cost }
        guard let m = hours.max(), m > 0 else { return nil }
        return hours.firstIndex(of: m)
    }
}

extension SessionStat {
    static let untitled = "Untitled session"
    /// `i-want-you-to-cached-sloth-of-46b41c9d-fd9b` → `I want you to cached sloth`
    static func prettify(_ slug: String) -> String {
        var words = slug.split(separator: "-").map(String.init)
        let isHexish: (String) -> Bool = { w in
            w.count >= 4 && w.allSatisfy(\.isHexDigit) && w.contains(where: \.isNumber)
        }
        if let firstId = words.firstIndex(where: isHexish) { words = Array(words[..<firstId]) }
        if let last = words.last, ["of", "to", "for", "the", "a", "and"].contains(last) { words.removeLast() }
        guard !words.isEmpty else { return untitled }
        let s = words.joined(separator: " ")
        return s.prefix(1).uppercased() + s.dropFirst()
    }
}

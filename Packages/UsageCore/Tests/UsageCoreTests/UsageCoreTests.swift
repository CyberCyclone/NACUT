import Foundation
import Testing
@testable import UsageCore

private let fixtures = Bundle.module.url(forResource: "Fixtures", withExtension: nil)!
private let utc: Calendar = {
    var c = Calendar(identifier: .gregorian)
    c.timeZone = TimeZone(identifier: "UTC")!
    return c
}()

private func scanFixtures() async -> LogCache {
    let scanner = LogScanner(roots: [fixtures.appending(path: "proj")])
    var cache = LogCache()
    _ = cache.merge(await scanner.scan(checkpoints: [:]))
    return cache
}

private func date(_ s: String) -> Date { LogScanner.parseDate(s)! }

@Suite struct ParsingTests {
    @Test func dedupKeepsFinalLineAndSkipsJunk() async {
        let cache = await scanFixtures()
        let ids = Set(cache.records.keys)
        #expect(ids == ["msg_1", "msg_2", "msg_3", "msg_4", "msg_9"])
        #expect(cache.records["msg_1"]?.output == 500)
        #expect(cache.records["msg_1"]?.cacheWrite1h == 1000)
        #expect(cache.records["msg_1"]?.cacheWrite5m == 0)
        #expect(cache.records["msg_9"]?.isSubagent == true)
        #expect(cache.records["msg_2"]?.fast == true)
        #expect(cache.records["msg_4"]?.webSearches == 3)
        #expect(cache.records["msg_1"]?.entrypoint == "claude-desktop")
    }

    @Test func partialLastLineIsReadLater() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appending(path: "s.jsonl")
        let line1 = #"{"type":"assistant","timestamp":"2026-11-20T01:00:00Z","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":1}}}"#
        let line2 = #"{"type":"assistant","timestamp":"2026-11-20T02:00:00Z","message":{"id":"m2","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":1}}}"#
        try (line1 + "\n" + line2.prefix(40)).write(to: file, atomically: true, encoding: .utf8)
        let r1 = try #require(LogScanner.scanFile(file, from: nil))
        #expect(r1.records.map(\.id) == ["m1"])

        let h = try FileHandle(forWritingTo: file)
        try h.seekToEnd()
        try h.write(contentsOf: Data((line2.dropFirst(40) + "\n").utf8))
        try h.close()
        let r2 = try #require(LogScanner.scanFile(file, from: r1.checkpoint))
        #expect(r2.records.map(\.id) == ["m2"])
        #expect(LogScanner.scanFile(file, from: r2.checkpoint) == nil)
    }

    @Test func worktreesFoldIntoRepo() {
        #expect(UsageRecord.projectPath(for: "/Users/me/Code/app/.claude/worktrees/x") == "/Users/me/Code/app")
    }

    @Test func cacheRoundTrip() async throws {
        let cache = await scanFixtures()
        let url = FileManager.default.temporaryDirectory.appending(path: "\(UUID()).json")
        try cache.save(to: url)
        let loaded = LogCache.load(from: url)
        #expect(loaded.records.count == cache.records.count)
        #expect(loaded.checkpoints == cache.checkpoints)
    }
}

@Suite struct PricingTests {
    let book = PriceBook.bundled

    @Test func normalization() {
        #expect(PriceBook.normalize("claude-sonnet-5-5-20261001") == "claude-sonnet-5-5")
        #expect(PriceBook.normalize("claude-opus-5-5[1m]") == "claude-opus-5-5")
        #expect(PriceBook.normalize("claude-mythos-5-2") == "claude-fable-5-2")
        #expect(PriceBook.normalize("anthropic.claude-opus-5-5") == "claude-opus-5-5")
        #expect(book.displayName(for: "claude-opus-7-1") == "Opus 7.1")
    }

    @Test func fixtureCosts() async {
        let cache = await scanFixtures()
        let priced = Dictionary(uniqueKeysWithValues: book.priced(Array(cache.records.values)).map { ($0.id, $0) })
        #expect(abs(priced["msg_1"]!.cost - 0.01844) < 1e-9)          // 1h cache write at $8
        #expect(abs(priced["msg_2"]!.cost - 8.0) < 1e-9)              // fast ×2
        #expect(abs(priced["msg_3"]!.cost - 2.575) < 1e-9)            // Haiku 5.5 >100K tier
        #expect(abs(priced["msg_4"]!.cost - 4.03) < 1e-9)             // unknown opus → newest opus + web search
        #expect(priced["msg_4"]!.estimated)
        #expect(abs(priced["msg_9"]!.cost - 12.1) < 1e-9)
        #expect(!priced["msg_9"]!.estimated)
    }

    @Test func overrides() {
        let r = UsageRecord(id: "x", timestamp: .now, model: "claude-opus-5-5", input: 1_000_000, output: 0,
                            cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0)
        let b = book.withOverrides(["claude-opus-5-5": Rates(input: 1, output: 1, write5m: 1, write1h: 1, read: 1)])
        #expect(b.cost(of: r).cost == 1)
    }
}

@Suite struct StoreTests {
    @Test func aggregates() async {
        let cache = await scanFixtures()
        let store = UsageStore(records: Array(cache.records.values), priceBook: .bundled, calendar: utc)
        let all = DateInterval(start: date("2026-11-01T00:00:00Z"), end: date("2026-12-01T00:00:00Z"))
        #expect(abs(store.cost(in: all) - 26.72344) < 1e-9)
        #expect(abs((store.logSeries[date("2026-11-20T00:00:00Z")]?.cost ?? 0) - 14.62344) < 1e-9)
        let models = store.modelStats(in: all)
        #expect(models.first?.id == "claude-sonnet-5-5")
        #expect(abs(models.reduce(0) { $0 + $1.share } - 1) < 1e-9)
        let split = store.agentSplit(in: all)
        #expect(abs(split.sub - 12.1) < 1e-9)
        #expect(store.projectStats(in: all).count == 1)
        let classes = store.tokenClasses(in: all)
        #expect(abs(classes.reduce(0) { $0 + $1.value } + 0.03 - 26.72344) < 1e-9)
        #expect(store.cacheSavings(in: all) > 0)
    }
}

@Suite struct ForecastTests {
    func series(_ pairs: [(String, Double)]) -> DailySeries {
        Dictionary(uniqueKeysWithValues: pairs.map { (date($0.0 + "T00:00:00Z"), DayCost(cost: $0.1)) })
    }

    @Test func cycleBoundaries() {
        let c = BillingCycle.containing(date("2026-11-10T12:00:00Z"), startDay: 15, calendar: utc)
        #expect(c.start == date("2026-10-15T00:00:00Z"))
        #expect(c.end == date("2026-11-15T00:00:00Z"))
        let c2 = BillingCycle.containing(date("2026-11-15T00:00:00Z"), startDay: 15, calendar: utc)
        #expect(c2.start == date("2026-11-15T00:00:00Z"))
        let c3 = BillingCycle.containing(date("2026-02-10T00:00:00Z"), startDay: 31, calendar: utc)
        #expect(c3.start == date("2026-01-28T00:00:00Z"))
    }

    @Test func projectionAndExhaustion() {
        // $50/day for 10 full days, now noon on day 11 with $25 so far.
        var pairs: [(String, Double)] = (1...10).map { (String(format: "2026-11-%02d", $0), 50) }
        pairs.append(("2026-11-11", 25))
        let f = Forecast(series: series(pairs), budget: 1000, cycleStartDay: 1,
                         now: date("2026-11-11T12:00:00Z"), calendar: utc)
        #expect(f.spent == 525)
        #expect(f.today == 25)
        #expect(abs(f.elapsedDays - 10.5) < 1e-9)
        #expect(abs(f.dailyAverage - 50) < 1e-9)
        #expect(abs(f.projected - (525 + 50 * 19.5)) < 1e-6)
        #expect(f.status == .willExceed)
        // (1000-525)/50 = 9.5 days after Nov 11 12:00 → Nov 21 00:00
        #expect(f.exhaustionDate == date("2026-11-21T00:00:00Z"))
        #expect(abs((f.safeDailySpend ?? 0) - 475 / 19.5) < 1e-9)
    }

    @Test func underBudgetAndExceeded() {
        let low = Forecast(series: series([("2026-11-01", 1)]), budget: 1000, cycleStartDay: 1,
                           now: date("2026-11-05T12:00:00Z"), calendar: utc)
        #expect(low.status == .underBudget)
        let over = Forecast(series: series([("2026-11-01", 600), ("2026-11-02", 600)]), budget: 1000,
                            cycleStartDay: 1, now: date("2026-11-03T12:00:00Z"), calendar: utc)
        #expect(over.status == .exceeded)
        #expect(over.exhaustionDate == date("2026-11-02T00:00:00Z"))
        let none = Forecast(series: [:], budget: 0, cycleStartDay: 1, now: .now)
        #expect(none.status == .noBudget)
    }

    @Test func insights() {
        let s = series([("2026-11-09", 5), ("2026-11-10", 10), ("2026-11-11", 3), ("2026-10-10", 4)])
        let cycle = BillingCycle.containing(date("2026-11-11T12:00:00Z"), startDay: 1, calendar: utc)
        let i = Insights(series: s, cycle: cycle, now: date("2026-11-11T12:00:00Z"), calendar: utc)
        #expect(i.activeStreak == 3)
        #expect(i.peakDay?.cost == 10)
        #expect(i.allTime == 22)
        #expect(i.previousCycleTotal == 4)
        #expect(i.vsPreviousCycle == (18.0 - 4) / 4)
    }
}

@Suite struct APITests {
    static func response(_ json: String, status: Int = 200) -> (Data, HTTPURLResponse) {
        (Data(json.utf8), HTTPURLResponse(url: URL(string: "https://x")!, statusCode: status, httpVersion: nil, headerFields: nil)!)
    }

    final class Pages: @unchecked Sendable {
        var calls: [URLRequest] = []
        var pages: [(Data, HTTPURLResponse)]
        init(_ pages: [(Data, HTTPURLResponse)]) { self.pages = pages }
        func next(_ r: URLRequest) -> (Data, HTTPURLResponse) { calls.append(r); return pages.removeFirst() }
    }

    @Test func pagingCentsAnd410Restart() async throws {
        let p1 = #"{"data":[{"starting_at":"2026-11-20T01:00:00Z","ending_at":"2026-11-20T02:00:00Z","group":{"model":"claude-opus-5-5"},"cost":{"amount":"41280.000000","list_amount":"51600.000000","currency":"USD"}}],"has_more":true,"next_page":"page_2","data_refreshed_at":"2026-11-24T16:02:11Z"}"#
        let p2 = #"{"data":[{"starting_at":"2026-11-20T02:00:00Z","ending_at":"2026-11-20T03:00:00Z","group":{"model":null},"cost":{"amount":"100.000000","list_amount":"100.000000","currency":"USD"}}],"has_more":false,"next_page":null,"data_refreshed_at":"2026-11-24T16:02:11Z"}"#
        let pages = Pages([Self.response(p1), Self.response("{}", status: 410), Self.response(p1), Self.response(p2)])
        let client = AnalyticsClient(key: "k") { pages.next($0) }
        let data = try await client.hourlyCost(from: date("2026-11-20T00:00:00Z"), to: date("2026-11-25T00:00:00Z"))
        #expect(data.rows.count == 2)
        #expect(data.rows[0].amount == 412.80)
        #expect(data.rows[0].listAmount == 516.00)
        #expect(data.rows[1].model == nil)
        #expect(data.refreshedAt == date("2026-11-24T16:02:11Z"))
        let first = try #require(pages.calls.first)
        #expect(first.value(forHTTPHeaderField: "anthropic-beta") == AnalyticsClient.betaHeader)
        #expect(first.value(forHTTPHeaderField: "x-api-key") == "k")
        #expect(first.url!.absoluteString.contains("time_bucket_width=1h"))
        #expect(pages.calls[3].url!.absoluteString.contains("page=page_2"))
    }

    @Test func statusMapping() async {
        let pages = Pages([Self.response(#"{"error":{"type":"permission_error","message":"missing read:analytics"}}"#, status: 403)])
        let client = AnalyticsClient(key: "k") { pages.next($0) }
        let s = await client.testConnection()
        #expect(s == .forbidden("missing read:analytics"))
        let empty = Pages([Self.response(#"{"data":[],"has_more":false,"data_refreshed_at":null}"#)])
        let s2 = await AnalyticsClient(key: "k") { empty.next($0) }.testConnection()
        #expect(s2 == .noDataYet)
    }

    @Test func actorParsing() {
        let v = try! JSONDecoder().decode(JSONValue.self, from: Data(#"{"type":"user","user_id":"user_1","email_address":"a@b.c"}"#.utf8))
        let a = AnalyticsClient.actor(from: v)
        #expect(a?.id == "user_1")
        #expect(a?.display == "a@b.c")
    }

    @Test func hybridMergeAndCalibration() {
        let merger = SourceMerger(calendar: utc)
        var logs: DailySeries = [:]
        var rows: [APICostRow] = []
        for d in 10...19 {
            let day = date(String(format: "2026-11-%02dT00:00:00Z", d))
            logs[day] = DayCost(cost: 100, byModel: ["claude-opus-5-5": 100])
            if d < 19 {
                rows.append(APICostRow(start: day.addingTimeInterval(3600), end: day.addingTimeInterval(7200),
                                       model: "claude-opus-5-5", amount: 80, listAmount: 100))
            }
        }
        // API refreshed at Nov 18 12:00; it has 80 billed on Nov 18 before that.
        let api = APICostData(rows: rows, refreshedAt: date("2026-11-18T12:00:00Z"),
                              coverageStart: date("2026-11-10T00:00:00Z"), coverageEnd: date("2026-11-20T00:00:00Z"))
        let cal = merger.calibration(logs: logs, api: api)
        #expect(cal.sampleDays == 8)
        #expect(abs(cal.ratio - 0.8) < 1e-9)

        let (s, _) = merger.merge(mode: .hybrid, logs: logs, api: api) { _, _ in DayCost(cost: 50) }
        #expect(s[date("2026-11-12T00:00:00Z")]?.cost == 80)
        #expect(s[date("2026-11-12T00:00:00Z")]?.source == .api)
        #expect(abs((s[date("2026-11-18T00:00:00Z")]?.cost ?? 0) - (80 + 40)) < 1e-9)
        #expect(abs((s[date("2026-11-19T00:00:00Z")]?.cost ?? 0) - 80) < 1e-9)
        #expect(s[date("2026-11-19T00:00:00Z")]?.source == .estimated)

        let (apiOnly, _) = merger.merge(mode: .api, logs: logs, api: api) { _, _ in DayCost() }
        #expect(apiOnly[date("2026-11-19T00:00:00Z")] == nil)
        let rec = merger.reconciliation(logs: logs, api: api)
        #expect(rec.count == 8)
    }

    @Test func limitsParsing() throws {
        let data = try Data(contentsOf: fixtures.appending(path: "limits.json"))
        let limits = PlanLimit.parse(data)
        #expect(limits.map(\.id) == ["five_hour", "seven_day"])
        #expect(abs(limits[0].utilization - 0.42) < 1e-9)
        #expect(limits[0].resetsAt == date("2026-11-25T08:00:00Z"))
    }

    /// Real response captured 2026-11-25 (no credentials in it): `limits` array, microsecond timestamps.
    @Test func liveLimitsResponse() throws {
        let data = try Data(contentsOf: fixtures.appending(path: "limits-live.json"))
        let limits = PlanLimit.parse(data)
        #expect(limits.map(\.id) == ["five_hour", "seven_day", "seven_day_fable"])
        #expect(limits.map(\.label) == ["5-hour session", "Weekly · all models", "Weekly · Fable"])
        #expect(abs(limits[1].utilization - 0.15) < 1e-9)
        let reset = try #require(limits[0].resetsAt)
        #expect(abs(reset.timeIntervalSince(date("2026-11-25T05:39:59Z")) - 0.95) < 0.01)
    }
}

@Suite struct FormattingTests {
    @Test func sessionTitles() {
        #expect(SessionStat.prettify("i-want-you-to-cached-sloth-of-46b41c9d-fd9b-47cc") == "I want you to cached sloth")
        #expect(SessionStat.prettify("floating-bouncing-church") == "Floating bouncing church")
        #expect(SessionStat.prettify("46b41c9d") == "Untitled session")
    }

    @Test func dollarsAreUSStyle() {
        #expect(CurrencyFormat.string(521.4) == "$521")
        #expect(CurrencyFormat.string(8.5) == "$8.50")
        #expect(CurrencyFormat.string(1234) == "$1,234")
    }
}

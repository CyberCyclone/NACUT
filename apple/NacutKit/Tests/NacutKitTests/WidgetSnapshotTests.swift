import Foundation
import Testing
@testable import NacutKit

@Suite("Widget snapshot JSON and file")
struct WidgetSnapshotTests {
    static let sample = WidgetSnapshot(
        generatedAt: date(2026, 11, 26, 9, 30), isSubscription: true, today: 12.5, cycleSpent: 310.25,
        cycleStart: date(2026, 11, 1), cycleEnd: date(2026, 12, 1), budget: 500, projected: 402.75,
        status: .tight, headline: "Tight · projected $403 of $500",
        last14: [1, 2.5, 0, 0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12.5],
        monthDays: [1: 4.5, 15: 0.25, 26: 12.5], monthTitle: "November",
        topModels: [.init(name: "Opus 5.5", cost: 240), .init(name: "Fable 5.2", cost: 52)],
        limits: [.init(label: "5h", utilization: 0.42, resetsAt: date(2026, 11, 26, 12)),
                 .init(label: "Week", utilization: 0.1, resetsAt: nil)],
        showLimits: true)

    /// What the current app writes (formatted by hand). The widget must keep reading this.
    static let currentFormatJSON = """
    {
      "generatedAt": "2026-11-26T09:30:00Z",
      "isSubscription": true,
      "today": 12.5,
      "cycleSpent": 310.25,
      "cycleStart": "2026-11-01T00:00:00Z",
      "cycleEnd": "2026-12-01T00:00:00Z",
      "budget": 500,
      "projected": 402.75,
      "status": "tight",
      "headline": "Tight · projected $403 of $500",
      "last14": [1, 2.5, 0, 0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12.5],
      "monthDays": {"1": 4.5, "15": 0.25, "26": 12.5},
      "monthTitle": "November",
      "topModels": [{"name": "Opus 5.5", "cost": 240}, {"name": "Fable 5.2", "cost": 52}],
      "limits": [{"label": "5h", "utilization": 0.42, "resetsAt": "2026-11-26T12:00:00Z"},
                 {"label": "Week", "utilization": 0.1}],
      "showLimits": true
    }
    """

    @Test func decodesTheCurrentFormat() throws {
        let s = try WidgetSnapshot.decode(Data(Self.currentFormatJSON.utf8))
        #expect(s == Self.sample)
        #expect(s.monthDays[26] == 12.5)
        #expect(s.limits[1].resetsAt == nil)
    }

    @Test func decodesSnapshotsWrittenBeforeShowLimits() throws {
        var json = try #require(try JSONSerialization.jsonObject(with: Data(Self.currentFormatJSON.utf8)) as? [String: Any])
        json["showLimits"] = nil
        let s = try WidgetSnapshot.decode(JSONSerialization.data(withJSONObject: json))
        #expect(s.showLimits == nil)
    }

    @Test func roundTripsThroughJSON() throws {
        let s = try WidgetSnapshot.decode(Self.sample.encoded())
        #expect(s == Self.sample)
    }

    @Test func roundTripsEmptyCollectionsAndNilShowLimits() throws {
        var e = Self.sample
        e.last14 = []
        e.monthDays = [:]
        e.topModels = []
        e.limits = []
        e.showLimits = nil
        #expect(try WidgetSnapshot.decode(e.encoded()) == e)
    }

    @Test func wireFormatKeysAndTypes() throws {
        let obj = try #require(try JSONSerialization.jsonObject(with: Self.sample.encoded()) as? [String: Any])
        #expect(Set(obj.keys) == ["generatedAt", "isSubscription", "today", "cycleSpent", "cycleStart", "cycleEnd",
                                  "budget", "projected", "status", "headline", "last14", "monthDays", "monthTitle",
                                  "topModels", "limits", "showLimits"])
        // Int-keyed dictionaries are JSON objects with string keys.
        let monthDays = try #require(obj["monthDays"] as? [String: Double])
        #expect(monthDays == ["1": 4.5, "15": 0.25, "26": 12.5])
        #expect(obj["status"] as? String == "tight")
        // ISO 8601 in UTC without fractional seconds.
        #expect(obj["generatedAt"] as? String == "2026-11-26T09:30:00Z")
        let limits = try #require(obj["limits"] as? [[String: Any]])
        #expect(limits[0]["resetsAt"] as? String == "2026-11-26T12:00:00Z")
        #expect(limits[1]["resetsAt"] == nil)
    }

    @Test func subSecondPrecisionIsDropped() throws {
        var s = Self.sample
        s.generatedAt = date(2026, 11, 26, 9, 30).addingTimeInterval(0.75)
        let obj = try #require(try JSONSerialization.jsonObject(with: s.encoded()) as? [String: Any])
        #expect(obj["generatedAt"] as? String == "2026-11-26T09:30:00Z")
        #expect(try WidgetSnapshot.decode(s.encoded()).generatedAt == date(2026, 11, 26, 9, 30))
    }

    @Test func toleratesFractionalSecondDates() throws {
        // The app never writes them, but a widget reading one shouldn't fall back to the placeholder.
        let json = Self.currentFormatJSON.replacingOccurrences(of: "2026-11-26T09:30:00Z", with: "2026-11-26T09:30:00.500Z")
        let offset = try WidgetSnapshot.decode(Data(json.utf8)).generatedAt.timeIntervalSince(date(2026, 11, 26, 9, 30))
        #expect(offset >= 0 && offset < 1)
    }

    @Test func rejectsDatesWithoutATimeZone() {
        let json = Self.currentFormatJSON.replacingOccurrences(of: "2026-11-26T09:30:00Z", with: "2026-11-26 09:30:00")
        #expect(throws: (any Error).self) { try WidgetSnapshot.decode(Data(json.utf8)) }
    }

    @Test func statusRawValuesAreStable() {
        #expect(SnapshotStatus.allCases.map(\.rawValue) == ["noBudget", "underBudget", "tight", "willExceed", "exceeded"])
    }

    @Test func writesAndLoadsFromAnInjectedLocation() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appending(path: "nested/NACUT/widget-snapshot.json")
        try Self.sample.write(to: url)
        #expect(WidgetSnapshot.load(from: url) == Self.sample)
        let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
        #expect((attrs[.posixPermissions] as? NSNumber)?.intValue == 0o600)
        // Overwrites in place.
        var newer = Self.sample
        newer.today = 99
        try newer.write(to: url)
        #expect(WidgetSnapshot.load(from: url)?.today == 99)
    }

    @Test func loadReturnsNilForMissingOrCorruptFiles() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        #expect(WidgetSnapshot.load(from: dir.appending(path: "missing.json")) == nil)
        let bad = dir.appending(path: "bad.json")
        try Data("{\"generatedAt\": 5}".utf8).write(to: bad)
        #expect(WidgetSnapshot.load(from: bad) == nil)
        try Data().write(to: bad)
        #expect(WidgetSnapshot.load(from: bad) == nil)
    }

    @Test func sharedURLIsInTheRealApplicationSupportFolder() {
        #expect(WidgetSnapshot.sharedURL.path.hasSuffix("/Library/Application Support/NACUT/widget-snapshot.json"))
        #expect(WidgetSnapshot.sharedURL.path.hasPrefix(WidgetSnapshot.realHome.path))
    }

    @Test func placeholderIsRelativeToNow() throws {
        let now = date(2026, 11, 26, 9, 30)
        let p = WidgetSnapshot.placeholder(at: now)
        #expect(p.generatedAt == now)
        #expect(p.cycleStart == now.addingTimeInterval(-20 * 86_400))
        #expect(p.cycleEnd == now.addingTimeInterval(10 * 86_400))
        #expect(p.monthDays.count == 24)
        #expect(p.last14.count == 14)
        #expect(try WidgetSnapshot.decode(p.encoded()) == p)
    }
}

@Suite("Widget snapshot month days")
struct MonthDaysTests {
    @Test func keepsOnlyTheCurrentMonth() {
        let cal = fixedCalendar()
        let days = [DayCost(year: 2026, month: 10, day: 31, cost: 1),
                    DayCost(year: 2026, month: 11, day: 1, cost: 2),
                    DayCost(year: 2026, month: 11, day: 26, cost: 3),
                    DayCost(year: 2025, month: 11, day: 26, cost: 4)]
        #expect(WidgetSnapshot.monthDays(days, now: date(2026, 11, 26, 9), calendar: cal) == [1: 2, 26: 3])
    }

    @Test func yearRollover() {
        let cal = fixedCalendar()
        let days = [DayCost(year: 2026, month: 12, day: 31, cost: 1), DayCost(year: 2027, month: 1, day: 1, cost: 2)]
        #expect(WidgetSnapshot.monthDays(days, now: date(2027, 1, 1, 0, 0, 1), calendar: cal) == [1: 2])
        #expect(WidgetSnapshot.monthDays(days, now: date(2026, 12, 31, 23, 59, 59), calendar: cal) == [31: 1])
    }

    @Test func usesTheCalendarsTimeZone() {
        // 23:30 UTC on 30 Nov is already 1 December in Auckland.
        let now = date(2026, 11, 30, 23, 30)
        let days = [DayCost(year: 2026, month: 11, day: 30, cost: 1), DayCost(year: 2026, month: 12, day: 1, cost: 2)]
        #expect(WidgetSnapshot.monthDays(days, now: now, calendar: fixedCalendar()) == [30: 1])
        #expect(WidgetSnapshot.monthDays(days, now: now, calendar: fixedCalendar(timeZone: "Pacific/Auckland")) == [1: 2])
    }

    @Test func emptySeries() {
        #expect(WidgetSnapshot.monthDays([], now: date(2026, 11, 26), calendar: fixedCalendar()).isEmpty)
    }
}

@Suite("Widget dollars")
struct WidgetUSDTests {
    @Test(arguments: [
        (0.0, "$0.00"), (4.21, "$4.21"), (99.99, "$99.99"), (100, "$100"), (1234.4, "$1,234"),
        (-5, "-$5.00"), (-250, "-$250"), (.nan, "$0.00"), (.infinity, "$0.00"), (-.infinity, "$0.00"),
    ] as [(Double, String)])
    func formats(value: Double, expected: String) {
        #expect(WidgetUSD.string(value) == expected)
    }
}

@Suite("Plan limit utilization")
struct EffectiveUtilizationTests {
    let now = date(2026, 11, 26, 12)

    @Test func beforeReset() {
        #expect(WidgetSnapshot.Limit(label: "5h", utilization: 0.4, resetsAt: now.addingTimeInterval(1)).effectiveUtilization(at: now) == 0.4)
    }

    @Test func atAndAfterReset() {
        #expect(WidgetSnapshot.Limit(label: "5h", utilization: 0.4, resetsAt: now).effectiveUtilization(at: now) == 0)
        #expect(WidgetSnapshot.Limit(label: "5h", utilization: 0.4, resetsAt: now.addingTimeInterval(-60)).effectiveUtilization(at: now) == 0)
    }

    @Test func withoutResetTime() {
        #expect(WidgetSnapshot.Limit(label: "Week", utilization: 1.2, resetsAt: nil).effectiveUtilization(at: now) == 1.2)
    }
}

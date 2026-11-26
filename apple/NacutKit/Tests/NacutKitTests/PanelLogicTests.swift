import Foundation
import Testing
@testable import NacutKit

@Suite("Coral ramp")
struct RampTests {
    @Test(arguments: [
        (0.0, nil), (-1, nil), (.nan, nil), (-.infinity, nil),
        (0.0001, 0), (0.2, 0), (0.2000001, 1), (0.4, 1), (0.41, 2), (0.6, 2), (0.61, 3), (0.8, 3), (0.81, 4),
        (1, 4), (1.5, 4), (1e300, 4), (.infinity, 4),
    ] as [(Double, Int?)])
    func index(fraction: Double, expected: Int?) {
        #expect(Ramp.index(fraction) == expected)
    }

    @Test func otherStepCounts() {
        #expect(Ramp.index(0.5, steps: 2) == 0)
        #expect(Ramp.index(0.51, steps: 2) == 1)
        #expect(Ramp.index(0.5, steps: 1) == 0)
        #expect(Ramp.index(0.5, steps: 0) == nil)
    }

    @Test func lightInkOnTheDarkSteps() {
        #expect(!Ramp.needsLightInk(0))
        #expect(!Ramp.needsLightInk(0.6))
        #expect(Ramp.needsLightInk(0.61))
        #expect(Ramp.needsLightInk(.infinity))
        #expect(!Ramp.needsLightInk(.nan))
    }
}

@Suite("Session duration")
struct SessionDurationTests {
    @Test(arguments: [
        (0.0, "<1m"), (59.9, "<1m"), (-5, "<1m"), (.nan, "<1m"), (.infinity, "<1m"), (-.infinity, "<1m"),
        (60, "1m"), (3599, "59m"), (3600, "1h 0m"), (3 * 3600 + 5 * 60 + 30, "3h 5m"),
        (48 * 3600 - 1, "47h 59m"), (48 * 3600, "2d 0h"), (49.5 * 3600, "2d 1h"), (10 * 86_400 + 7200, "10d 2h"),
    ] as [(Double, String)])
    func formats(seconds: Double, expected: String) {
        #expect(formatSessionDuration(seconds) == expected)
    }

    @Test func hugeValuesDontTrap() {
        #expect(formatSessionDuration(1e20).hasSuffix("h"))
    }
}

@Suite("Menu bar gauge")
struct MenuBarGaugeTests {
    @Test(arguments: [
        (-1.0, "0percent"), (0, "0percent"), (0.1999, "0percent"), (0.2, "33percent"), (0.4499, "33percent"),
        (0.45, "50percent"), (0.5999, "50percent"), (0.6, "67percent"), (0.8499, "67percent"),
        (0.85, "100percent"), (1, "100percent"), (7, "100percent"), (.nan, "100percent"),
    ] as [(Double, String)])
    func needle(fraction: Double, suffix: String) {
        #expect(MenuBarGauge.needle(fraction) == "gauge.with.dots.needle.\(suffix)")
    }

    @Test func fullLimitShowsAWarning() {
        #expect(MenuBarGauge.symbol(showsLimits: true, gaugeFraction: 1, budgetExceeded: false) == "exclamationmark.triangle.fill")
        #expect(MenuBarGauge.symbol(showsLimits: true, gaugeFraction: 1.3, budgetExceeded: false) == "exclamationmark.triangle.fill")
        #expect(MenuBarGauge.symbol(showsLimits: true, gaugeFraction: 0.99, budgetExceeded: false) == "gauge.with.dots.needle.100percent")
    }

    @Test func exceededBudgetOnlyMattersInCostMode() {
        #expect(MenuBarGauge.symbol(showsLimits: false, gaugeFraction: 0.3, budgetExceeded: true) == "exclamationmark.triangle.fill")
        #expect(MenuBarGauge.symbol(showsLimits: true, gaugeFraction: 0.5, budgetExceeded: true) == "gauge.with.dots.needle.50percent")
        #expect(MenuBarGauge.symbol(showsLimits: false, gaugeFraction: 1.2, budgetExceeded: false) == "gauge.with.dots.needle.100percent")
    }

    @Test func nothingToMeasure() {
        #expect(MenuBarGauge.symbol(showsLimits: false, gaugeFraction: nil, budgetExceeded: false) == "gauge.with.dots.needle.33percent")
        #expect(MenuBarGauge.symbol(showsLimits: true, gaugeFraction: nil, budgetExceeded: false) == "gauge.with.dots.needle.33percent")
    }
}

@Suite("Donut selection")
struct DonutSelectionTests {
    let values = [10.0, 5, 0, 20]

    @Test func noAngleOrNoSlices() {
        #expect(donutSliceIndex(angle: nil, values: values) == nil)
        #expect(donutSliceIndex(angle: 3, values: []) == nil)
    }

    @Test(arguments: [(0.0, 0), (9.99, 0), (10, 0), (10.01, 1), (15, 1), (15.01, 3), (35, 3)] as [(Double, Int)])
    func slice(angle: Double, expected: Int) {
        #expect(donutSliceIndex(angle: angle, values: values) == expected)
    }

    @Test func pastTheEndOrNaN() {
        #expect(donutSliceIndex(angle: 35.01, values: values) == nil)
        #expect(donutSliceIndex(angle: .nan, values: values) == nil)
    }

    @Test func negativeAngleIsTheFirstSlice() {
        #expect(donutSliceIndex(angle: -1, values: values) == 0)
    }
}

@Suite("Daily bars by model")
struct DailyBreakdownTests {
    let slots = ["claude-opus-5-5": 0, "claude-fable-5-2": 1, "claude-sonnet-5-5": 2]
    var slotOrder: (String) -> Int { { slots[$0] ?? 99 } }
    let names = ["claude-opus-5-5": "Opus 5.5", "claude-fable-5-2": "Fable 5.2", "claude-sonnet-5-5": "Sonnet 5.5"]
    var displayName: (String) -> String { { names[$0] ?? $0 } }

    @Test func emptyInput() {
        #expect(DailyBreakdown.rows([], slotOrder: slotOrder, displayName: displayName).isEmpty)
        #expect(DailyBreakdown.modelIds([], slotOrder: slotOrder).isEmpty)
    }

    @Test func rowsInSlotOrderWithDisplayNames() {
        let d = date(2026, 11, 26)
        let day = DailyBreakdown.Day(date: d, cost: 6, byModel: ["claude-sonnet-5-5": 1, "zeta-model": 2, "claude-opus-5-5": 3])
        let rows = DailyBreakdown.rows([day], slotOrder: slotOrder, displayName: displayName)
        #expect(rows.map(\.model) == ["Opus 5.5", "Sonnet 5.5", "zeta-model"])
        #expect(rows.map(\.cost) == [3, 1, 2])
        #expect(rows.allSatisfy { $0.date == d })
        #expect(rows[0].id == "\(d.timeIntervalSince1970)-Opus 5.5")
    }

    @Test func otherCoversTheUnattributedRest() {
        let d = date(2026, 11, 26)
        let rows = DailyBreakdown.rows([.init(date: d, cost: 5, byModel: ["claude-opus-5-5": 3])],
                                       slotOrder: slotOrder, displayName: displayName)
        #expect(rows.map(\.model) == ["Opus 5.5", "Other"])
        #expect(rows.last?.cost == 2)
    }

    @Test func dayWithOnlyOther() {
        let rows = DailyBreakdown.rows([.init(date: date(2026, 11, 26), cost: 1.25, byModel: [:])],
                                       slotOrder: slotOrder, displayName: displayName)
        #expect(rows.map(\.model) == [DailyBreakdown.otherName])
        #expect(rows.first?.cost == 1.25)
    }

    @Test func noOtherForRoundingNoiseOrOverAttribution() {
        let d = date(2026, 11, 26)
        let days = [DailyBreakdown.Day(date: d, cost: 3.005, byModel: ["claude-opus-5-5": 3]),
                    DailyBreakdown.Day(date: d.addingTimeInterval(86_400), cost: 2, byModel: ["claude-opus-5-5": 3]),
                    DailyBreakdown.Day(date: d.addingTimeInterval(2 * 86_400), cost: 0, byModel: [:])]
        #expect(!DailyBreakdown.rows(days, slotOrder: slotOrder, displayName: displayName).contains { $0.model == "Other" })
    }

    @Test func modelIdsAreUniqueAndOrdered() {
        let d = date(2026, 11, 26)
        let days = [DailyBreakdown.Day(date: d, cost: 1, byModel: ["b-unknown": 1, "claude-fable-5-2": 1]),
                    DailyBreakdown.Day(date: d, cost: 1, byModel: ["a-unknown": 1, "claude-fable-5-2": 1, "claude-opus-5-5": 1])]
        #expect(DailyBreakdown.modelIds(days, slotOrder: slotOrder) == ["claude-opus-5-5", "claude-fable-5-2", "a-unknown", "b-unknown"])
    }
}

@Suite("Small formatters")
struct SmallFormatterTests {
    @Test(arguments: [
        (0.0, "$0"), (0.08, "$0.08"), (0.3, "$0.3"), (0.375, "$0.375"), (0.0625, "$0.0625"), (0.9996, "$1"),
        (1, "$1"), (1.5, "$1.50"), (3.75, "$3.75"), (15, "$15"), (100, "$100"), (18.75, "$18.75"),
    ] as [(Double, String)])
    func perMillionPrice(value: Double, expected: String) {
        #expect(formatPerMillionPrice(value) == expected)
    }

    @Test(arguments: [
        ("sk-ant-admin01-abc", true), ("  sk-ant-admin01-abc", true), ("\tsk-ant-admin", true),
        ("sk-ant-api03-abc", false), ("", false), ("SK-ANT-ADMIN01", false), ("\nsk-ant-admin01", false),
        ("x sk-ant-admin01", false),
    ] as [(String, Bool)])
    func adminKeyPrefix(input: String, isAdmin: Bool) {
        #expect(isAdminAPIKey(input) == isAdmin)
    }

    @Test func wholePercent() {
        let en = Locale(identifier: "en_US_POSIX")
        #expect(NacutKit.wholePercent(0, locale: en) == "0%")
        #expect(NacutKit.wholePercent(0.424, locale: en) == "42%")
        #expect(NacutKit.wholePercent(0.426, locale: en) == "43%")
        #expect(NacutKit.wholePercent(1, locale: en) == "100%")
        #expect(NacutKit.wholePercent(1.5, locale: en) == "150%")
        #expect(plainSpaces(NacutKit.wholePercent(0.42, locale: Locale(identifier: "fr_FR"))) == "42 %")
    }
}

@Suite("Render options")
struct RenderOptionsTests {
    @Test func absentOrIncomplete() {
        #expect(RenderOptions.parse(arguments: ["NACUT"]) == nil)
        #expect(RenderOptions.parse(arguments: ["NACUT", "--render-to"]) == nil)
        #expect(RenderOptions.parse(arguments: []) == nil)
        #expect(RenderOptions.parse(arguments: ["NACUT", "--limits-json", "/tmp/l.json"]) == nil)
    }

    @Test func outputDirOnly() throws {
        let o = try #require(RenderOptions.parse(arguments: ["NACUT", "--render-to", "/tmp/shots"]))
        #expect(o.outputDir.path == "/tmp/shots")
        #expect(o.outputDir.hasDirectoryPath)
        #expect(o.limitsJSON == nil)
    }

    @Test func withLimitsJSONInEitherOrder() throws {
        let a = try #require(RenderOptions.parse(arguments: ["NACUT", "--render-to", "/tmp/s", "--limits-json", "/x/l.json"]))
        let b = try #require(RenderOptions.parse(arguments: ["NACUT", "--limits-json", "/x/l.json", "--render-to", "/tmp/s"]))
        #expect(a == b)
        #expect(a.limitsJSON?.path == "/x/l.json")
    }

    @Test func limitsJSONWithoutAPathIsIgnored() throws {
        let o = try #require(RenderOptions.parse(arguments: ["NACUT", "--render-to", "/tmp/s", "--limits-json"]))
        #expect(o.limitsJSON == nil)
    }

    @Test func firstOccurrenceWins() throws {
        let o = try #require(RenderOptions.parse(arguments: ["NACUT", "--render-to", "/a", "--render-to", "/b"]))
        #expect(o.outputDir.path == "/a")
    }
}

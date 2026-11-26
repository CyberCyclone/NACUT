import Foundation
import Testing
@testable import NacutKit

@Suite("Widget timeline refresh")
struct NextRefreshTests {
    let now = date(2026, 11, 26, 12)

    func snapshot(resets: [Date?]) -> WidgetSnapshot {
        var s = WidgetSnapshot.placeholder(at: now)
        s.limits = resets.map { .init(label: "x", utilization: 0.5, resetsAt: $0) }
        return s
    }

    @Test func fifteenMinutesWithoutASnapshotOrLimits() {
        #expect(WidgetSnapshot.nextRefresh(for: nil, now: now) == now.addingTimeInterval(900))
        #expect(WidgetSnapshot.nextRefresh(for: snapshot(resets: []), now: now) == now.addingTimeInterval(900))
        #expect(WidgetSnapshot.nextRefresh(for: snapshot(resets: [nil]), now: now) == now.addingTimeInterval(900))
    }

    @Test func fiveSecondsAfterASoonerReset() {
        let reset = now.addingTimeInterval(300)
        #expect(WidgetSnapshot.nextRefresh(for: snapshot(resets: [reset]), now: now) == reset.addingTimeInterval(5))
    }

    @Test func laterResetsDontDelayTheRefresh() {
        #expect(WidgetSnapshot.nextRefresh(for: snapshot(resets: [now.addingTimeInterval(3600)]), now: now) == now.addingTimeInterval(900))
        // Reset + 5 s lands exactly on the 15-minute mark.
        #expect(WidgetSnapshot.nextRefresh(for: snapshot(resets: [now.addingTimeInterval(895)]), now: now) == now.addingTimeInterval(900))
    }

    @Test func picksTheEarliestFutureReset() {
        let s = snapshot(resets: [now.addingTimeInterval(600), nil, now.addingTimeInterval(120), now.addingTimeInterval(-30)])
        #expect(WidgetSnapshot.nextRefresh(for: s, now: now) == now.addingTimeInterval(125))
    }

    @Test func ignoresResetsThatArentInTheFuture() {
        let s = snapshot(resets: [now, now.addingTimeInterval(-1)])
        #expect(WidgetSnapshot.nextRefresh(for: s, now: now) == now.addingTimeInterval(900))
    }
}

@Suite("Widget status and headline")
struct WidgetStatusTests {
    @Test(arguments: [
        (SnapshotStatus.noBudget, StatusTone.good, "checkmark.circle.fill"),
        (.underBudget, .good, "checkmark.circle.fill"),
        (.tight, .tight, "exclamationmark.circle.fill"),
        (.willExceed, .willExceed, "exclamationmark.triangle.fill"),
        (.exceeded, .exceeded, "xmark.octagon.fill"),
    ])
    func toneAndSymbol(status: SnapshotStatus, tone: StatusTone, symbol: String) {
        #expect(status.tone == tone)
        #expect(status.symbolName == symbol)
    }

    @Test func shortHeadlineWithBudget() {
        var s = WidgetSnapshot.placeholder(at: date(2026, 11, 26))
        s.projected = 468
        s.budget = 500
        #expect(s.shortHeadline == "→ $468 of $500")
        s.projected = 42.5
        s.budget = 50
        #expect(s.shortHeadline == "→ $42.50 of $50.00")
    }

    @Test func shortHeadlineWithoutBudget() {
        var s = WidgetSnapshot.placeholder(at: date(2026, 11, 26))
        s.projected = 1234.4
        s.budget = 0
        #expect(s.shortHeadline == "→ $1,234 projected")
        s.budget = -10
        #expect(s.shortHeadline == "→ $1,234 projected")
    }
}

@Suite("Widget limit titles and reset text")
struct WidgetLimitTextTests {
    let now = date(2026, 11, 26, 10)   // a Thursday
    let cal = fixedCalendar()
    let locale = Locale(identifier: "en_US_POSIX")

    func limit(resetsIn secs: Double?) -> WidgetSnapshot.Limit {
        .init(label: "5h", utilization: 0.5, resetsAt: secs.map { now.addingTimeInterval($0) })
    }

    @Test(arguments: [("5h", "5-hour"), ("Week", "Weekly"), ("Fable", "Fable"), ("", ""), ("week", "week")])
    func title(label: String, expected: String) {
        #expect(WidgetSnapshot.Limit(label: label, utilization: 0, resetsAt: nil).title == expected)
    }

    @Test func passedOrMissingReset() {
        #expect(limit(resetsIn: nil).resetText(at: now, calendar: cal, locale: locale) == "reset")
        #expect(limit(resetsIn: 0).resetText(at: now, calendar: cal, locale: locale) == "reset")
        #expect(limit(resetsIn: -3600).resetText(at: now, calendar: cal, locale: locale) == "reset")
    }

    @Test(arguments: [
        (0.5, "0m"), (59, "0m"), (60, "1m"), (3599, "59m"), (3600, "1h 0m"), (5400, "1h 30m"),
        (4 * 3600 + 35 * 60 + 59, "4h 35m"), (86_399, "23h 59m"),
    ] as [(Double, String)])
    func withinADay(secs: Double, expected: String) {
        #expect(limit(resetsIn: secs).resetText(at: now, calendar: cal, locale: locale) == expected)
    }

    @Test func aDayOrMoreShowsWeekdayAndHour() {
        #expect(plainSpaces(limit(resetsIn: 86_400).resetText(at: now, calendar: cal, locale: locale)) == "Fri 10 AM")
        #expect(plainSpaces(limit(resetsIn: 3 * 86_400 + 5 * 3600).resetText(at: now, calendar: cal, locale: locale)) == "Sun 3 PM")
    }

    @Test func weekdayAndHourFollowTheCalendarsTimeZone() {
        // Fri 10:00 UTC is Fri 19:00 in Tokyo and Fri 02:00 in Los Angeles.
        let l = limit(resetsIn: 86_400)
        #expect(plainSpaces(l.resetText(at: now, calendar: fixedCalendar(timeZone: "Asia/Tokyo"), locale: locale)) == "Fri 7 PM")
        #expect(plainSpaces(l.resetText(at: now, calendar: fixedCalendar(timeZone: "America/Los_Angeles"), locale: locale)) == "Fri 2 AM")
    }
}

@Suite("Widget mini calendar")
struct MiniCalendarTests {
    @Test func novemberStartsOnSunday() {
        let g = MiniCalendarGrid(date: date(2026, 11, 26, 9), calendar: fixedCalendar(firstWeekday: 1))
        #expect(g.cells.first == 1)
        #expect(g.cells.count == 30)
        #expect(g.today == 26)
    }

    @Test func mondayFirstCalendarLeadsWithSixBlanks() {
        let g = MiniCalendarGrid(date: date(2026, 11, 26, 9), calendar: fixedCalendar(firstWeekday: 2))
        #expect(g.cells.prefix(6).allSatisfy { $0 == nil })
        #expect(g.cells[6] == 1)
        #expect(g.cells.last == 30)
        #expect(g.cells.count == 36)
    }

    @Test func februaryAndLeapYears() {
        #expect(MiniCalendarGrid(date: date(2027, 2, 10), calendar: fixedCalendar()).cells.compactMap { $0 }.count == 28)
        #expect(MiniCalendarGrid(date: date(2028, 2, 29), calendar: fixedCalendar()).cells.compactMap { $0 }.count == 29)
        // 1 Feb 2027 is a Monday.
        #expect(MiniCalendarGrid(date: date(2027, 2, 10), calendar: fixedCalendar(firstWeekday: 2)).cells.first == 1)
        #expect(MiniCalendarGrid(date: date(2027, 2, 10), calendar: fixedCalendar(firstWeekday: 1)).cells.first == .some(nil))
    }

    @Test func monthAndTodayFollowTheCalendarsTimeZone() {
        let g = MiniCalendarGrid(date: date(2026, 11, 30, 23, 30), calendar: fixedCalendar(timeZone: "Pacific/Auckland"))
        #expect(g.today == 1)
        #expect(g.cells.compactMap { $0 }.count == 31)   // December
    }

    @Test func scaleIsNeverZero() {
        #expect(MiniCalendarGrid.scale([:]) == 0.0001)
        #expect(MiniCalendarGrid.scale([1: 0, 2: 0]) == 0.0001)
        #expect(MiniCalendarGrid.scale([1: 3, 2: 7.5]) == 7.5)
    }
}

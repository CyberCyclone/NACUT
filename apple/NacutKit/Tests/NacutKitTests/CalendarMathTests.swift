import Foundation
import Testing
@testable import NacutKit

@Suite("Calendar tab months")
struct MonthsTests {
    let cal = fixedCalendar()
    let now = date(2026, 11, 26, 9)

    @Test func noHistoryIsJustThisMonth() {
        #expect(CalendarMath.months(from: nil, now: now, calendar: cal) == [date(2026, 11, 1)])
    }

    @Test func fromTheFirstRecordedMonth() {
        #expect(CalendarMath.months(from: date(2026, 9, 17, 15), now: now, calendar: cal)
                == [date(2026, 9, 1), date(2026, 10, 1), date(2026, 11, 1)])
    }

    @Test func acrossAYearBoundary() {
        let months = CalendarMath.months(from: date(2025, 11, 30), now: date(2026, 2, 1), calendar: cal)
        #expect(months == [date(2025, 11, 1), date(2025, 12, 1), date(2026, 1, 1), date(2026, 2, 1)])
    }

    @Test func historyInTheFutureStillEndsAtThisMonth() {
        #expect(CalendarMath.months(from: date(2027, 3, 1), now: now, calendar: cal) == [date(2026, 11, 1)])
    }

    @Test func monthStartsAreLocalMidnight() {
        let ny = fixedCalendar(timeZone: "America/New_York")
        let months = CalendarMath.months(from: date(2026, 10, 15, in: ny), now: date(2026, 11, 26, in: ny), calendar: ny)
        #expect(months == [date(2026, 10, 1, in: ny), date(2026, 11, 1, in: ny)])
        #expect(months.allSatisfy { ny.component(.hour, from: $0) == 0 })
    }
}

@Suite("Calendar tab color scale")
struct ScaleMaxTests {
    @Test func defaultsToOneWithoutActiveDays() {
        #expect(CalendarMath.scaleMax([Double]()) == 1)
        #expect(CalendarMath.scaleMax([0, -2, .nan]) == 1)
    }

    @Test func singleDay() {
        #expect(CalendarMath.scaleMax([5]) == 5)
    }

    @Test func floorOfOneCent() {
        #expect(CalendarMath.scaleMax([0.001, 0.002]) == 0.01)
    }

    @Test func ninetyFifthPercentile() {
        #expect(CalendarMath.scaleMax((1...20).map(Double.init)) == 20)          // index 19 of 20
        #expect(CalendarMath.scaleMax((1...100).map(Double.init).reversed()) == 96) // index 95, input order ignored
        #expect(CalendarMath.scaleMax([1, 1000]) == 1000)                       // index 1 of 2
    }

    @Test func ignoresZeroAndNegativeDays() {
        #expect(CalendarMath.scaleMax([0, 0, 0, 3, -4]) == 3)
    }
}

@Suite("Calendar tab month stepping")
struct StepTests {
    let months = [date(2026, 9, 1), date(2026, 10, 1), date(2026, 11, 1)]

    @Test func stepsAndClamps() {
        #expect(CalendarMath.step(-1, from: months[2], in: months) == months[1])
        #expect(CalendarMath.step(1, from: months[0], in: months) == months[1])
        #expect(CalendarMath.step(1, from: months[2], in: months) == months[2])
        #expect(CalendarMath.step(-5, from: months[1], in: months) == months[0])
        #expect(CalendarMath.step(0, from: months[1], in: months) == months[1])
    }

    @Test func startsFromTheLastMonthWhenNothingIsVisible() {
        #expect(CalendarMath.step(-1, from: nil, in: months) == months[1])
    }

    @Test func unknownOrEmpty() {
        #expect(CalendarMath.step(1, from: date(2020, 1, 1), in: months) == nil)
        #expect(CalendarMath.step(1, from: nil, in: []) == nil)
    }
}

@Suite("Calendar tab grid")
struct GridTests {
    @Test func weekdaySymbolsStartOnTheFirstWeekday() {
        let sunday = CalendarMath.weekdaySymbols(calendar: fixedCalendar(firstWeekday: 1))
        let monday = CalendarMath.weekdaySymbols(calendar: fixedCalendar(firstWeekday: 2))
        let strip = { (s: String) in s.replacingOccurrences(of: "\u{200B}", with: "") }
        #expect(sunday.map(strip) == ["S", "M", "T", "W", "T", "F", "S"])
        #expect(monday.map(strip) == ["M", "T", "W", "T", "F", "S", "S"])
        #expect(Set(sunday).count == 7)
        #expect(Set(monday).count == 7)
        #expect(monday.enumerated().allSatisfy { i, s in s.filter { $0 == "\u{200B}" }.count == i })
    }

    @Test func alwaysSixWeeks() {
        for firstWeekday in 1...7 {
            for month in 1...12 {
                let cal = fixedCalendar(firstWeekday: firstWeekday)
                let days = CalendarMath.gridDays(month: date(2026, month, 1, in: cal), calendar: cal)
                #expect(days.count == 42)
                let first = try! #require(days.firstIndex { $0 != nil })
                #expect(cal.component(.weekday, from: days[first]!) == (first + firstWeekday - 1) % 7 + 1)
            }
        }
    }

    @Test func novemberSundayFirst() {
        let cal = fixedCalendar(firstWeekday: 1)
        let days = CalendarMath.gridDays(month: date(2026, 11, 1), calendar: cal)
        #expect(days[0] == date(2026, 11, 1))
        #expect(days[29] == date(2026, 11, 30))
        #expect(days[30...].allSatisfy { $0 == nil })
        #expect(CalendarMath.leadingBlanks(monthStart: date(2026, 11, 1), calendar: cal) == 0)
    }

    @Test func novemberMondayFirst() {
        let cal = fixedCalendar(firstWeekday: 2)
        let days = CalendarMath.gridDays(month: date(2026, 11, 1), calendar: cal)
        #expect(days[..<6].allSatisfy { $0 == nil })
        #expect(days[6] == date(2026, 11, 1))
        #expect(days.compactMap { $0 }.count == 30)
        #expect(CalendarMath.leadingBlanks(monthStart: date(2026, 11, 1), calendar: cal) == 6)
    }

    @Test func monthThatNeedsSixRows() {
        // 1 Aug 2026 is a Saturday: 6 blanks + 31 days = 37 cells.
        let cal = fixedCalendar(firstWeekday: 1)
        let days = CalendarMath.gridDays(month: date(2026, 8, 1), calendar: cal)
        #expect(days.count == 42)
        #expect(days[6] == date(2026, 8, 1))
        #expect(days[36] == date(2026, 8, 31))
    }

    @Test func daysAreLocalMidnightsAcrossADSTChange() {
        // US daylight saving time ends on 1 Nov 2026.
        let ny = fixedCalendar(timeZone: "America/New_York")
        let days = CalendarMath.gridDays(month: date(2026, 11, 1, in: ny), calendar: ny).compactMap { $0 }
        #expect(days.count == 30)
        #expect(days.allSatisfy { ny.component(.hour, from: $0) == 0 })
        #expect(days.map { ny.component(.day, from: $0) } == Array(1...30))
    }
}

@Suite("Calendar tab month summary")
struct MonthSummaryTests {
    let cal = fixedCalendar()

    @Test func countsOnlyActiveDaysInTheMonth() {
        let costs: [Date: Double] = [
            date(2026, 10, 31): 50,      // previous month
            date(2026, 11, 1): 10,       // first day counts
            date(2026, 11, 2): 0,        // inactive
            date(2026, 11, 15): 30,
            date(2026, 11, 30): 20,
            date(2026, 12, 1): 99,       // the interval's end: next month
        ]
        let s = MonthSummaryStats(month: date(2026, 11, 20), costs: costs, calendar: cal)
        #expect(s.total == 60)
        #expect(s.activeDays == 3)
        #expect(s.averagePerActiveDay == 20)
        #expect(s.busiest?.day == date(2026, 11, 15))
        #expect(s.busiest?.cost == 30)
    }

    @Test func emptyMonth() {
        let s = MonthSummaryStats(month: date(2026, 11, 1), costs: [date(2026, 10, 3): 4], calendar: cal)
        #expect(s.total == 0)
        #expect(s.activeDays == 0)
        #expect(s.averagePerActiveDay == nil)
        #expect(s.busiest == nil)
    }

    @Test func busiestTieIsTheEarliestDay() {
        let costs = [date(2026, 11, 20): 5.0, date(2026, 11, 3): 5, date(2026, 11, 9): 5]
        #expect(MonthSummaryStats(month: date(2026, 11, 1), costs: costs, calendar: cal).busiest?.day == date(2026, 11, 3))
    }

    @Test func decemberToJanuary() {
        let costs = [date(2026, 12, 31): 7.0, date(2027, 1, 1): 3]
        #expect(MonthSummaryStats(month: date(2026, 12, 1), costs: costs, calendar: cal).total == 7)
        #expect(MonthSummaryStats(month: date(2027, 1, 31), costs: costs, calendar: cal).total == 3)
    }
}

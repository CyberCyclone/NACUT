import Foundation

/// Month and grid arithmetic for the Calendar tab and the widget's mini calendar.
/// Every function takes the calendar (time zone, first weekday) and the current time explicitly.
public enum CalendarMath {
    /// Start of each month from the one containing `firstDay` (or `now` if there is no history)
    /// through the month containing `now`, oldest first. Never empty.
    public static func months(from firstDay: Date?, now: Date, calendar: Calendar) -> [Date] {
        let current = calendar.dateInterval(of: .month, for: now)!.start
        let first = firstDay.flatMap { calendar.dateInterval(of: .month, for: $0)?.start } ?? current
        var out: [Date] = []
        var m = min(first, current)
        while m <= current {
            out.append(m)
            m = calendar.date(byAdding: .month, value: 1, to: m)!
        }
        return out
    }

    /// Color scale shared by all months so they're comparable: the 95th-percentile active day.
    /// 1 when there are no active days; never below 0.01.
    public static func scaleMax(_ costs: some Sequence<Double>) -> Double {
        let active = costs.filter { $0 > 0 }.sorted()
        guard !active.isEmpty else { return 1 }
        return max(active[min(Int(Double(active.count) * 0.95), active.count - 1)], 0.01)
    }

    /// The month `delta` steps from `current` (the last month when `current` is nil), clamped to the
    /// list. `nil` if `current` isn't in `months` or `months` is empty.
    public static func step(_ delta: Int, from current: Date?, in months: [Date]) -> Date? {
        guard let current = current ?? months.last, let i = months.firstIndex(of: current) else { return nil }
        return months[min(max(i + delta, 0), months.count - 1)]
    }

    /// Blank cells before the 1st so the grid's first column is `calendar.firstWeekday`.
    public static func leadingBlanks(monthStart: Date, calendar: Calendar) -> Int {
        (calendar.component(.weekday, from: monthStart) - calendar.firstWeekday + 7) % 7
    }

    /// Very short weekday symbols starting at `calendar.firstWeekday`. Each gets a different number
    /// of zero-width spaces so duplicates (`S`, `T`) stay unique for `ForEach`.
    public static func weekdaySymbols(calendar: Calendar) -> [String] {
        let s = calendar.veryShortStandaloneWeekdaySymbols
        let first = calendar.firstWeekday - 1
        return Array(s[first...] + s[..<first]).enumerated().map {
            "\($0.element)\(String(repeating: "\u{200B}", count: $0.offset))"
        }
    }

    /// The month's days laid out in weeks (`nil` for padding), always 42 cells (6 rows of 7).
    public static func gridDays(month: Date, calendar: Calendar) -> [Date?] {
        let range = calendar.range(of: .day, in: .month, for: month)!
        var out: [Date?] = Array(repeating: nil, count: leadingBlanks(monthStart: month, calendar: calendar))
        for d in range { out.append(calendar.date(byAdding: .day, value: d - 1, to: month)) }
        while out.count % 7 != 0 { out.append(nil) }
        while out.count < 42 { out.append(nil) }
        return out
    }
}

/// The figures under the Calendar tab's month grid.
public struct MonthSummaryStats: Sendable, Equatable {
    public var total: Double
    public var activeDays: Int
    /// Total per active day; `nil` with no active days.
    public var averagePerActiveDay: Double?
    /// The highest-cost day (the earliest one on a tie).
    public var busiest: (day: Date, cost: Double)?

    /// `costs` is keyed by local midnight. Only days inside `month`'s calendar month with a positive
    /// cost count.
    public init(month: Date, costs: [Date: Double], calendar: Calendar) {
        let interval = calendar.dateInterval(of: .month, for: month)!
        let days = costs.filter { interval.contains($0.key) && $0.key < interval.end && $0.value > 0 }
        total = days.values.reduce(0, +)
        activeDays = days.count
        averagePerActiveDay = days.isEmpty ? nil : total / Double(days.count)
        busiest = days.max { ($0.value, $1.key) < ($1.value, $0.key) }.map { (day: $0.key, cost: $0.value) }
    }

    public static func == (a: Self, b: Self) -> Bool {
        a.total == b.total && a.activeDays == b.activeDays && a.averagePerActiveDay == b.averagePerActiveDay
            && a.busiest?.day == b.busiest?.day && a.busiest?.cost == b.busiest?.cost
    }
}

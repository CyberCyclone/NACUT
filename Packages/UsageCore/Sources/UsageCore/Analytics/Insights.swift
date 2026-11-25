import Foundation

public struct Insights: Sendable {
    public var allTime: Double
    public var last7Days: Double
    public var last30Days: Double
    public var peakDay: (date: Date, cost: Double)?
    public var activeStreak: Int
    public var activeDaysThisCycle: Int
    /// Cycle-to-date vs the same number of days into the previous cycle; nil without history.
    public var vsPreviousCycle: Double?
    public var previousCycleTotal: Double

    public init(series: DailySeries, cycle: BillingCycle, now: Date = .now, calendar: Calendar = .current) {
        let today = calendar.startOfDay(for: now)
        allTime = series.values.reduce(0) { $0 + $1.cost }

        func sum(daysBack n: Int) -> Double {
            (0..<n).reduce(0) { acc, i in
                acc + (calendar.date(byAdding: .day, value: -i, to: today).flatMap { series[$0]?.cost } ?? 0)
            }
        }
        last7Days = sum(daysBack: 7)
        last30Days = sum(daysBack: 30)

        if let best = series.max(by: { $0.value.cost < $1.value.cost }), best.value.cost > 0 {
            peakDay = (best.key, best.value.cost)
        } else {
            peakDay = nil
        }

        // Consecutive active days ending today (or yesterday if today has no usage yet).
        var streak = 0
        var d = (series[today]?.cost ?? 0) > 0 ? today : calendar.date(byAdding: .day, value: -1, to: today)!
        while (series[d]?.cost ?? 0) > 0 {
            streak += 1
            d = calendar.date(byAdding: .day, value: -1, to: d)!
        }
        activeStreak = streak

        activeDaysThisCycle = series.filter { cycle.interval.contains($0.key) && $0.value.cost > 0 }.count

        let prev = cycle.previous(calendar: calendar)
        previousCycleTotal = series.filter { prev.interval.contains($0.key) }.reduce(0) { $0 + $1.value.cost }
        let elapsed = now.timeIntervalSince(cycle.start)
        let prevCutoff = prev.start.addingTimeInterval(elapsed)
        let prevToDate = series.filter { $0.key >= prev.start && $0.key < prevCutoff }.reduce(0) { $0 + $1.value.cost }
        let thisToDate = series.filter { cycle.interval.contains($0.key) && $0.key <= today }.reduce(0) { $0 + $1.value.cost }
        vsPreviousCycle = prevToDate > 0 ? (thisToDate - prevToDate) / prevToDate : nil
    }
}

public extension DailySeries {
    func cost(in interval: DateInterval) -> Double {
        reduce(0) { interval.contains($1.key) && $1.key < interval.end ? $0 + $1.value.cost : $0 }
    }

    /// Consecutive days ending at `end`, filling gaps with zero.
    func days(endingAt end: Date, count: Int, calendar: Calendar = .current) -> [(date: Date, cost: DayCost)] {
        let last = calendar.startOfDay(for: end)
        return (0..<count).reversed().map { i in
            let d = calendar.date(byAdding: .day, value: -i, to: last)!
            return (d, self[d] ?? DayCost())
        }
    }
}

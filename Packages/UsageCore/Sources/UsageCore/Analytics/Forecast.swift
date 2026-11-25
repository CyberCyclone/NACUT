import Foundation

public enum BudgetStatus: String, Codable, Sendable {
    case noBudget
    case underBudget
    case tight
    case willExceed
    case exceeded
}

/// A billing cycle of one month starting on `startDay` (1…28).
public struct BillingCycle: Sendable, Hashable {
    public var start: Date
    public var end: Date
    public var interval: DateInterval { DateInterval(start: start, end: end) }

    public static func containing(_ date: Date, startDay: Int, calendar: Calendar = .current) -> BillingCycle {
        let day = min(max(startDay, 1), 28)
        let comps = calendar.dateComponents([.year, .month, .day], from: date)
        var startComps = DateComponents(year: comps.year, month: comps.month, day: day)
        if (comps.day ?? 1) < day {
            let prev = calendar.date(byAdding: .month, value: -1, to: calendar.date(from: startComps)!)!
            startComps = calendar.dateComponents([.year, .month, .day], from: prev)
        }
        let start = calendar.date(from: startComps)!
        let end = calendar.date(byAdding: .month, value: 1, to: start)!
        return BillingCycle(start: start, end: end)
    }

    public func previous(calendar: Calendar = .current) -> BillingCycle {
        let s = calendar.date(byAdding: .month, value: -1, to: start)!
        return BillingCycle(start: s, end: start)
    }

    public func days(calendar: Calendar = .current) -> [Date] {
        var out: [Date] = []
        var d = start
        while d < end {
            out.append(d)
            d = calendar.date(byAdding: .day, value: 1, to: d)!
        }
        return out
    }
}

public struct ForecastPoint: Identifiable, Sendable, Hashable {
    public var id: Date { date }
    public var date: Date
    public var cumulative: Double
    public var kind: Kind
    public enum Kind: String, Sendable { case actual, projected, pace }
}

public struct Forecast: Sendable {
    public var cycle: BillingCycle
    public var budget: Double
    public var spent: Double
    public var today: Double
    public var elapsedDays: Double
    public var remainingDays: Double
    public var totalDays: Double
    public var dailyAverage: Double
    public var trailing7Average: Double
    public var projectedRate: Double
    public var projected: Double
    public var exhaustionDate: Date?
    public var status: BudgetStatus
    public var safeDailySpend: Double?
    public var points: [ForecastPoint]

    public var budgetUsedFraction: Double { budget > 0 ? spent / budget : 0 }
    public var projectedFraction: Double { budget > 0 ? projected / budget : 0 }

    public init(series: DailySeries, budget: Double, cycleStartDay: Int, now: Date = .now, calendar: Calendar = .current) {
        let cycle = BillingCycle.containing(now, startDay: cycleStartDay, calendar: calendar)
        self.cycle = cycle
        self.budget = budget
        let todayStart = calendar.startOfDay(for: now)
        let days = cycle.days(calendar: calendar)
        totalDays = Double(days.count)

        var cumulative = 0.0
        var actualPoints: [ForecastPoint] = []
        for d in days where d <= todayStart {
            cumulative += series[d]?.cost ?? 0
            actualPoints.append(.init(date: d, cumulative: cumulative, kind: .actual))
        }
        spent = cumulative
        today = series[todayStart]?.cost ?? 0

        let fractionOfToday = now.timeIntervalSince(todayStart) / 86_400
        let fullDaysBefore = Double(calendar.dateComponents([.day], from: cycle.start, to: todayStart).day ?? 0)
        // Count today by how much of it has passed, but never less than a quarter day,
        // so an early-morning session doesn't make the average explode.
        elapsedDays = max(fullDaysBefore + max(fractionOfToday, 0.25), 0.25)
        remainingDays = max(totalDays - fullDaysBefore - fractionOfToday, 0)
        dailyAverage = spent / elapsedDays

        // Trailing 7 complete days plus the part of today that has passed.
        var last7 = 0.0
        for i in 1...7 {
            if let d = calendar.date(byAdding: .day, value: -i, to: todayStart) { last7 += series[d]?.cost ?? 0 }
        }
        trailing7Average = (last7 + today) / (7 + max(fractionOfToday, 0.25))

        let hasHistory = fullDaysBefore >= 3
        projectedRate = hasHistory ? 0.6 * trailing7Average + 0.4 * dailyAverage : max(trailing7Average, dailyAverage)
        projected = spent + projectedRate * remainingDays

        if budget > 0 {
            safeDailySpend = remainingDays > 0 ? max(budget - spent, 0) / remainingDays : nil
            if spent >= budget {
                status = .exceeded
                exhaustionDate = actualPoints.first { $0.cumulative >= budget }?.date
            } else if projected > budget {
                status = .willExceed
                let daysLeft = projectedRate > 0 ? (budget - spent) / projectedRate : .infinity
                exhaustionDate = daysLeft.isFinite ? now.addingTimeInterval(daysLeft * 86_400) : nil
            } else {
                status = projected >= 0.9 * budget ? .tight : .underBudget
                exhaustionDate = nil
            }
        } else {
            status = .noBudget
            safeDailySpend = nil
            exhaustionDate = nil
        }

        // Projection from end of today to cycle end, one point per day.
        var projectedPoints: [ForecastPoint] = []
        var running = spent
        if let last = actualPoints.last { projectedPoints.append(.init(date: last.date, cumulative: last.cumulative, kind: .projected)) }
        for d in days where d > todayStart {
            running += projectedRate
            projectedPoints.append(.init(date: d, cumulative: running, kind: .projected))
        }
        var pacePoints: [ForecastPoint] = []
        if budget > 0 {
            for (i, d) in days.enumerated() {
                pacePoints.append(.init(date: d, cumulative: budget * Double(i + 1) / totalDays, kind: .pace))
            }
        }
        points = actualPoints + projectedPoints + pacePoints
    }

    public var headline: String {
        let f = CurrencyFormat.self
        switch status {
        case .noBudget:
            return "Projected \(f.string(projected)) this cycle"
        case .underBudget:
            return "On track · projected \(f.string(projected)) of \(f.string(budget))"
        case .tight:
            return "Tight · projected \(f.string(projected)) of \(f.string(budget))"
        case .willExceed:
            if let d = exhaustionDate {
                return "Budget runs out ~\(d.formatted(.dateTime.month(.abbreviated).day()))"
            }
            return "Projected to exceed budget"
        case .exceeded:
            return "Over budget by \(f.string(spent - budget))"
        }
    }
}

public enum CurrencyFormat {
    /// Dollar formatting is always US style (`$1,234`) whatever the system region,
    /// since every amount here is in US dollars.
    static let locale = Locale(identifier: "en_US")

    /// `$4.21` under $100, `$1,234` above.
    public static func string(_ value: Double, compact: Bool = false) -> String {
        let v = value.isFinite ? value : 0
        if compact && abs(v) >= 10_000 {
            return "$" + String(format: "%.1fk", v / 1000)
        }
        if abs(v) >= 100 {
            return v.formatted(.currency(code: "USD").precision(.fractionLength(0)).locale(locale))
        }
        return v.formatted(.currency(code: "USD").precision(.fractionLength(2)).locale(locale))
    }

    public static func exact(_ value: Double) -> String {
        value.formatted(.currency(code: "USD").precision(.fractionLength(2...4)).locale(locale))
    }

    public static func tokens(_ n: Int) -> String {
        let d = Double(n)
        switch d {
        case 1_000_000_000...: return String(format: "%.2fB", d / 1_000_000_000)
        case 1_000_000...: return String(format: "%.1fM", d / 1_000_000)
        case 1_000...: return String(format: "%.1fK", d / 1_000)
        default: return "\(n)"
        }
    }
}

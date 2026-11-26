import Foundation

// Pure logic behind the WidgetKit views, so it can be tested without WidgetKit.

extension WidgetSnapshot {
    /// When the widget should reload: in 15 minutes, or 5 s after the next limit reset if that's sooner.
    public static func nextRefresh(for snapshot: WidgetSnapshot?, now: Date) -> Date {
        var next = now.addingTimeInterval(15 * 60)
        if let reset = snapshot?.limits.compactMap(\.resetsAt).filter({ $0 > now }).min() {
            next = min(next, reset.addingTimeInterval(5))
        }
        return next
    }

    /// The small widget's one-line forecast: `→ $468.00 of $500.00` or `→ $468.00 projected`.
    public var shortHeadline: String {
        budget > 0 ? "→ \(WidgetUSD.string(projected)) of \(WidgetUSD.string(budget))"
                   : "→ \(WidgetUSD.string(projected)) projected"
    }
}

/// How alarming a status is; each platform maps it to its own colors.
public enum StatusTone: Sendable, Hashable {
    case good, tight, willExceed, exceeded
}

extension SnapshotStatus {
    public var tone: StatusTone {
        switch self {
        case .noBudget, .underBudget: return .good
        case .tight: return .tight
        case .willExceed: return .willExceed
        case .exceeded: return .exceeded
        }
    }

    /// SF Symbol for the status (the app uses the same symbols for the core's `BudgetStatus`).
    public var symbolName: String {
        switch self {
        case .noBudget, .underBudget: return "checkmark.circle.fill"
        case .tight: return "exclamationmark.circle.fill"
        case .willExceed: return "exclamationmark.triangle.fill"
        case .exceeded: return "xmark.octagon.fill"
        }
    }
}

extension WidgetSnapshot.Limit {
    /// Ring title: `5-hour`, `Weekly`, or the label as is (e.g. a per-model limit).
    public var title: String {
        switch label {
        case "5h": return "5-hour"
        case "Week": return "Weekly"
        default: return label
        }
    }

    /// `reset` once it has passed, `4h 35m` / `12m` within a day, else weekday and hour (`Fri 3 PM`).
    public func resetText(at date: Date, calendar: Calendar = .autoupdatingCurrent,
                          locale: Locale = .autoupdatingCurrent) -> String {
        guard let resetsAt, resetsAt > date else { return "reset" }
        let secs = resetsAt.timeIntervalSince(date)
        if secs >= 86_400 {
            let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: calendar.timeZone)
            return resetsAt.formatted(style.weekday(.abbreviated).hour())
        }
        let h = Int(secs) / 3600, m = (Int(secs) % 3600) / 60
        return h > 0 ? "\(h)h \(m)m" : "\(m)m"
    }
}

/// The large widget's month grid: blank leading cells, then day numbers 1…n.
public struct MiniCalendarGrid: Sendable, Equatable {
    public var cells: [Int?]
    /// Day of the month `date` falls on (outlined in the grid).
    public var today: Int

    public init(date: Date, calendar: Calendar) {
        let month = calendar.dateInterval(of: .month, for: date)!.start
        let count = calendar.range(of: .day, in: .month, for: month)!.count
        let lead = CalendarMath.leadingBlanks(monthStart: month, calendar: calendar)
        cells = Array(repeating: nil, count: lead) + (1...count).map { Optional($0) }
        today = calendar.component(.day, from: date)
    }

    /// Denominator for the cells' color fractions (never zero).
    public static func scale(_ monthDays: [Int: Double]) -> Double {
        max(monthDays.values.max() ?? 0, 0.0001)
    }
}

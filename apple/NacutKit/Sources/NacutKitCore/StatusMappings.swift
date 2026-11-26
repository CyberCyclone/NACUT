import Foundation
import NacutCore
import NacutKit

extension SnapshotStatus {
    /// The widget's copy of the core's budget status.
    public init(_ s: BudgetStatus) {
        switch s {
        case .noBudget: self = .noBudget
        case .underBudget: self = .underBudget
        case .tight: self = .tight
        case .willExceed: self = .willExceed
        case .exceeded: self = .exceeded
        }
    }
}

extension BudgetStatus {
    /// SF Symbol for the status (the same ones the widget uses).
    public var symbolName: String { SnapshotStatus(self).symbolName }
}

extension ApiState {
    /// SF Symbol for the Usage & Cost API connection state.
    public var symbol: String {
        switch self {
        case .notConfigured: return "circle.dashed"
        case .connected: return "checkmark.circle.fill"
        case .noDataYet: return "clock.badge.checkmark"
        case .unauthorized, .forbidden: return "lock.circle.fill"
        case .unavailable: return "nosign"
        case .failed: return "exclamationmark.triangle.fill"
        }
    }
}

extension LimitView {
    /// Tooltip: `5-hour session: 42% used. resets in 4h 35m. On pace for 80% by reset.`
    public func helpText(locale: Locale = .autoupdatingCurrent) -> String {
        var s = "\(label): \(wholePercent(utilization, locale: locale)) used. \(resetText)."
        if let p = projectedAtReset { s += " On pace for \(wholePercent(p, locale: locale)) by reset." }
        return s
    }
}

extension WidgetSnapshot.Limit {
    /// The widget's copy of a plan limit.
    public init(_ l: LimitView) {
        self.init(label: l.shortLabel, utilization: l.utilization,
                  resetsAt: l.resetsAtMs.map { Date(timeIntervalSince1970: Double($0) / 1000) })
    }
}

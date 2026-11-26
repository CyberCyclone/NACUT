import Foundation
import NacutCore
import NacutKit
import Testing
@testable import NacutKitCore

// Plain enums; the generated bindings just don't declare it. Needed for parameterized tests.
extension BudgetStatus: @retroactive @unchecked Sendable {}
extension ApiState: @retroactive @unchecked Sendable {}

@Suite("Core status mappings")
struct StatusMappingsTests {
    @Test(arguments: [
        (BudgetStatus.noBudget, SnapshotStatus.noBudget, "checkmark.circle.fill"),
        (.underBudget, .underBudget, "checkmark.circle.fill"),
        (.tight, .tight, "exclamationmark.circle.fill"),
        (.willExceed, .willExceed, "exclamationmark.triangle.fill"),
        (.exceeded, .exceeded, "xmark.octagon.fill"),
    ])
    func budgetStatus(core: BudgetStatus, widget: SnapshotStatus, symbol: String) {
        #expect(SnapshotStatus(core) == widget)
        #expect(core.symbolName == symbol)
    }

    @Test(arguments: [
        (ApiState.notConfigured, "circle.dashed"), (.connected, "checkmark.circle.fill"),
        (.noDataYet, "clock.badge.checkmark"), (.unauthorized, "lock.circle.fill"), (.forbidden, "lock.circle.fill"),
        (.unavailable, "nosign"), (.failed, "exclamationmark.triangle.fill"),
    ])
    func apiState(state: ApiState, symbol: String) {
        #expect(state.symbol == symbol)
    }
}

@Suite("Plan limit view helpers")
struct LimitViewTests {
    func limit(utilization: Double = 0.42, projected: Double? = nil, resetsAtMs: Int64? = 1_795_000_000_000) -> LimitView {
        LimitView(id: "five_hour", label: "5-hour session", shortLabel: "5h", title: "5-hour session",
                  utilization: utilization, resetsAtMs: resetsAtMs, resetText: "resets in 4h 35m",
                  projectedAtReset: projected, projectedHitMs: nil, subtitle: "resets in 4h 35m", level: .normal)
    }

    let en = Locale(identifier: "en_US_POSIX")

    @Test func helpTextWithoutPace() {
        #expect(limit().helpText(locale: en) == "5-hour session: 42% used. resets in 4h 35m.")
    }

    @Test func helpTextWithPace() {
        #expect(limit(utilization: 0.5, projected: 1.234).helpText(locale: en)
                == "5-hour session: 50% used. resets in 4h 35m. On pace for 123% by reset.")
    }

    @Test func helpTextFollowsTheLocale() {
        #expect(plainSpaces(limit().helpText(locale: Locale(identifier: "fr_FR"))) == "5-hour session: 42 % used. resets in 4h 35m.")
    }

    @Test func widgetLimitFromCore() {
        let w = WidgetSnapshot.Limit(limit(resetsAtMs: 1_795_000_000_500))
        #expect(w.label == "5h")
        #expect(w.utilization == 0.42)
        #expect(w.resetsAt == Date(timeIntervalSince1970: 1_795_000_000.5))
        #expect(WidgetSnapshot.Limit(limit(resetsAtMs: nil)).resetsAt == nil)
    }
}

func plainSpaces(_ s: String) -> String {
    s.replacingOccurrences(of: "\u{202F}", with: " ").replacingOccurrences(of: "\u{00A0}", with: " ")
}

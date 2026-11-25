import AppKit
import SwiftUI
import NacutCore

extension Color {
    /// A color with separate light and dark appearances.
    init(light: UInt32, dark: UInt32) {
        self.init(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            return NSColor(hex: isDark ? dark : light)
        })
    }
}

extension NSColor {
    convenience init(hex: UInt32) {
        self.init(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
                  blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }
}

enum Theme {
    /// Claude coral, used for the primary series and accents.
    static let coral = Color(light: 0xD97757, dark: 0xE58262)
    static let coralSoft = Color(light: 0xF5C9B8, dark: 0x5C3426)

    /// Categorical slots in fixed order (validated palette). Color follows the model, never its rank.
    static let categorical: [Color] = [
        Color(light: 0x2A78D6, dark: 0x3987E5),  // blue
        Color(light: 0xEB6834, dark: 0xD95926),  // orange
        Color(light: 0x1BAF7A, dark: 0x199E70),  // aqua
        Color(light: 0xEDA100, dark: 0xC98500),  // yellow
        Color(light: 0xE87BA4, dark: 0xD55181),  // magenta
        Color(light: 0x008300, dark: 0x0CA30C),  // green
        Color(light: 0x6250D6, dark: 0x9085E9),  // violet
        Color(light: 0xE34948, dark: 0xE66767),  // red
    ]
    static let other = Color(light: 0x9A9890, dark: 0x6E6D66)

    /// Fixed model → slot registry so a model keeps its color across every view and filter.
    private static let modelSlots: [String: Int] = [
        "claude-opus-5-5": 0,
        "claude-fable-5-2": 1,
        "claude-sonnet-5-5": 2,
        "claude-haiku-5-5": 3,
        "claude-fable-5-1": 4,
        "claude-opus-5": 5,
        "claude-sonnet-5": 6,
        "claude-opus-4-8": 7,
    ]

    /// `id` is a normalized model id from the core (e.g. `claude-opus-5-5`).
    static func color(forModel id: String) -> Color {
        if let slot = modelSlots[id] { return categorical[slot] }
        return other
    }

    static func slotOrder(_ id: String) -> Int { modelSlots[id] ?? 99 }

    static func color(forModelName name: String, in stats: [ModelView]) -> Color {
        if let s = stats.first(where: { $0.name == name }) { return color(forModel: s.id) }
        return other
    }

    /// Sequential coral ramp for magnitude (calendar, heatmap), light → dark.
    static let ramp: [Color] = [
        Color(light: 0xFBE9E2, dark: 0x3A2620),
        Color(light: 0xF5C9B8, dark: 0x5C3426),
        Color(light: 0xEDA58B, dark: 0x8A4730),
        Color(light: 0xE0805F, dark: 0xBD5F3F),
        Color(light: 0xC25E3D, dark: 0xE58262),
    ]

    static func rampColor(_ fraction: Double) -> Color {
        guard fraction > 0 else { return .clear }
        let i = min(Int((fraction * Double(ramp.count)).rounded(.up)) - 1, ramp.count - 1)
        return ramp[max(i, 0)]
    }

    /// Ink that stays legible on a ramp step.
    static func inkOnRamp(_ fraction: Double) -> Color {
        fraction > 0.6 ? .white : .primary
    }

    static func statusColor(_ s: BudgetStatus) -> Color {
        switch s {
        case .noBudget, .underBudget: return Color(light: 0x0B8A4A, dark: 0x2FBF71)
        case .tight: return Color(light: 0xB86E00, dark: 0xF0A23B)
        case .willExceed: return Color(light: 0xC2410C, dark: 0xF97316)
        case .exceeded: return Color(light: 0xC0262D, dark: 0xF05252)
        }
    }

    static func statusSymbol(_ s: BudgetStatus) -> String {
        switch s {
        case .noBudget, .underBudget: return "checkmark.circle.fill"
        case .tight: return "exclamationmark.circle.fill"
        case .willExceed: return "exclamationmark.triangle.fill"
        case .exceeded: return "xmark.octagon.fill"
        }
    }

    static func levelColor(_ l: Level) -> Color {
        switch l {
        case .normal: return coral
        case .warning: return statusColor(.tight)
        case .critical: return statusColor(.exceeded)
        }
    }

    static func limitColor(_ u: Double) -> Color {
        u >= 0.9 ? statusColor(.exceeded) : u >= 0.7 ? statusColor(.tight) : coral
    }
}

/// Rounded card used throughout the panel.
struct Card<Content: View>: View {
    var title: String?
    var trailing: AnyView?
    @ViewBuilder var content: Content

    init(_ title: String? = nil, trailing: AnyView? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.trailing = trailing
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if title != nil || trailing != nil {
                HStack {
                    if let title { Text(title).font(.subheadline.weight(.semibold)) }
                    Spacer()
                    trailing
                }
            }
            content
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(.separator.opacity(0.5), lineWidth: 0.5))
    }
}

extension Double {
    var usd: String { USD.string(self) }
    var usdCompact: String {
        if self == 0 { return "$0" }
        if self < 10 { return USD.string(self) }
        if self < 1000 { return "$" + String(Int(self.rounded())) }
        return "$" + String(format: "%.1fk", self / 1000)
    }
    var percent: String { (self).formatted(.percent.precision(.fractionLength(0))) }
}

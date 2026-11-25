import SwiftUI
import UsageCore

@main
struct NACUTApp: App {
    @State private var model: AppModel

    init() {
        LegacyMigration.run()
        let m = AppModel()
        _model = State(initialValue: m)
        DebugRenderer.runIfRequested(model: m)
    }

    var body: some Scene {
        MenuBarExtra {
            PanelView()
                .environment(model)
        } label: {
            MenuBarLabel()
                .environment(model)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .environment(model)
        }
        .windowResizability(.contentSize)
    }
}

struct MenuBarLabel: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: symbol)
            if !model.hasLoaded && model.store.records.isEmpty && model.limits.isEmpty {
                Text("…")
            } else {
                Text(model.menuBarText)
            }
        }
        .monospacedDigit()
        .accessibilityLabel(model.showsLimits && !model.limits.isEmpty
            ? "Claude plan usage \(model.menuBarText)" : "Claude usage today \(model.today.usd)")
    }

    /// A gauge glyph whose needle tracks budget use (or plan limits when there's no budget).
    private var symbol: String {
        let f = model.forecast
        let worstLimit = model.limits.map { $0.effectiveUtilization() }.max()
        if model.showsLimits, let worstLimit {
            if worstLimit >= 1 { return "exclamationmark.triangle.fill" }
            return Self.gauge(worstLimit)
        }
        if f.status == .exceeded { return "exclamationmark.triangle.fill" }
        let fraction: Double
        if f.budget > 0 {
            fraction = f.budgetUsedFraction
        } else if let worst = model.limits.map(\.utilization).max() {
            fraction = worst
        } else {
            return "gauge.with.dots.needle.33percent"
        }
        return Self.gauge(fraction)
    }

    private static func gauge(_ fraction: Double) -> String {
        switch fraction {
        case ..<0.2: return "gauge.with.dots.needle.0percent"
        case ..<0.45: return "gauge.with.dots.needle.33percent"
        case ..<0.6: return "gauge.with.dots.needle.50percent"
        case ..<0.85: return "gauge.with.dots.needle.67percent"
        default: return "gauge.with.dots.needle.100percent"
        }
    }
}

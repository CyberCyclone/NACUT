import SwiftUI
import NacutCore

@main
struct NACUTApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
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

/// Handles `nacut://open` (tapping the widget) by opening the menu bar panel.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func application(_ application: NSApplication, open urls: [URL]) {
        guard urls.contains(where: { $0.scheme == "nacut" }) else { return }
        // Let the launch finish if the tap started the app.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { Self.openPanel() }
    }

    /// SwiftUI has no API to open a MenuBarExtra, so click its status item.
    @MainActor static func openPanel() {
        for window in NSApp.windows where window.className.contains("NSStatusBarWindow") {
            if let item = window.value(forKey: "statusItem") as? NSStatusItem, let button = item.button {
                button.performClick(nil)
                return
            }
        }
    }
}

struct MenuBarLabel: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: symbol)
            if !model.hasLoaded && model.recordCount == 0 && model.limits.isEmpty {
                Text("…")
            } else {
                Text(model.menuBarText)
            }
        }
        .monospacedDigit()
        .accessibilityLabel(model.showsLimits && !model.limits.isEmpty
            ? "Claude plan usage \(model.menuBarText)" : "Claude usage today \(model.today.usd)")
    }

    /// A gauge glyph whose needle tracks the worst plan limit or the budget (decided by the core).
    private var symbol: String {
        let st = model.status
        if st.showsLimits, (st.gaugeFraction ?? 0) >= 1 { return "exclamationmark.triangle.fill" }
        if !st.showsLimits, model.forecast.status == .exceeded { return "exclamationmark.triangle.fill" }
        guard let fraction = st.gaugeFraction else { return "gauge.with.dots.needle.33percent" }
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

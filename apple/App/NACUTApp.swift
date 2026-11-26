import SwiftUI
import NacutCore
import NacutKit

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
        return MenuBarGauge.symbol(showsLimits: st.showsLimits, gaugeFraction: st.gaugeFraction,
                                   budgetExceeded: model.forecast.status == .exceeded)
    }
}

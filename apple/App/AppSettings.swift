import Foundation
import NacutCore
import NacutKitCore
import Observation

/// User preferences, persisted in UserDefaults (the same keys the app has always used) and handed
/// to the shared core as a `Settings` value. Secrets live in the Keychain, never here.
/// Loading, clamping and encoding rules live in `SettingsStore` (NacutKitCore), which has tests.
@MainActor
@Observable
final class AppSettings {
    // First launch: defaults detected by the core from ~/.claude.json.
    @ObservationIgnored private let store = SettingsStore(defaults: .standard, detected: detectedSettings())
    /// Called after any change, so the model can push the new settings to the core.
    @ObservationIgnored var onChange: (() -> Void)?

    var accountMode: AccountMode { didSet { persist(.accountMode) } }
    var planPrice: Double { didSet { persist(.planPrice) } }
    var monthlyBudget: Double { didSet { persist(.monthlyBudget) } }
    var cycleStartDay: Int { didSet { persist(.cycleStartDay) } }
    var secondFigure: SecondFigure { didSet { persist(.secondFigure) } }
    var compactMenuBar: Bool { didSet { persist(.compactMenuBar) } }
    var usageDisplay: UsageDisplay { didSet { persist(.usageDisplay) } }

    var dataSource: DataSourceMode { didSet { persist(.dataSource) } }
    var apiActorId: String? { didSet { persist(.apiActorId) } }
    var apiActorName: String? { didSet { persist(.apiActorName) } }

    var limitsEnabled: Bool { didSet { persist(.limitsEnabled) } }
    var autoRefreshSignIn: Bool { didSet { persist(.autoRefreshSignIn) } }

    var alertsEnabled: Bool { didSet { persist(.alertsEnabled) } }
    var alertThresholds: [Int] { didSet { persist(.alertThresholds) } }
    var forecastAlert: Bool { didSet { persist(.forecastAlert) } }
    var limitAlerts: Bool { didSet { persist(.limitAlerts) } }

    var extraRoots: [String] { didSet { persist(.extraRoots) } }
    var priceOverrides: [String: Rates] { didSet { persist(.priceOverrides) } }

    init() {
        let s = store.load()
        accountMode = s.accountMode
        planPrice = s.planPrice
        monthlyBudget = s.monthlyBudget
        cycleStartDay = s.cycleStartDay
        secondFigure = s.secondFigure
        compactMenuBar = s.compactMenuBar
        usageDisplay = s.usageDisplay
        dataSource = s.dataSource
        apiActorId = s.apiActorId
        apiActorName = s.apiActorName
        limitsEnabled = s.limitsEnabled
        autoRefreshSignIn = s.autoRefreshSignIn
        alertsEnabled = s.alertsEnabled
        alertThresholds = s.alertThresholds
        forecastAlert = s.forecastAlert
        limitAlerts = s.limitAlerts
        extraRoots = s.extraRoots
        priceOverrides = s.priceOverrides
    }

    private var stored: StoredSettings {
        StoredSettings(
            accountMode: accountMode, planPrice: planPrice, monthlyBudget: monthlyBudget, cycleStartDay: cycleStartDay,
            secondFigure: secondFigure, compactMenuBar: compactMenuBar, usageDisplay: usageDisplay, dataSource: dataSource,
            apiActorId: apiActorId, apiActorName: apiActorName, limitsEnabled: limitsEnabled,
            autoRefreshSignIn: autoRefreshSignIn, alertsEnabled: alertsEnabled, alertThresholds: alertThresholds,
            forecastAlert: forecastAlert, limitAlerts: limitAlerts, extraRoots: extraRoots, priceOverrides: priceOverrides)
    }

    private func persist(_ key: SettingsKey) {
        store.save(key, from: stored)
        onChange?()
    }

    /// These settings as the core's value type.
    var core: Settings { stored.core }

    /// Takes changes the core made (e.g. switching to Hybrid after connecting an API key).
    func absorb(_ s: Settings) {
        let saved = onChange
        onChange = nil
        defer { onChange = saved }
        if dataSource != s.dataSource { dataSource = s.dataSource }
        if apiActorId != s.apiActorId { apiActorId = s.apiActorId }
        if apiActorName != s.apiActorName { apiActorName = s.apiActorName }
        if limitsEnabled != s.limitsEnabled { limitsEnabled = s.limitsEnabled }
        if usageDisplay != s.usageDisplay { usageDisplay = s.usageDisplay }
    }
}

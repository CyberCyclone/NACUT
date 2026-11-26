import Foundation
import NacutCore
import Observation

// Titles and persistence helpers for the core's settings enums.

extension AccountMode: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [AccountMode] { [.subscription, .api] }
    public var id: String { key }
    var key: String { self == .subscription ? "subscription" : "api" }
    init?(key: String) { self.init(matching: key, in: Self.allCases) }
    var title: String { self == .subscription ? "Max / Pro subscription" : "API pay-as-you-go" }
}

extension UsageDisplay: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [UsageDisplay] { [.limits, .cost] }
    public var id: String { key }
    var key: String { self == .limits ? "limits" : "cost" }
    init?(key: String) { self.init(matching: key, in: Self.allCases) }
    var title: String { self == .limits ? "Plan limits" : "API value" }
}

extension SecondFigure: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [SecondFigure] { [.cycle, .allTime, .none] }
    public var id: String { key }
    var key: String {
        switch self {
        case .cycle: return "cycle"
        case .allTime: return "allTime"
        case .none: return "none"
        }
    }
    init?(key: String) { self.init(matching: key, in: Self.allCases) }
    var title: String {
        switch self {
        case .cycle: return "This cycle"
        case .allTime: return "All time"
        case .none: return "Hidden"
        }
    }
}

extension DataSourceMode: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [DataSourceMode] { [.logs, .api, .hybrid] }
    public var id: String { key }
    var key: String {
        switch self {
        case .logs: return "logs"
        case .api: return "api"
        case .hybrid: return "hybrid"
        }
    }
    init?(key: String) { self.init(matching: key, in: Self.allCases) }
    var title: String {
        switch self {
        case .logs: return "Local logs"
        case .api: return "Anthropic API only"
        case .hybrid: return "Hybrid (API + live logs)"
        }
    }
}

private protocol KeyedCase: Identifiable where ID == String {}
extension AccountMode: KeyedCase {}
extension UsageDisplay: KeyedCase {}
extension SecondFigure: KeyedCase {}
extension DataSourceMode: KeyedCase {}

private extension KeyedCase {
    init?(matching key: String, in all: [Self]) {
        guard let v = all.first(where: { $0.id == key }) else { return nil }
        self = v
    }
}

/// User preferences, persisted in UserDefaults (the same keys the app has always used) and handed
/// to the shared core as a `Settings` value. Secrets live in the Keychain, never here.
@MainActor
@Observable
final class AppSettings {
    private let d = UserDefaults.standard
    /// Called after any change, so the model can push the new settings to the core.
    @ObservationIgnored var onChange: (() -> Void)?

    var accountMode: AccountMode { didSet { d.set(accountMode.key, forKey: "accountMode"); onChange?() } }
    var planPrice: Double { didSet { d.set(planPrice, forKey: "planPrice"); onChange?() } }
    var monthlyBudget: Double { didSet { d.set(monthlyBudget, forKey: "monthlyBudget"); onChange?() } }
    var cycleStartDay: Int { didSet { d.set(cycleStartDay, forKey: "cycleStartDay"); onChange?() } }
    var secondFigure: SecondFigure { didSet { d.set(secondFigure.key, forKey: "secondFigure"); onChange?() } }
    var compactMenuBar: Bool { didSet { d.set(compactMenuBar, forKey: "compactMenuBar"); onChange?() } }
    var usageDisplay: UsageDisplay { didSet { d.set(usageDisplay.key, forKey: "usageDisplay"); onChange?() } }

    var dataSource: DataSourceMode { didSet { d.set(dataSource.key, forKey: "dataSource"); onChange?() } }
    var apiActorId: String? { didSet { d.set(apiActorId, forKey: "apiActorId"); onChange?() } }
    var apiActorName: String? { didSet { d.set(apiActorName, forKey: "apiActorName"); onChange?() } }

    var limitsEnabled: Bool { didSet { d.set(limitsEnabled, forKey: "limitsEnabled"); onChange?() } }
    var autoRefreshSignIn: Bool { didSet { d.set(autoRefreshSignIn, forKey: "autoRefreshSignIn"); onChange?() } }

    var alertsEnabled: Bool { didSet { d.set(alertsEnabled, forKey: "alertsEnabled"); onChange?() } }
    var alertThresholds: [Int] { didSet { d.set(alertThresholds, forKey: "alertThresholds"); onChange?() } }
    var forecastAlert: Bool { didSet { d.set(forecastAlert, forKey: "forecastAlert"); onChange?() } }
    var limitAlerts: Bool { didSet { d.set(limitAlerts, forKey: "limitAlerts"); onChange?() } }

    var extraRoots: [String] { didSet { d.set(extraRoots, forKey: "extraRoots"); onChange?() } }
    var priceOverrides: [String: Rates] {
        didSet {
            let plain = priceOverrides.mapValues { [$0.input, $0.output, $0.write5m, $0.write1h, $0.read] }
            d.set(try? JSONEncoder().encode(plain), forKey: "priceOverridesV2")
            onChange?()
        }
    }

    init() {
        // First launch: defaults detected by the core from ~/.claude.json.
        let detected = detectedSettings()
        d.register(defaults: [
            "accountMode": detected.accountMode.key,
            "planPrice": detected.planPrice,
            "monthlyBudget": 0.0,
            "cycleStartDay": 1,
            "secondFigure": SecondFigure.cycle.key,
            "usageDisplay": UsageDisplay.limits.key,
            "dataSource": DataSourceMode.logs.key,
            "limitsEnabled": detected.limitsEnabled,
            "autoRefreshSignIn": true,
            "alertsEnabled": true,
            "alertThresholds": [50, 80, 100],
            "forecastAlert": true,
            "limitAlerts": true,
        ])
        accountMode = AccountMode(key: d.string(forKey: "accountMode") ?? "") ?? .api
        planPrice = d.double(forKey: "planPrice")
        monthlyBudget = d.double(forKey: "monthlyBudget")
        cycleStartDay = min(max(d.integer(forKey: "cycleStartDay"), 1), 28)
        secondFigure = SecondFigure(key: d.string(forKey: "secondFigure") ?? "") ?? .cycle
        compactMenuBar = d.bool(forKey: "compactMenuBar")
        usageDisplay = UsageDisplay(key: d.string(forKey: "usageDisplay") ?? "") ?? .limits
        dataSource = DataSourceMode(key: d.string(forKey: "dataSource") ?? "") ?? .logs
        apiActorId = d.string(forKey: "apiActorId")
        apiActorName = d.string(forKey: "apiActorName")
        limitsEnabled = d.bool(forKey: "limitsEnabled")
        autoRefreshSignIn = d.bool(forKey: "autoRefreshSignIn")
        alertsEnabled = d.bool(forKey: "alertsEnabled")
        alertThresholds = (d.array(forKey: "alertThresholds") as? [Int]) ?? [50, 80, 100]
        forecastAlert = d.bool(forKey: "forecastAlert")
        limitAlerts = d.bool(forKey: "limitAlerts")
        extraRoots = d.stringArray(forKey: "extraRoots") ?? []
        let plain = d.data(forKey: "priceOverridesV2").flatMap { try? JSONDecoder().decode([String: [Double]].self, from: $0) } ?? [:]
        priceOverrides = plain.compactMapValues { v in
            v.count == 5 ? Rates(input: v[0], output: v[1], write5m: v[2], write1h: v[3], read: v[4]) : nil
        }
    }

    /// These settings as the core's value type.
    var core: Settings {
        Settings(
            accountMode: accountMode, planPrice: planPrice, monthlyBudget: monthlyBudget,
            cycleStartDay: UInt8(min(max(cycleStartDay, 1), 28)), secondFigure: secondFigure,
            compactMenuBar: compactMenuBar, usageDisplay: usageDisplay, dataSource: dataSource,
            apiActorId: apiActorId, apiActorName: apiActorName, limitsEnabled: limitsEnabled,
            autoRefreshSignIn: autoRefreshSignIn, alertsEnabled: alertsEnabled, alertThresholds: alertThresholds.map { UInt32(max($0, 0)) },
            forecastAlert: forecastAlert, limitAlerts: limitAlerts, extraRoots: extraRoots,
            includeWsl: false, priceOverrides: priceOverrides)
    }

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

import Foundation
import NacutCore

/// The Mac app's preferences as plain values. The app keeps them in UserDefaults (see
/// `SettingsStore`) and hands them to the core as a `Settings` value. Secrets live in the Keychain,
/// never here.
public struct StoredSettings: Equatable {
    public var accountMode: AccountMode
    public var planPrice: Double
    public var monthlyBudget: Double
    public var cycleStartDay: Int
    public var secondFigure: SecondFigure
    public var compactMenuBar: Bool
    public var usageDisplay: UsageDisplay
    public var dataSource: DataSourceMode
    public var apiActorId: String?
    public var apiActorName: String?
    public var limitsEnabled: Bool
    public var autoRefreshSignIn: Bool
    public var alertsEnabled: Bool
    public var alertThresholds: [Int]
    public var forecastAlert: Bool
    public var limitAlerts: Bool
    public var extraRoots: [String]
    public var priceOverrides: [String: Rates]

    public init(
        accountMode: AccountMode, planPrice: Double, monthlyBudget: Double, cycleStartDay: Int,
        secondFigure: SecondFigure, compactMenuBar: Bool, usageDisplay: UsageDisplay, dataSource: DataSourceMode,
        apiActorId: String?, apiActorName: String?, limitsEnabled: Bool, autoRefreshSignIn: Bool,
        alertsEnabled: Bool, alertThresholds: [Int], forecastAlert: Bool, limitAlerts: Bool,
        extraRoots: [String], priceOverrides: [String: Rates]
    ) {
        self.accountMode = accountMode
        self.planPrice = planPrice
        self.monthlyBudget = monthlyBudget
        self.cycleStartDay = cycleStartDay
        self.secondFigure = secondFigure
        self.compactMenuBar = compactMenuBar
        self.usageDisplay = usageDisplay
        self.dataSource = dataSource
        self.apiActorId = apiActorId
        self.apiActorName = apiActorName
        self.limitsEnabled = limitsEnabled
        self.autoRefreshSignIn = autoRefreshSignIn
        self.alertsEnabled = alertsEnabled
        self.alertThresholds = alertThresholds
        self.forecastAlert = forecastAlert
        self.limitAlerts = limitAlerts
        self.extraRoots = extraRoots
        self.priceOverrides = priceOverrides
    }

    /// These settings as the core's value type (cycle day clamped to 1…28, negative thresholds to 0).
    public var core: Settings {
        Settings(
            accountMode: accountMode, planPrice: planPrice, monthlyBudget: monthlyBudget,
            cycleStartDay: UInt8(SettingsStore.clampCycleDay(cycleStartDay)), secondFigure: secondFigure,
            compactMenuBar: compactMenuBar, usageDisplay: usageDisplay, dataSource: dataSource,
            apiActorId: apiActorId, apiActorName: apiActorName, limitsEnabled: limitsEnabled,
            autoRefreshSignIn: autoRefreshSignIn, alertsEnabled: alertsEnabled,
            alertThresholds: alertThresholds.map { UInt32(clamping: $0) },
            forecastAlert: forecastAlert, limitAlerts: limitAlerts, extraRoots: extraRoots,
            includeWsl: false, priceOverrides: priceOverrides)
    }
}

/// UserDefaults keys (the same ones the app has always used).
public enum SettingsKey: String, CaseIterable, Sendable {
    case accountMode, planPrice, monthlyBudget, cycleStartDay, secondFigure, compactMenuBar, usageDisplay
    case dataSource, apiActorId, apiActorName, limitsEnabled, autoRefreshSignIn
    case alertsEnabled, alertThresholds, forecastAlert, limitAlerts, extraRoots
    /// `[model id: [input, output, write5m, write1h, read]]` as JSON data.
    case priceOverrides = "priceOverridesV2"
}

/// Reads and writes `StoredSettings` in a UserDefaults domain.
///
/// Keys that were never saved read as the first-launch values. (This used to go through
/// `UserDefaults.register(defaults:)`, whose registration domain is shared by every UserDefaults
/// in the process, so it couldn't be tested in isolation.)
public struct SettingsStore {
    public let defaults: UserDefaults
    /// What a key reads as until it's first saved.
    public let firstLaunch: StoredSettings

    public static let defaultThresholds = [50, 80, 100]

    /// `detected` comes from the core's `detectedSettings()` (account mode, plan price and plan
    /// limits from `~/.claude.json`).
    public init(defaults: UserDefaults, detected: Settings) {
        self.defaults = defaults
        firstLaunch = Self.firstLaunch(detected: detected)
    }

    public static func firstLaunch(detected: Settings) -> StoredSettings {
        StoredSettings(
            accountMode: detected.accountMode, planPrice: detected.planPrice, monthlyBudget: 0, cycleStartDay: 1,
            secondFigure: .cycle, compactMenuBar: false, usageDisplay: .limits, dataSource: .logs,
            apiActorId: nil, apiActorName: nil, limitsEnabled: detected.limitsEnabled, autoRefreshSignIn: true,
            alertsEnabled: true, alertThresholds: defaultThresholds, forecastAlert: true, limitAlerts: true,
            extraRoots: [], priceOverrides: [:])
    }

    static func clampCycleDay(_ day: Int) -> Int { min(max(day, 1), 28) }

    /// Saved values, or the first-launch value for keys never saved. Unknown enum keys fall back
    /// to a default, the cycle day is clamped to 1…28, unreadable thresholds become [50, 80, 100],
    /// and price overrides without exactly five rates are dropped.
    public func load() -> StoredSettings {
        let d = defaults
        let f = firstLaunch
        func saved<T>(_ k: SettingsKey, _ fallback: T, _ read: (String) -> T) -> T {
            d.object(forKey: k.rawValue) == nil ? fallback : read(k.rawValue)
        }
        func string(_ k: String) -> String { d.string(forKey: k) ?? "" }
        return StoredSettings(
            accountMode: saved(.accountMode, f.accountMode) { AccountMode(key: string($0)) ?? .api },
            planPrice: saved(.planPrice, f.planPrice, d.double(forKey:)),
            monthlyBudget: saved(.monthlyBudget, f.monthlyBudget, d.double(forKey:)),
            cycleStartDay: saved(.cycleStartDay, f.cycleStartDay) { Self.clampCycleDay(d.integer(forKey: $0)) },
            secondFigure: saved(.secondFigure, f.secondFigure) { SecondFigure(key: string($0)) ?? .cycle },
            compactMenuBar: saved(.compactMenuBar, f.compactMenuBar, d.bool(forKey:)),
            usageDisplay: saved(.usageDisplay, f.usageDisplay) { UsageDisplay(key: string($0)) ?? .limits },
            dataSource: saved(.dataSource, f.dataSource) { DataSourceMode(key: string($0)) ?? .logs },
            apiActorId: saved(.apiActorId, f.apiActorId, d.string(forKey:)),
            apiActorName: saved(.apiActorName, f.apiActorName, d.string(forKey:)),
            limitsEnabled: saved(.limitsEnabled, f.limitsEnabled, d.bool(forKey:)),
            autoRefreshSignIn: saved(.autoRefreshSignIn, f.autoRefreshSignIn, d.bool(forKey:)),
            alertsEnabled: saved(.alertsEnabled, f.alertsEnabled, d.bool(forKey:)),
            alertThresholds: saved(.alertThresholds, f.alertThresholds) { (d.array(forKey: $0) as? [Int]) ?? Self.defaultThresholds },
            forecastAlert: saved(.forecastAlert, f.forecastAlert, d.bool(forKey:)),
            limitAlerts: saved(.limitAlerts, f.limitAlerts, d.bool(forKey:)),
            extraRoots: saved(.extraRoots, f.extraRoots) { d.stringArray(forKey: $0) ?? [] },
            priceOverrides: saved(.priceOverrides, f.priceOverrides) { k in
                let plain = d.data(forKey: k).flatMap { try? JSONDecoder().decode([String: [Double]].self, from: $0) } ?? [:]
                return plain.compactMapValues { v in
                    v.count == 5 ? Rates(input: v[0], output: v[1], write5m: v[2], write1h: v[3], read: v[4]) : nil
                }
            })
    }

    /// Writes one setting from `s` (a `nil` actor removes the key).
    public func save(_ key: SettingsKey, from s: StoredSettings) {
        let d = defaults
        let k = key.rawValue
        switch key {
        case .accountMode: d.set(s.accountMode.key, forKey: k)
        case .planPrice: d.set(s.planPrice, forKey: k)
        case .monthlyBudget: d.set(s.monthlyBudget, forKey: k)
        case .cycleStartDay: d.set(s.cycleStartDay, forKey: k)
        case .secondFigure: d.set(s.secondFigure.key, forKey: k)
        case .compactMenuBar: d.set(s.compactMenuBar, forKey: k)
        case .usageDisplay: d.set(s.usageDisplay.key, forKey: k)
        case .dataSource: d.set(s.dataSource.key, forKey: k)
        case .apiActorId: d.set(s.apiActorId, forKey: k)
        case .apiActorName: d.set(s.apiActorName, forKey: k)
        case .limitsEnabled: d.set(s.limitsEnabled, forKey: k)
        case .autoRefreshSignIn: d.set(s.autoRefreshSignIn, forKey: k)
        case .alertsEnabled: d.set(s.alertsEnabled, forKey: k)
        case .alertThresholds: d.set(s.alertThresholds, forKey: k)
        case .forecastAlert: d.set(s.forecastAlert, forKey: k)
        case .limitAlerts: d.set(s.limitAlerts, forKey: k)
        case .extraRoots: d.set(s.extraRoots, forKey: k)
        case .priceOverrides:
            let plain = s.priceOverrides.mapValues { [$0.input, $0.output, $0.write5m, $0.write1h, $0.read] }
            d.set(try? JSONEncoder().encode(plain), forKey: k)
        }
    }
}

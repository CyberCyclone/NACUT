import Foundation
import NacutCore
import Testing
@testable import NacutKitCore

/// A throwaway UserDefaults domain, never the app's real one. Call `remove()` when done.
/// The suite name is a path, so its plist lives in a temp folder rather than ~/Library/Preferences.
struct TestDefaults {
    let dir = FileManager.default.temporaryDirectory.appending(path: "nacut-tests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let defaults: UserDefaults

    init() {
        try! FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defaults = UserDefaults(suiteName: dir.appending(path: "prefs").path)!
    }

    func remove() {
        defaults.removePersistentDomain(forName: dir.appending(path: "prefs").path)
        try? FileManager.default.removeItem(at: dir)
    }
}

func detected(_ mode: AccountMode = .subscription, planPrice: Double = 100, limitsEnabled: Bool = true) -> Settings {
    Settings(accountMode: mode, planPrice: planPrice, monthlyBudget: 0, cycleStartDay: 1, secondFigure: .cycle,
             compactMenuBar: false, usageDisplay: .limits, dataSource: .logs, apiActorId: nil, apiActorName: nil,
             limitsEnabled: limitsEnabled, autoRefreshSignIn: true, alertsEnabled: true, alertThresholds: [50, 80, 100],
             forecastAlert: true, limitAlerts: true, extraRoots: [], includeWsl: false, priceOverrides: [:])
}

@Suite("Settings persistence")
struct SettingsStoreTests {
    @Test func firstLaunchUsesRegisteredDefaults() {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected(.subscription, planPrice: 200, limitsEnabled: true))
        let s = store.load()
        #expect(s.accountMode == .subscription)
        #expect(s.planPrice == 200)
        #expect(s.monthlyBudget == 0)
        #expect(s.cycleStartDay == 1)
        #expect(s.secondFigure == .cycle)
        #expect(s.compactMenuBar == false)
        #expect(s.usageDisplay == .limits)
        #expect(s.dataSource == .logs)
        #expect(s.apiActorId == nil)
        #expect(s.apiActorName == nil)
        #expect(s.limitsEnabled)
        #expect(s.autoRefreshSignIn)
        #expect(s.alertsEnabled)
        #expect(s.alertThresholds == [50, 80, 100])
        #expect(s.forecastAlert)
        #expect(s.limitAlerts)
        #expect(s.extraRoots.isEmpty)
        #expect(s.priceOverrides.isEmpty)
    }

    @Test func detectedApiAccount() {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected(.api, planPrice: 0, limitsEnabled: false))
        let s = store.load()
        #expect(s.accountMode == .api)
        #expect(s.planPrice == 0)
        #expect(!s.limitsEnabled)
    }

    @Test func savedValuesOfTheWrongTypeUseTheOldFallbacks() {
        let t = TestDefaults()
        defer { t.remove() }
        t.defaults.set(["x"], forKey: "accountMode")
        t.defaults.set("abc", forKey: "cycleStartDay")
        t.defaults.set("abc", forKey: "alertsEnabled")
        t.defaults.set("abc", forKey: "planPrice")
        t.defaults.set(5, forKey: "extraRoots")
        let s = SettingsStore(defaults: t.defaults, detected: detected(.subscription, planPrice: 100)).load()
        #expect(s.accountMode == .api)
        #expect(s.cycleStartDay == 1)
        #expect(!s.alertsEnabled)
        #expect(s.planPrice == 0)
        #expect(s.extraRoots == [])
    }

    @Test func firstLaunchMatchesTheOldRegisteredDefaults() {
        let f = SettingsStore.firstLaunch(detected: detected(.api, planPrice: 0, limitsEnabled: false))
        #expect(f == StoredSettings(
            accountMode: .api, planPrice: 0, monthlyBudget: 0, cycleStartDay: 1, secondFigure: .cycle,
            compactMenuBar: false, usageDisplay: .limits, dataSource: .logs, apiActorId: nil, apiActorName: nil,
            limitsEnabled: false, autoRefreshSignIn: true, alertsEnabled: true, alertThresholds: [50, 80, 100],
            forecastAlert: true, limitAlerts: true, extraRoots: [], priceOverrides: [:]))
    }

    @Test func savedValuesWinOverFirstLaunchValues() {
        let t = TestDefaults()
        defer { t.remove() }
        t.defaults.set("api", forKey: "accountMode")
        t.defaults.set(false, forKey: "limitsEnabled")
        let store = SettingsStore(defaults: t.defaults, detected: detected(.subscription, limitsEnabled: true))
        let s = store.load()
        #expect(s.accountMode == .api)
        #expect(!s.limitsEnabled)
    }

    @Test(arguments: [(-3, 1), (0, 1), (1, 1), (15, 15), (28, 28), (29, 28), (31, 28), (Int.max, 28)])
    func cycleStartDayIsClamped(stored: Int, expected: Int) {
        let t = TestDefaults()
        defer { t.remove() }
        t.defaults.set(stored, forKey: "cycleStartDay")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().cycleStartDay == expected)
    }

    @Test func unknownEnumKeysFallBack() {
        let t = TestDefaults()
        defer { t.remove() }
        for key in ["accountMode", "secondFigure", "usageDisplay", "dataSource"] {
            t.defaults.set("bogus", forKey: key)
        }
        let s = SettingsStore(defaults: t.defaults, detected: detected()).load()
        #expect(s.accountMode == .api)
        #expect(s.secondFigure == .cycle)
        #expect(s.usageDisplay == .limits)
        #expect(s.dataSource == .logs)
    }

    @Test func enumKeysAreCaseSensitive() {
        let t = TestDefaults()
        defer { t.remove() }
        t.defaults.set("Subscription", forKey: "accountMode")
        t.defaults.set("alltime", forKey: "secondFigure")
        let s = SettingsStore(defaults: t.defaults, detected: detected()).load()
        #expect(s.accountMode == .api)
        #expect(s.secondFigure == .cycle)
    }

    @Test func unreadableThresholdsFallBackButEmptyStaysEmpty() {
        let t = TestDefaults()
        defer { t.remove() }
        t.defaults.set(["fifty"], forKey: "alertThresholds")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().alertThresholds == [50, 80, 100])
        t.defaults.set("50", forKey: "alertThresholds")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().alertThresholds == [50, 80, 100])
        t.defaults.set([Int](), forKey: "alertThresholds")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().alertThresholds == [])
        t.defaults.set([90, 50], forKey: "alertThresholds")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().alertThresholds == [90, 50])
    }

    @Test func priceOverridesKeepOnlyFiveValueEntries() throws {
        let t = TestDefaults()
        defer { t.remove() }
        let plain: [String: [Double]] = [
            "claude-opus-5-5": [5, 25, 6.25, 10, 0.5],
            "short": [1, 2, 3, 4],
            "long": [1, 2, 3, 4, 5, 6],
            "empty": [],
        ]
        t.defaults.set(try JSONEncoder().encode(plain), forKey: "priceOverridesV2")
        let s = SettingsStore(defaults: t.defaults, detected: detected()).load()
        #expect(s.priceOverrides == ["claude-opus-5-5": Rates(input: 5, output: 25, write5m: 6.25, write1h: 10, read: 0.5)])
    }

    @Test func corruptPriceOverridesAreIgnored() {
        let t = TestDefaults()
        defer { t.remove() }
        t.defaults.set(Data("not json".utf8), forKey: "priceOverridesV2")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().priceOverrides.isEmpty)
        t.defaults.set("a string", forKey: "priceOverridesV2")
        #expect(SettingsStore(defaults: t.defaults, detected: detected()).load().priceOverrides.isEmpty)
    }

    @Test func everyKeyRoundTrips() {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected())
        let changed = StoredSettings(
            accountMode: .subscription, planPrice: 200, monthlyBudget: 750, cycleStartDay: 17,
            secondFigure: .allTime, compactMenuBar: true, usageDisplay: .cost, dataSource: .hybrid,
            apiActorId: "user_123", apiActorName: "Pat", limitsEnabled: true, autoRefreshSignIn: false,
            alertsEnabled: false, alertThresholds: [80, 90], forecastAlert: false, limitAlerts: false,
            extraRoots: ["/a", "/b c"],
            priceOverrides: ["claude-opus-5-5": Rates(input: 1, output: 2, write5m: 3, write1h: 4, read: 0.1)])
        for key in SettingsKey.allCases { store.save(key, from: changed) }
        #expect(store.load() == changed)
    }

    @Test func savesOnlyTheGivenKey() {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected())
        var s = store.load()
        s.monthlyBudget = 300
        s.planPrice = 20
        store.save(.monthlyBudget, from: s)
        #expect(t.defaults.double(forKey: "monthlyBudget") == 300)
        #expect(t.defaults.object(forKey: "planPrice") == nil)
    }

    @Test func wireFormat() throws {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected())
        var s = store.load()
        s.secondFigure = .allTime
        s.dataSource = .hybrid
        s.accountMode = .subscription
        s.usageDisplay = .cost
        s.priceOverrides = ["m": Rates(input: 1, output: 2, write5m: 3, write1h: 4, read: 5)]
        for key in SettingsKey.allCases { store.save(key, from: s) }
        #expect(t.defaults.string(forKey: "secondFigure") == "allTime")
        #expect(t.defaults.string(forKey: "dataSource") == "hybrid")
        #expect(t.defaults.string(forKey: "accountMode") == "subscription")
        #expect(t.defaults.string(forKey: "usageDisplay") == "cost")
        let data = try #require(t.defaults.data(forKey: "priceOverridesV2"))
        #expect(try JSONDecoder().decode([String: [Double]].self, from: data) == ["m": [1, 2, 3, 4, 5]])
    }

    @Test func clearingTheActorRemovesTheKeys() {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected())
        var s = store.load()
        s.apiActorId = "user_1"
        s.apiActorName = "Pat"
        store.save(.apiActorId, from: s)
        store.save(.apiActorName, from: s)
        #expect(t.defaults.string(forKey: "apiActorId") == "user_1")
        s.apiActorId = nil
        s.apiActorName = nil
        store.save(.apiActorId, from: s)
        store.save(.apiActorName, from: s)
        #expect(t.defaults.object(forKey: "apiActorId") == nil)
        #expect(t.defaults.object(forKey: "apiActorName") == nil)
    }

    @Test func keyNamesAreStable() {
        #expect(SettingsKey.allCases.map(\.rawValue) == [
            "accountMode", "planPrice", "monthlyBudget", "cycleStartDay", "secondFigure", "compactMenuBar",
            "usageDisplay", "dataSource", "apiActorId", "apiActorName", "limitsEnabled", "autoRefreshSignIn",
            "alertsEnabled", "alertThresholds", "forecastAlert", "limitAlerts", "extraRoots", "priceOverridesV2",
        ])
    }
}

@Suite("Settings → core")
struct CoreSettingsTests {
    func stored() -> StoredSettings {
        let t = TestDefaults()
        defer { t.remove() }
        let store = SettingsStore(defaults: t.defaults, detected: detected())
        return store.load()
    }

    @Test func carriesEveryField() {
        var s = stored()
        s.planPrice = 200
        s.monthlyBudget = 50
        s.cycleStartDay = 12
        s.secondFigure = .none
        s.compactMenuBar = true
        s.usageDisplay = .cost
        s.dataSource = .api
        s.apiActorId = "id"
        s.apiActorName = "name"
        s.extraRoots = ["/x"]
        s.alertThresholds = [50, 90]
        s.priceOverrides = ["m": Rates(input: 1, output: 2, write5m: 3, write1h: 4, read: 5)]
        let c = s.core
        #expect(c.accountMode == .subscription)
        #expect(c.planPrice == 200)
        #expect(c.monthlyBudget == 50)
        #expect(c.cycleStartDay == 12)
        #expect(c.secondFigure == SecondFigure.none)
        #expect(c.compactMenuBar)
        #expect(c.usageDisplay == .cost)
        #expect(c.dataSource == .api)
        #expect(c.apiActorId == "id")
        #expect(c.apiActorName == "name")
        #expect(c.limitsEnabled)
        #expect(c.alertThresholds == [50, 90])
        #expect(c.extraRoots == ["/x"])
        #expect(c.includeWsl == false)
        #expect(c.priceOverrides == s.priceOverrides)
    }

    @Test(arguments: [(-1, 1), (0, 1), (28, 28), (29, 28), (1000, 28)] as [(Int, UInt8)])
    func cycleDayIsClamped(day: Int, expected: UInt8) {
        var s = stored()
        s.cycleStartDay = day
        #expect(s.core.cycleStartDay == expected)
    }

    @Test func thresholdsAreClampedToUInt32() {
        var s = stored()
        s.alertThresholds = [-5, 0, 80, Int(UInt32.max) + 10]
        #expect(s.core.alertThresholds == [0, 0, 80, UInt32.max])
    }
}

@Suite("Settings enum keys and titles")
struct SettingsEnumTests {
    @Test func accountMode() {
        #expect(AccountMode.allCases.map(\.key) == ["subscription", "api"])
        #expect(AccountMode.allCases.map(\.title) == ["Max / Pro subscription", "API pay-as-you-go"])
        for m in AccountMode.allCases { #expect(AccountMode(key: m.key) == m && m.id == m.key) }
        #expect(AccountMode(key: "") == nil)
    }

    @Test func usageDisplay() {
        #expect(UsageDisplay.allCases.map(\.key) == ["limits", "cost"])
        #expect(UsageDisplay.allCases.map(\.title) == ["Plan limits", "API value"])
        for m in UsageDisplay.allCases { #expect(UsageDisplay(key: m.key) == m && m.id == m.key) }
        #expect(UsageDisplay(key: "Limits") == nil)
    }

    @Test func secondFigure() {
        #expect(SecondFigure.allCases.map(\.key) == ["cycle", "allTime", "none"])
        #expect(SecondFigure.allCases.map(\.title) == ["This cycle", "All time", "Hidden"])
        for m in SecondFigure.allCases { #expect(SecondFigure(key: m.key) == m && m.id == m.key) }
        #expect(SecondFigure(key: "all_time") == nil)
    }

    @Test func dataSourceMode() {
        #expect(DataSourceMode.allCases.map(\.key) == ["logs", "api", "hybrid"])
        #expect(DataSourceMode.allCases.map(\.title) == ["Local logs", "Anthropic API only", "Hybrid (API + live logs)"])
        for m in DataSourceMode.allCases { #expect(DataSourceMode(key: m.key) == m && m.id == m.key) }
        #expect(DataSourceMode(key: "Hybrid") == nil)
    }
}

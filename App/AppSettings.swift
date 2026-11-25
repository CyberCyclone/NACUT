import Foundation
import Observation
import UsageCore

enum AccountMode: String, CaseIterable, Identifiable, Codable {
    case subscription, api
    var id: String { rawValue }
    var title: String {
        switch self {
        case .subscription: return "Max / Pro subscription"
        case .api: return "API pay-as-you-go"
        }
    }
    var costNoun: String { self == .subscription ? "API value" : "Spend" }
}

/// What a subscription's menu bar, panel header and widget lead with.
enum UsageDisplay: String, CaseIterable, Identifiable {
    case limits, cost
    var id: String { rawValue }
    var title: String { self == .limits ? "Plan limits" : "API value" }
}

enum SecondFigure: String, CaseIterable, Identifiable {
    case cycle, allTime, none
    var id: String { rawValue }
    var title: String {
        switch self {
        case .cycle: return "This cycle"
        case .allTime: return "All time"
        case .none: return "Hidden"
        }
    }
}

/// User preferences, persisted in UserDefaults. Secrets live in the Keychain, never here.
@MainActor
@Observable
final class AppSettings {
    private let d = UserDefaults.standard

    var accountMode: AccountMode { didSet { d.set(accountMode.rawValue, forKey: "accountMode") } }
    var planPrice: Double { didSet { d.set(planPrice, forKey: "planPrice") } }
    var monthlyBudget: Double { didSet { d.set(monthlyBudget, forKey: "monthlyBudget") } }
    var cycleStartDay: Int { didSet { d.set(cycleStartDay, forKey: "cycleStartDay") } }
    var secondFigure: SecondFigure { didSet { d.set(secondFigure.rawValue, forKey: "secondFigure") } }
    var compactMenuBar: Bool { didSet { d.set(compactMenuBar, forKey: "compactMenuBar") } }
    var usageDisplay: UsageDisplay { didSet { d.set(usageDisplay.rawValue, forKey: "usageDisplay") } }

    var dataSource: DataSourceMode { didSet { d.set(dataSource.rawValue, forKey: "dataSource") } }
    var apiActorId: String? { didSet { d.set(apiActorId, forKey: "apiActorId") } }
    var apiActorName: String? { didSet { d.set(apiActorName, forKey: "apiActorName") } }

    var limitsEnabled: Bool { didSet { d.set(limitsEnabled, forKey: "limitsEnabled") } }

    var alertsEnabled: Bool { didSet { d.set(alertsEnabled, forKey: "alertsEnabled") } }
    var alertThresholds: [Int] { didSet { d.set(alertThresholds, forKey: "alertThresholds") } }
    var forecastAlert: Bool { didSet { d.set(forecastAlert, forKey: "forecastAlert") } }
    var limitAlerts: Bool { didSet { d.set(limitAlerts, forKey: "limitAlerts") } }

    var extraRoots: [String] { didSet { d.set(extraRoots, forKey: "extraRoots") } }
    var priceOverrides: [String: Rates] {
        didSet { d.set(try? JSONEncoder().encode(priceOverrides), forKey: "priceOverrides") }
    }

    init() {
        d.register(defaults: [
            "planPrice": 100.0,
            "monthlyBudget": 0.0,
            "cycleStartDay": 1,
            "secondFigure": SecondFigure.cycle.rawValue,
            "usageDisplay": UsageDisplay.limits.rawValue,
            "dataSource": DataSourceMode.logs.rawValue,
            "alertsEnabled": true,
            "alertThresholds": [50, 80, 100],
            "forecastAlert": true,
            "limitAlerts": true,
        ])
        if d.string(forKey: "accountMode") == nil {
            d.set(Self.detectAccountMode().rawValue, forKey: "accountMode")
            if let price = Self.detectPlanPrice() { d.set(price, forKey: "planPrice") }
        }
        accountMode = AccountMode(rawValue: d.string(forKey: "accountMode") ?? "") ?? .api
        planPrice = d.double(forKey: "planPrice")
        monthlyBudget = d.double(forKey: "monthlyBudget")
        cycleStartDay = min(max(d.integer(forKey: "cycleStartDay"), 1), 28)
        secondFigure = SecondFigure(rawValue: d.string(forKey: "secondFigure") ?? "") ?? .cycle
        compactMenuBar = d.bool(forKey: "compactMenuBar")
        usageDisplay = UsageDisplay(rawValue: d.string(forKey: "usageDisplay") ?? "") ?? .limits
        dataSource = DataSourceMode(rawValue: d.string(forKey: "dataSource") ?? "") ?? .logs
        apiActorId = d.string(forKey: "apiActorId")
        apiActorName = d.string(forKey: "apiActorName")
        limitsEnabled = d.bool(forKey: "limitsEnabled")
        alertsEnabled = d.bool(forKey: "alertsEnabled")
        alertThresholds = (d.array(forKey: "alertThresholds") as? [Int]) ?? [50, 80, 100]
        forecastAlert = d.bool(forKey: "forecastAlert")
        limitAlerts = d.bool(forKey: "limitAlerts")
        extraRoots = d.stringArray(forKey: "extraRoots") ?? []
        priceOverrides = d.data(forKey: "priceOverrides")
            .flatMap { try? JSONDecoder().decode([String: Rates].self, from: $0) } ?? [:]
    }

    /// Reads `~/.claude.json` to guess whether this Mac is signed in to a Max/Pro plan.
    static func detectAccountMode() -> AccountMode {
        let url = FileManager.default.homeDirectoryForCurrentUser.appending(path: ".claude.json")
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let account = obj["oauthAccount"] as? [String: Any],
              let orgType = account["organizationType"] as? String
        else { return .api }
        return orgType.hasPrefix("claude_max") || orgType.hasPrefix("claude_pro") ? .subscription : .api
    }

    /// Max 5x → $100, Max 20x → $200, Pro → $20.
    static func detectPlanPrice() -> Double? {
        let url = FileManager.default.homeDirectoryForCurrentUser.appending(path: ".claude.json")
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let account = obj["oauthAccount"] as? [String: Any]
        else { return nil }
        let tier = (account["organizationRateLimitTier"] as? String) ?? ""
        let type = (account["organizationType"] as? String) ?? ""
        if tier.contains("20x") { return 200 }
        if tier.contains("5x") || type.hasPrefix("claude_max") { return 100 }
        if type.hasPrefix("claude_pro") { return 20 }
        return nil
    }

    var logRoots: [URL] {
        LogScanner.defaultRoots() + extraRoots.map { URL(filePath: ($0 as NSString).expandingTildeInPath, directoryHint: .isDirectory) }
    }
}

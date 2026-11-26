import Foundation
import NacutCore

// Titles and persistence keys for the core's settings enums.

extension AccountMode: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [AccountMode] { [.subscription, .api] }
    public var id: String { key }
    public var key: String { self == .subscription ? "subscription" : "api" }
    public init?(key: String) { self.init(matching: key, in: Self.allCases) }
    public var title: String { self == .subscription ? "Max / Pro subscription" : "API pay-as-you-go" }
}

extension UsageDisplay: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [UsageDisplay] { [.limits, .cost] }
    public var id: String { key }
    public var key: String { self == .limits ? "limits" : "cost" }
    public init?(key: String) { self.init(matching: key, in: Self.allCases) }
    public var title: String { self == .limits ? "Plan limits" : "API value" }
}

extension SecondFigure: @retroactive CaseIterable, @retroactive Identifiable {
    public static var allCases: [SecondFigure] { [.cycle, .allTime, .none] }
    public var id: String { key }
    public var key: String {
        switch self {
        case .cycle: return "cycle"
        case .allTime: return "allTime"
        case .none: return "none"
        }
    }
    public init?(key: String) { self.init(matching: key, in: Self.allCases) }
    public var title: String {
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
    public var key: String {
        switch self {
        case .logs: return "logs"
        case .api: return "api"
        case .hybrid: return "hybrid"
        }
    }
    public init?(key: String) { self.init(matching: key, in: Self.allCases) }
    public var title: String {
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

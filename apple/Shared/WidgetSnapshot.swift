import Foundation

/// Budget status as the widget sees it (same raw values the snapshot JSON has always used).
enum SnapshotStatus: String, Codable, Sendable {
    case noBudget, underBudget, tight, willExceed, exceeded
}

/// Compact summary the app writes for the WidgetKit extension. Shared by both targets, so the
/// widget stays small and doesn't link the Rust core.
struct WidgetSnapshot: Codable, Sendable {
    struct ModelSlice: Codable, Sendable, Hashable {
        var name: String
        var cost: Double
    }

    struct Limit: Codable, Sendable, Hashable {
        var label: String
        var utilization: Double   // 0…1
        var resetsAt: Date?

        /// Zero once the window has reset, even if the snapshot predates the reset.
        func effectiveUtilization(at date: Date) -> Double {
            if let resetsAt, resetsAt <= date { return 0 }
            return utilization
        }
    }

    var generatedAt: Date
    var isSubscription: Bool
    var today: Double
    var cycleSpent: Double
    var cycleStart: Date
    var cycleEnd: Date
    var budget: Double
    var projected: Double
    var status: SnapshotStatus
    var headline: String
    var last14: [Double]
    /// Day-of-month (1-based, in the calendar month of `generatedAt`) → cost.
    var monthDays: [Int: Double]
    var monthTitle: String
    var topModels: [ModelSlice]
    var limits: [Limit]
    /// Lead with plan limits instead of cost (subscriptions only).
    var showLimits: Bool?

    static let fileName = "widget-snapshot.json"

    /// The real home folder, even inside the widget's sandbox container.
    static var realHome: URL {
        if let pw = getpwuid(getuid()), let dir = pw.pointee.pw_dir {
            return URL(filePath: String(cString: dir), directoryHint: .isDirectory)
        }
        return FileManager.default.homeDirectoryForCurrentUser
    }

    /// `~/Library/Application Support/NACUT/widget-snapshot.json`
    static var sharedURL: URL {
        realHome.appending(path: "Library/Application Support/NACUT", directoryHint: .isDirectory)
            .appending(path: fileName)
    }

    static func load() -> WidgetSnapshot? {
        guard let data = try? Data(contentsOf: sharedURL) else { return nil }
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return try? d.decode(WidgetSnapshot.self, from: data)
    }

    func write() throws {
        let url = Self.sharedURL
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        try e.encode(self).write(to: url, options: .atomic)
    }

    static let placeholder = WidgetSnapshot(
        generatedAt: .now, isSubscription: false, today: 12.84, cycleSpent: 312.40,
        cycleStart: .now.addingTimeInterval(-20 * 86_400), cycleEnd: .now.addingTimeInterval(10 * 86_400),
        budget: 500, projected: 468, status: .tight, headline: "Tight · projected $468 of $500",
        last14: [8, 12, 4, 0, 18, 22, 15, 9, 14, 20, 11, 7, 16, 12.84],
        monthDays: Dictionary(uniqueKeysWithValues: (1...24).map { ($0, Double(($0 * 37) % 25)) }),
        monthTitle: "November", topModels: [.init(name: "Opus 5.5", cost: 240), .init(name: "Fable 5.2", cost: 52), .init(name: "Sonnet 5.5", cost: 20)],
        limits: [], showLimits: false)
}

/// US-style dollars for the widget (the app uses the core's formatter; this matches it).
enum WidgetUSD {
    private static let whole: NumberFormatter = make(0)
    private static let cents: NumberFormatter = make(2)

    private static func make(_ digits: Int) -> NumberFormatter {
        let f = NumberFormatter()
        f.numberStyle = .currency
        f.currencyCode = "USD"
        f.locale = Locale(identifier: "en_US")
        f.minimumFractionDigits = digits
        f.maximumFractionDigits = digits
        return f
    }

    /// `$4.21` under $100, `$1,234` above.
    static func string(_ v: Double) -> String {
        let value = v.isFinite ? v : 0
        return (abs(value) >= 100 ? whole : cents).string(from: NSNumber(value: value)) ?? "$0"
    }
}

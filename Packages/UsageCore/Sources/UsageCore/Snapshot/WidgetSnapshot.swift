import Foundation

/// Compact summary the app writes for the WidgetKit extension.
public struct WidgetSnapshot: Codable, Sendable {
    public struct ModelSlice: Codable, Sendable, Hashable {
        public var name: String
        public var cost: Double
        public init(name: String, cost: Double) { self.name = name; self.cost = cost }
    }

    public struct Limit: Codable, Sendable, Hashable {
        public var label: String
        public var utilization: Double   // 0…1
        public var resetsAt: Date?
        public init(label: String, utilization: Double, resetsAt: Date?) {
            self.label = label; self.utilization = utilization; self.resetsAt = resetsAt
        }
        /// Zero once the window has reset, even if the snapshot predates the reset.
        public func effectiveUtilization(at date: Date) -> Double {
            if let resetsAt, resetsAt <= date { return 0 }
            return utilization
        }
    }

    public var generatedAt: Date
    public var isSubscription: Bool
    public var today: Double
    public var cycleSpent: Double
    public var cycleStart: Date
    public var cycleEnd: Date
    public var budget: Double
    public var projected: Double
    public var status: BudgetStatus
    public var headline: String
    public var last14: [Double]
    /// Day-of-month (1-based, in the calendar month of `generatedAt`) → cost.
    public var monthDays: [Int: Double]
    public var monthTitle: String
    public var topModels: [ModelSlice]
    public var limits: [Limit]
    /// Lead with plan limits instead of cost (subscriptions only). Optional so older snapshots still decode.
    public var showLimits: Bool?

    public init(
        generatedAt: Date, isSubscription: Bool, today: Double, cycleSpent: Double, cycleStart: Date,
        cycleEnd: Date, budget: Double, projected: Double, status: BudgetStatus, headline: String,
        last14: [Double], monthDays: [Int: Double], monthTitle: String, topModels: [ModelSlice], limits: [Limit],
        showLimits: Bool = false
    ) {
        self.generatedAt = generatedAt
        self.isSubscription = isSubscription
        self.today = today
        self.cycleSpent = cycleSpent
        self.cycleStart = cycleStart
        self.cycleEnd = cycleEnd
        self.budget = budget
        self.projected = projected
        self.status = status
        self.headline = headline
        self.last14 = last14
        self.monthDays = monthDays
        self.monthTitle = monthTitle
        self.topModels = topModels
        self.limits = limits
        self.showLimits = showLimits
    }

    public static let fileName = "widget-snapshot.json"

    /// The real home folder, even inside the widget's sandbox container.
    static var realHome: URL {
        if let pw = getpwuid(getuid()), let dir = pw.pointee.pw_dir {
            return URL(filePath: String(cString: dir), directoryHint: .isDirectory)
        }
        return FileManager.default.homeDirectoryForCurrentUser
    }

    /// `~/Library/Application Support/NACUT/widget-snapshot.json`
    public static var sharedURL: URL {
        realHome.appending(path: "Library/Application Support/NACUT", directoryHint: .isDirectory)
            .appending(path: fileName)
    }

    public static func load() -> WidgetSnapshot? {
        guard let data = try? Data(contentsOf: sharedURL) else { return nil }
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return try? d.decode(WidgetSnapshot.self, from: data)
    }

    public func write() throws {
        let url = Self.sharedURL
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        try e.encode(self).write(to: url, options: .atomic)
    }

    public static let placeholder = WidgetSnapshot(
        generatedAt: .now, isSubscription: false, today: 12.84, cycleSpent: 312.40,
        cycleStart: .now.addingTimeInterval(-20 * 86_400), cycleEnd: .now.addingTimeInterval(10 * 86_400),
        budget: 500, projected: 468, status: .tight, headline: "Tight · projected $468 of $500",
        last14: [8, 12, 4, 0, 18, 22, 15, 9, 14, 20, 11, 7, 16, 12.84],
        monthDays: Dictionary(uniqueKeysWithValues: (1...24).map { ($0, Double(($0 * 37) % 25)) }),
        monthTitle: "November", topModels: [.init(name: "Opus 5.5", cost: 240), .init(name: "Fable 5.2", cost: 52), .init(name: "Sonnet 5.5", cost: 20)],
        limits: [])
}

import Foundation

/// Budget status as the widget sees it (same raw values the snapshot JSON has always used).
public enum SnapshotStatus: String, Codable, Sendable, CaseIterable {
    case noBudget, underBudget, tight, willExceed, exceeded
}

/// Compact summary the app writes for the WidgetKit extension. Shared by both targets, so the
/// widget stays small and doesn't link the Rust core.
///
/// The JSON is the contract between the app and the widget (which may be a different build for a
/// moment after an update): keep property names, order and types stable.
public struct WidgetSnapshot: Codable, Sendable, Equatable {
    public struct ModelSlice: Codable, Sendable, Hashable {
        public var name: String
        public var cost: Double

        public init(name: String, cost: Double) {
            self.name = name
            self.cost = cost
        }
    }

    public struct Limit: Codable, Sendable, Hashable {
        public var label: String
        public var utilization: Double   // 0…1
        public var resetsAt: Date?

        public init(label: String, utilization: Double, resetsAt: Date?) {
            self.label = label
            self.utilization = utilization
            self.resetsAt = resetsAt
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
    public var status: SnapshotStatus
    public var headline: String
    public var last14: [Double]
    /// Day-of-month (1-based, in the calendar month of `generatedAt`) → cost.
    public var monthDays: [Int: Double]
    public var monthTitle: String
    public var topModels: [ModelSlice]
    public var limits: [Limit]
    /// Lead with plan limits instead of cost (subscriptions only).
    public var showLimits: Bool?

    public init(
        generatedAt: Date, isSubscription: Bool, today: Double, cycleSpent: Double, cycleStart: Date,
        cycleEnd: Date, budget: Double, projected: Double, status: SnapshotStatus, headline: String,
        last14: [Double], monthDays: [Int: Double], monthTitle: String, topModels: [ModelSlice],
        limits: [Limit], showLimits: Bool? = nil
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
    public static var realHome: URL {
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

    public static func decode(_ data: Data) throws -> WidgetSnapshot {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return try d.decode(WidgetSnapshot.self, from: data)
    }

    public func encoded() throws -> Data {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        return try e.encode(self)
    }

    /// `nil` if the file is missing or unreadable.
    public static func load(from url: URL = sharedURL) -> WidgetSnapshot? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? decode(data)
    }

    public func write(to url: URL = Self.sharedURL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try encoded().write(to: url, options: .atomic)
        // Spend figures: readable only by this user (the widget runs as the same user).
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }

    /// Day-of-month → cost for the calendar month containing `now`, from a daily series.
    public static func monthDays(_ days: [DayCost], now: Date, calendar: Calendar) -> [Int: Double] {
        let year = calendar.component(.year, from: now)
        let month = calendar.component(.month, from: now)
        var out: [Int: Double] = [:]
        for d in days where d.year == year && d.month == month {
            out[d.day] = d.cost
        }
        return out
    }

    /// Sample data for the widget gallery and for when the app hasn't written a snapshot yet.
    public static func placeholder(at now: Date) -> WidgetSnapshot {
        WidgetSnapshot(
            generatedAt: now, isSubscription: false, today: 12.84, cycleSpent: 312.40,
            cycleStart: now.addingTimeInterval(-20 * 86_400), cycleEnd: now.addingTimeInterval(10 * 86_400),
            budget: 500, projected: 468, status: .tight, headline: "Tight · projected $468 of $500",
            last14: [8, 12, 4, 0, 18, 22, 15, 9, 14, 20, 11, 7, 16, 12.84],
            monthDays: Dictionary(uniqueKeysWithValues: (1...24).map { ($0, Double(($0 * 37) % 25)) }),
            monthTitle: "November", topModels: [.init(name: "Opus 5.5", cost: 240), .init(name: "Fable 5.2", cost: 52), .init(name: "Sonnet 5.5", cost: 20)],
            limits: [], showLimits: false)
    }

    public static let placeholder = placeholder(at: .now)
}

/// One calendar day's cost, as plain values (the app maps the core's `DayEntry` onto it).
public struct DayCost: Sendable, Hashable {
    public var year: Int
    public var month: Int
    public var day: Int
    public var cost: Double

    public init(year: Int, month: Int, day: Int, cost: Double) {
        self.year = year
        self.month = month
        self.day = day
        self.cost = cost
    }
}

/// US-style dollars for the widget (the app uses the core's formatter; this matches it).
public enum WidgetUSD {
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
    public static func string(_ v: Double) -> String {
        let value = v.isFinite ? v : 0
        return (abs(value) >= 100 ? whole : cents).string(from: NSNumber(value: value)) ?? "$0"
    }
}

import Foundation
import NacutCore

// Conformances and Foundation conveniences for the core's UniFFI-generated types.

// The Rust engine is internally synchronized (every method is safe from any thread), and the
// generated records are plain values; the generated code just doesn't declare it.
extension NacutEngine: @retroactive @unchecked Sendable {}
extension ApiResult: @retroactive @unchecked Sendable {}
extension ApiActor: @retroactive @unchecked Sendable {}
extension Snapshot: @retroactive @unchecked Sendable {}
extension Alert: @retroactive @unchecked Sendable {}

extension ModelView: @retroactive Identifiable {}
extension SessionView: @retroactive Identifiable {}
extension LimitView: @retroactive Identifiable {}
extension ApiActor: @retroactive Identifiable {}

extension ProjectView: @retroactive Identifiable {
    public var id: String { path }
}

extension NamedValue: @retroactive Identifiable {
    public var id: String { name }
    init(name: String, value: Double) { self.init(name: name, value: value, tokens: 0) }
}

extension ReconciliationView: @retroactive Identifiable {
    public var id: DayKey { date }
}

extension ForecastPointView: @retroactive Identifiable {
    public var id: String { "\(kind)-\(startMs)" }
    var day: Date { Date(ms: startMs) }
}

extension Date {
    /// From Unix milliseconds (the core's timestamp format).
    init(ms: Int64) { self.init(timeIntervalSince1970: Double(ms) / 1000) }
    var ms: Int64 { Int64((timeIntervalSince1970 * 1000).rounded()) }
}

extension DayKey {
    /// Local midnight of this calendar day.
    var date: Date {
        Calendar.current.date(from: DateComponents(year: Int(year), month: Int(month), day: Int(day)))!
    }

    init(_ date: Date) {
        let c = Calendar.current.dateComponents([.year, .month, .day], from: date)
        self.init(year: Int32(c.year!), month: UInt8(c.month!), day: UInt8(c.day!))
    }
}

extension DayEntry {
    var day: Date { date.date }
}

extension SessionView {
    var start: Date { Date(ms: startMs) }
    var duration: TimeInterval { durationSecs }
}

extension ProjectView {
    var lastActive: Date { Date(ms: lastActiveMs) }
}

extension LimitView {
    var resetsAt: Date? { resetsAtMs.map(Date.init(ms:)) }
}

struct CycleDates {
    var start: Date
    var end: Date
    var lastDay: Date { Calendar.current.date(byAdding: .day, value: -1, to: end)! }
}

extension ForecastView {
    var cycle: CycleDates { CycleDates(start: Date(ms: cycleStartMs), end: Date(ms: cycleEndMs)) }
    var exhaustionDate: Date? { exhaustionMs.map(Date.init(ms:)) }
}

/// The panel's range picker, persisted with `@AppStorage`.
enum RangeOption: String, CaseIterable, Identifiable {
    case cycle, last30, allTime
    var id: String { rawValue }
    var title: String {
        switch self {
        case .cycle: return "This cycle"
        case .last30: return "30 days"
        case .allTime: return "All time"
        }
    }
    var core: StatRange {
        switch self {
        case .cycle: return .cycle
        case .last30: return .last30
        case .allTime: return .allTime
        }
    }
}

/// US-style dollars from the core, so every platform formats identically.
enum USD {
    static func string(_ v: Double, compact: Bool = false) -> String { formatUsd(value: v, compact: compact) }
    static func exact(_ v: Double) -> String { formatUsdExact(value: v) }
    static func tokens(_ n: UInt64) -> String { formatTokens(count: n) }
}

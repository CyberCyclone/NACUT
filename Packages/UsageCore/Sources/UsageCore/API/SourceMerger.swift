import Foundation

public enum DataSourceMode: String, Codable, CaseIterable, Sendable, Identifiable {
    case logs, api, hybrid
    public var id: String { rawValue }
    public var title: String {
        switch self {
        case .logs: return "Local logs"
        case .api: return "Anthropic API only"
        case .hybrid: return "Hybrid (API + live logs)"
        }
    }
}

public struct Calibration: Sendable, Hashable {
    /// Billed ÷ list-price estimate over recent complete days. 1 when unknown.
    public var ratio: Double
    public var sampleDays: Int
    public var apiTotal: Double
    public var logTotal: Double
    public static let identity = Calibration(ratio: 1, sampleDays: 0, apiTotal: 0, logTotal: 0)
}

public struct ReconciliationDay: Identifiable, Sendable, Hashable {
    public var id: Date { date }
    public var date: Date
    public var api: Double
    public var logs: Double
}

/// Combines local-log estimates with billed API data.
public struct SourceMerger: Sendable {
    public var calendar: Calendar

    public init(calendar: Calendar = .current) {
        self.calendar = calendar
    }

    /// API rows folded into local days.
    public func apiSeries(_ data: APICostData) -> DailySeries {
        var s: DailySeries = [:]
        for row in data.rows {
            let day = calendar.startOfDay(for: row.start)
            var d = s[day] ?? DayCost(source: .api)
            d.cost += row.amount
            if let m = row.model { d.byModel[PriceBook.normalize(m), default: 0] += row.amount }
            s[day] = d
        }
        return s
    }

    /// Local days entirely covered by API data (before the refresh point).
    private func completeDays(_ data: APICostData) -> (first: Date, endExclusive: Date)? {
        guard let refreshed = data.refreshedAt else { return nil }
        let first = calendar.startOfDay(for: data.coverageStart)
        let firstFull = first < data.coverageStart ? calendar.date(byAdding: .day, value: 1, to: first)! : first
        let end = calendar.startOfDay(for: min(refreshed, data.coverageEnd))
        return firstFull < end ? (firstFull, end) : nil
    }

    public func calibration(logs: DailySeries, api: APICostData, window: Int = 14) -> Calibration {
        guard let (first, endEx) = completeDays(api) else { return .identity }
        let apiDays = apiSeries(api)
        var apiSum = 0.0, logSum = 0.0, n = 0
        var d = calendar.date(byAdding: .day, value: -1, to: endEx)!
        while d >= first && n < window {
            let a = apiDays[d]?.cost ?? 0
            let l = logs[d]?.cost ?? 0
            if a > 0 && l > 0 {
                apiSum += a
                logSum += l
                n += 1
            }
            d = calendar.date(byAdding: .day, value: -1, to: d)!
        }
        guard n >= 3, logSum > 0 else { return Calibration(ratio: 1, sampleDays: n, apiTotal: apiSum, logTotal: logSum) }
        let ratio = min(max(apiSum / logSum, 0.2), 5)
        return Calibration(ratio: ratio, sampleDays: n, apiTotal: apiSum, logTotal: logSum)
    }

    /// The series the UI shows.
    /// - Parameter logCostAfter: local-log cost for a given day after a moment (to fill the partial refresh day).
    public func merge(
        mode: DataSourceMode, logs: DailySeries, api: APICostData?,
        logCostAfter: (Date, Date) -> DayCost
    ) -> (series: DailySeries, calibration: Calibration) {
        guard mode != .logs, let api else { return (logs, .identity) }
        let apiDays = apiSeries(api)
        let cal = calibration(logs: logs, api: api)
        let coverageFirst = calendar.startOfDay(for: api.coverageStart)

        if mode == .api {
            var s = logs.filter { $0.key < coverageFirst }
            for (k, v) in apiDays { s[k] = v }
            return (s, cal)
        }

        // Hybrid
        var s = logs
        guard let refreshed = api.refreshedAt else {
            // No API data yet: logs scaled by calibration (identity).
            return (logs, cal)
        }
        let refreshDay = calendar.startOfDay(for: refreshed)
        for (day, logDay) in logs where day > refreshDay {
            s[day] = scaled(logDay, by: cal.ratio)
        }
        // Complete API days override logs.
        var d = coverageFirst
        while d < refreshDay {
            s[d] = apiDays[d] ?? DayCost(source: .api)
            d = calendar.date(byAdding: .day, value: 1, to: d)!
        }
        // Refresh day: billed part from the API plus the live estimate after the refresh point.
        var partial = apiDays[refreshDay] ?? DayCost(source: .api)
        let rest = scaled(logCostAfter(refreshDay, refreshed), by: cal.ratio)
        partial.cost += rest.cost
        partial.messages += rest.messages
        partial.tokens += rest.tokens
        for (m, v) in rest.byModel { partial.byModel[m, default: 0] += v }
        partial.source = rest.cost > 0 ? .estimated : .api
        s[refreshDay] = partial
        return (s, cal)
    }

    public func reconciliation(logs: DailySeries, api: APICostData, days: Int = 30) -> [ReconciliationDay] {
        guard let (first, endEx) = completeDays(api) else { return [] }
        let apiDays = apiSeries(api)
        var out: [ReconciliationDay] = []
        var d = calendar.date(byAdding: .day, value: -1, to: endEx)!
        while d >= first && out.count < days {
            out.append(.init(date: d, api: apiDays[d]?.cost ?? 0, logs: logs[d]?.cost ?? 0))
            d = calendar.date(byAdding: .day, value: -1, to: d)!
        }
        return out.reversed()
    }

    private func scaled(_ day: DayCost, by r: Double) -> DayCost {
        var d = day
        d.cost *= r
        d.byModel = d.byModel.mapValues { $0 * r }
        d.source = r == 1 ? .logs : .estimated
        return d
    }
}

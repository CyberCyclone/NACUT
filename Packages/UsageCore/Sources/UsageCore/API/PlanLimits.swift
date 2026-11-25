import Foundation

/// One Max/Pro plan limit window, e.g. the 5-hour session or the weekly limit.
public struct PlanLimit: Identifiable, Codable, Sendable, Hashable {
    public var id: String
    public var label: String
    /// 0…1
    public var utilization: Double
    public var resetsAt: Date?

    public init(id: String, label: String, utilization: Double, resetsAt: Date?) {
        self.id = id
        self.label = label
        self.utilization = utilization
        self.resetsAt = resetsAt
    }

    /// Parses the subscription usage response. Tolerant of new or missing windows:
    /// any top-level object with a numeric `utilization` becomes a limit.
    public static func parse(_ data: Data) -> [PlanLimit] {
        guard let root = try? JSONDecoder().decode(JSONValue.self, from: data),
              case .object(let obj) = root else { return [] }
        if case .array(let items)? = obj["limits"] {
            let fromList = parseList(items)
            if !fromList.isEmpty { return fromList }
        }
        var out: [PlanLimit] = []
        for (key, value) in obj {
            guard case .object = value, let util = value["utilization"]?.number else { continue }
            let resets = value["resets_at"]?.string.flatMap(LogScanner.parseDate)
            out.append(PlanLimit(id: key, label: label(for: key), utilization: min(max(util / 100, 0), 1.5), resetsAt: resets))
        }
        return out.sorted { (order(of: $0.id), $0.id) < (order(of: $1.id), $1.id) }
    }

    /// Newer responses list every limit in `limits: [{kind, percent, resets_at, scope}]`,
    /// including per-model weekly limits that the top-level keys leave null.
    /// Ids are mapped onto the top-level key names so labels and windows stay the same.
    static func parseList(_ items: [JSONValue]) -> [PlanLimit] {
        var out: [PlanLimit] = []
        for item in items {
            guard case .object = item, let kind = item["kind"]?.string,
                  let pct = item["percent"]?.number else { continue }
            let resets = item["resets_at"]?.string.flatMap(LogScanner.parseDate)
            let scopeName = item["scope"]?["model"]?["display_name"]?.string
                ?? item["scope"]?["surface"]?.string
            let id: String, label: String
            switch kind {
            case "session":
                id = "five_hour"; label = Self.label(for: id)
            case "weekly_all":
                id = "seven_day"; label = Self.label(for: id)
            case "weekly_scoped" where scopeName != nil:
                id = "seven_day_" + scopeName!.lowercased().replacingOccurrences(of: " ", with: "_")
                label = "Weekly · \(scopeName!)"
            default:
                id = kind + (scopeName.map { "_" + $0.lowercased() } ?? "")
                label = Self.label(for: id)
            }
            guard !out.contains(where: { $0.id == id }) else { continue }
            out.append(PlanLimit(id: id, label: label, utilization: min(max(pct / 100, 0), 1.5), resetsAt: resets))
        }
        return out.sorted { (order(of: $0.id), $0.id) < (order(of: $1.id), $1.id) }
    }

    static func label(for key: String) -> String {
        switch key {
        case "five_hour": return "5-hour session"
        case "seven_day": return "Weekly · all models"
        case "seven_day_opus": return "Weekly · Opus"
        case "seven_day_sonnet": return "Weekly · Sonnet"
        case "seven_day_fable": return "Weekly · Fable"
        case "seven_day_oauth_apps": return "Weekly · apps"
        default:
            let words = key.split(separator: "_").map { $0.capitalized }
            return words.joined(separator: " ")
        }
    }

    static func order(of key: String) -> Int {
        switch key {
        case "five_hour": return 0
        case "seven_day": return 1
        default: return 2
        }
    }

    /// Utilization as of `now`: zero once the window has reset, even before the next fetch.
    public func effectiveUtilization(now: Date = .now) -> Double {
        if let resetsAt, resetsAt <= now { return 0 }
        return utilization
    }

    public var shortLabel: String {
        switch id {
        case "five_hour": return "5h"
        case "seven_day": return "Week"
        default: return label.replacingOccurrences(of: "Weekly · ", with: "")
        }
    }
}

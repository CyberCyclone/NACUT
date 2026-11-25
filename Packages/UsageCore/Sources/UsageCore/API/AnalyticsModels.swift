import Foundation

/// One hour of billed cost for one model, from the Usage & Cost Analytics API.
public struct APICostRow: Codable, Sendable, Hashable {
    public var start: Date
    public var end: Date
    public var model: String?
    /// USD after discounts, before credits.
    public var amount: Double
    /// USD at list price.
    public var listAmount: Double

    public init(start: Date, end: Date, model: String?, amount: Double, listAmount: Double) {
        self.start = start
        self.end = end
        self.model = model
        self.amount = amount
        self.listAmount = listAmount
    }
}

public struct APIActor: Identifiable, Codable, Sendable, Hashable {
    public var id: String
    public var type: String
    public var display: String
    public var cost: Double
}

public struct APICostData: Codable, Sendable {
    public var rows: [APICostRow]
    public var refreshedAt: Date?
    public var coverageStart: Date
    public var coverageEnd: Date
    public var fetchedAt: Date

    public init(rows: [APICostRow], refreshedAt: Date?, coverageStart: Date, coverageEnd: Date, fetchedAt: Date = .now) {
        self.rows = rows
        self.refreshedAt = refreshedAt
        self.coverageStart = coverageStart
        self.coverageEnd = coverageEnd
        self.fetchedAt = fetchedAt
    }
}

public enum ConnectionStatus: Codable, Sendable, Hashable {
    case notConfigured
    case connected(refreshedAt: Date?)
    case noDataYet
    case unauthorized(String)
    case forbidden(String)
    case unavailable(String)
    case failed(String)

    public var isUsable: Bool {
        switch self {
        case .connected, .noDataYet: return true
        default: return false
        }
    }

    public var symbol: String {
        switch self {
        case .notConfigured: return "circle.dashed"
        case .connected: return "checkmark.circle.fill"
        case .noDataYet: return "clock.badge.checkmark"
        case .unauthorized, .forbidden: return "lock.circle.fill"
        case .unavailable: return "nosign"
        case .failed: return "exclamationmark.triangle.fill"
        }
    }

    public var message: String {
        switch self {
        case .notConfigured:
            return "Not connected. Using local logs."
        case .connected(let at):
            if let at { return "Connected · data through \(at.formatted(.dateTime.month(.abbreviated).day().hour().minute()))" }
            return "Connected"
        case .noDataYet:
            return "Connected, but the API has no data for this period yet (it can lag up to 24 hours)."
        case .unauthorized(let m):
            return "Key rejected (401). Check that you pasted an Admin key or a scoped key with read:analytics. \(m)"
        case .forbidden(let m):
            return "Access denied (403). This key or your organization isn't allowed to read usage analytics. \(m)"
        case .unavailable(let m):
            return "Usage analytics aren't available to this organization. \(m)"
        case .failed(let m):
            return "Couldn't reach the API: \(m)"
        }
    }
}

// MARK: - Wire format

struct WirePage<Row: Decodable>: Decodable {
    var data: [Row]
    var has_more: Bool?
    var next_page: String?
    var data_refreshed_at: String?
}

struct WireCostRow: Decodable {
    var starting_at: String
    var ending_at: String
    var group: WireGroup?
    var cost: WireCost
}

struct WireGroup: Decodable {
    var model: String?
    var actor: JSONValue?
}

struct WireCost: Decodable {
    var amount: String?
    var list_amount: String?
    var currency: String?
}

struct WireError: Decodable {
    struct Inner: Decodable { var type: String?; var message: String? }
    var error: Inner?
}

/// Minimal JSON value for fields whose shape varies (actor objects).
public enum JSONValue: Decodable, Sendable, Hashable {
    case string(String), number(Double), bool(Bool), object([String: JSONValue]), array([JSONValue]), null

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else if let a = try? c.decode([JSONValue].self) { self = .array(a) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }

    public subscript(key: String) -> JSONValue? {
        if case .object(let o) = self { return o[key] }
        return nil
    }

    public var string: String? {
        if case .string(let s) = self { return s }
        return nil
    }

    public var number: Double? {
        switch self {
        case .number(let n): return n
        case .string(let s): return Double(s)
        default: return nil
        }
    }
}

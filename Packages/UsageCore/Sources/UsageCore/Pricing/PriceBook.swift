import Foundation

/// USD per million tokens.
public struct Rates: Codable, Sendable, Hashable {
    public var input: Double
    public var output: Double
    public var write5m: Double
    public var write1h: Double
    public var read: Double

    public init(input: Double, output: Double, write5m: Double, write1h: Double, read: Double) {
        self.input = input
        self.output = output
        self.write5m = write5m
        self.write1h = write1h
        self.read = read
    }
}

public struct LongContextRates: Codable, Sendable, Hashable {
    public var threshold: Int
    public var input: Double
    public var output: Double
    public var write5m: Double
    public var write1h: Double
    public var read: Double

    var rates: Rates { Rates(input: input, output: output, write5m: write5m, write1h: write1h, read: read) }
}

public struct ModelPrice: Codable, Sendable, Hashable {
    public var id: String
    public var name: String
    public var family: String
    public var rank: Int
    public var input: Double
    public var output: Double
    public var write5m: Double
    public var write1h: Double
    public var read: Double
    public var longContext: LongContextRates?

    public var rates: Rates { Rates(input: input, output: output, write5m: write5m, write1h: write1h, read: read) }
}

public struct PriceTable: Codable, Sendable {
    public var updated: String
    public var webSearchPerThousand: Double
    public var fastMultiplier: Double
    public var models: [ModelPrice]
}

/// Turns token counts into USD at Anthropic list prices.
public struct PriceBook: Sendable {
    public private(set) var table: PriceTable
    private var byId: [String: ModelPrice]
    /// User overrides keyed by normalized model id.
    public var overrides: [String: Rates]

    public init(table: PriceTable, overrides: [String: Rates] = [:]) {
        self.table = table
        self.overrides = overrides
        self.byId = Dictionary(table.models.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    public static let bundled: PriceBook = {
        guard let url = Bundle.module.url(forResource: "Pricing", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let table = try? JSONDecoder().decode(PriceTable.self, from: data)
        else { fatalError("Pricing.json missing from UsageCore bundle") }
        return PriceBook(table: table)
    }()

    public func withOverrides(_ o: [String: Rates]) -> PriceBook {
        var copy = self
        copy.overrides = o
        return copy
    }

    /// `claude-opus-5-5-20260401[1m]` → `claude-opus-5-5`; Mythos maps to the matching Fable.
    public static func normalize(_ model: String) -> String {
        var m = model.lowercased()
        if let r = m.range(of: "[") { m = String(m[..<r.lowerBound]) }
        for prefix in ["anthropic.", "us.anthropic.", "eu.anthropic.", "global.anthropic."] where m.hasPrefix(prefix) {
            m = String(m.dropFirst(prefix.count))
        }
        if let at = m.firstIndex(of: "@") { m = String(m[..<at]) }
        // Strip a trailing date (-20250101) or version tag (-v1:0).
        if let r = m.range(of: #"-\d{8}$"#, options: .regularExpression) { m.removeSubrange(r) }
        if let r = m.range(of: #"-v\d+(:\d+)?$"#, options: .regularExpression) { m.removeSubrange(r) }
        m = m.replacingOccurrences(of: "claude-mythos-", with: "claude-fable-")
        return m
    }

    public static func family(of normalized: String) -> String? {
        for f in ["fable", "opus", "sonnet", "haiku"] where normalized.contains(f) { return f }
        return nil
    }

    /// Price entry for a model, and whether it was guessed from the family.
    public func price(for model: String) -> (price: ModelPrice?, estimated: Bool) {
        let n = Self.normalize(model)
        if let p = byId[n] { return (p, false) }
        if let fam = Self.family(of: n) {
            let best = table.models.filter { $0.family == fam }.max { $0.rank < $1.rank }
            return (best, true)
        }
        return (nil, true)
    }

    public func displayName(for model: String) -> String {
        let n = Self.normalize(model)
        if let p = byId[n] { return p.name }
        // claude-opus-6-1 → Opus 6.1
        let parts = n.replacingOccurrences(of: "claude-", with: "").split(separator: "-")
        guard let fam = parts.first else { return model }
        let version = parts.dropFirst().joined(separator: ".")
        return version.isEmpty ? fam.capitalized : "\(fam.capitalized) \(version)"
    }

    public func rates(for record: UsageRecord) -> (rates: Rates, estimated: Bool) {
        let n = Self.normalize(record.model)
        let (p, estimated) = price(for: record.model)
        if let o = overrides[n] { return (o, false) }
        guard let p else { return (Rates(input: 0, output: 0, write5m: 0, write1h: 0, read: 0), true) }
        if let lc = p.longContext, record.promptTokens > lc.threshold { return (lc.rates, estimated) }
        return (p.rates, estimated)
    }

    public func cost(of record: UsageRecord) -> (cost: Double, estimated: Bool) {
        let (r, est) = rates(for: record)
        var c = Double(record.input) * r.input
            + Double(record.output) * r.output
            + Double(record.cacheWrite5m) * r.write5m
            + Double(record.cacheWrite1h) * r.write1h
            + Double(record.cacheRead) * r.read
        c /= 1_000_000
        if record.fast { c *= table.fastMultiplier }
        c += Double(record.webSearches) * table.webSearchPerThousand / 1000
        return (c, est)
    }

    /// What cache reads would have cost at the full input price, minus what they did cost.
    public func cacheSavings(of record: UsageRecord) -> Double {
        let (r, _) = rates(for: record)
        let mult = record.fast ? table.fastMultiplier : 1
        return Double(record.cacheRead) * (r.input - r.read) / 1_000_000 * mult
    }

    public func priced(_ records: [UsageRecord]) -> [UsageRecord] {
        records.map { rec in
            var r = rec
            (r.cost, r.estimated) = cost(of: rec)
            return r
        }
    }
}

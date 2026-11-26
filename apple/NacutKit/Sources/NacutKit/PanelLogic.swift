import Foundation

// Pure logic behind the menu bar panel's views.

/// The sequential coral ramp (calendar, heatmap, widget mini calendar).
public enum Ramp {
    /// Number of steps in the ramp.
    public static let steps = 5

    /// Ramp step for a magnitude fraction (0…1 of the scale): `nil` for nothing (≤ 0 or NaN),
    /// otherwise 0…`steps - 1`. Fractions above 1 (including +∞) use the darkest step.
    public static func index(_ fraction: Double, steps: Int = steps) -> Int? {
        guard fraction > 0, steps > 0 else { return nil }   // also false for NaN
        guard fraction < 1 else { return steps - 1 }
        return min(Int((fraction * Double(steps)).rounded(.up)) - 1, steps - 1)
    }

    /// Whether text on that step needs light ink (the two darkest steps).
    public static func needsLightInk(_ fraction: Double) -> Bool {
        fraction > 0.6
    }
}

/// `<1m`, `12m`, `3h 5m`, or `2d 4h` from 48 hours up.
public func formatSessionDuration(_ seconds: Double) -> String {
    guard seconds.isFinite, seconds >= 60 else { return "<1m" }
    let s = Int(min(seconds, Double(Int32.max)))
    let h = s / 3600, m = (s % 3600) / 60
    if h >= 48 { return "\(h / 24)d \(h % 24)h" }
    return h > 0 ? "\(h)h \(m)m" : "\(m)m"
}

/// The menu bar's gauge glyph.
public enum MenuBarGauge {
    /// `gaugeFraction` is the worst plan limit or the budget share (the core decides which), or
    /// `nil` when there's nothing to measure.
    public static func symbol(showsLimits: Bool, gaugeFraction: Double?, budgetExceeded: Bool) -> String {
        if showsLimits, (gaugeFraction ?? 0) >= 1 { return "exclamationmark.triangle.fill" }
        if !showsLimits, budgetExceeded { return "exclamationmark.triangle.fill" }
        guard let fraction = gaugeFraction else { return "gauge.with.dots.needle.33percent" }
        return needle(fraction)
    }

    public static func needle(_ fraction: Double) -> String {
        switch fraction {
        case ..<0.2: return "gauge.with.dots.needle.0percent"
        case ..<0.45: return "gauge.with.dots.needle.33percent"
        case ..<0.6: return "gauge.with.dots.needle.50percent"
        case ..<0.85: return "gauge.with.dots.needle.67percent"
        default: return "gauge.with.dots.needle.100percent"
        }
    }
}

/// Which slice of a donut chart an angle value (in the chart's value units, from 0 at the top)
/// falls in: slices run in order, each `values[i]` wide. `nil` with no angle or past the end.
public func donutSliceIndex(angle: Double?, values: [Double]) -> Int? {
    guard let a = angle else { return nil }
    var running = 0.0
    for (i, v) in values.enumerated() {
        running += v
        if a <= running { return i }
    }
    return nil
}

/// Stacked daily bars by model, with whatever the per-model figures don't cover as "Other".
public enum DailyBreakdown {
    public static let otherName = "Other"

    public struct Day: Sendable, Hashable {
        public var date: Date
        public var cost: Double
        /// Model id → cost.
        public var byModel: [String: Double]

        public init(date: Date, cost: Double, byModel: [String: Double]) {
            self.date = date
            self.cost = cost
            self.byModel = byModel
        }
    }

    public struct Row: Identifiable, Sendable, Hashable {
        public var id: String { "\(date.timeIntervalSince1970)-\(model)" }
        public var date: Date
        /// Display name, or `otherName`.
        public var model: String
        public var cost: Double

        public init(date: Date, model: String, cost: Double) {
            self.date = date
            self.model = model
            self.cost = cost
        }
    }

    /// Model ids that appear on any day, in color-slot order (registered models first), then by id.
    public static func modelIds(_ days: [Day], slotOrder: (String) -> Int) -> [String] {
        Array(Set(days.flatMap { $0.byModel.keys })).sorted { (slotOrder($0), $0) < (slotOrder($1), $1) }
    }

    /// One row per day and model (in `modelIds` order), plus an "Other" row when the day's total
    /// exceeds the per-model sum by more than half a cent.
    public static func rows(_ days: [Day], slotOrder: (String) -> Int, displayName: (String) -> String) -> [Row] {
        days.flatMap { day in
            var out = day.byModel.keys.sorted { (slotOrder($0), $0) < (slotOrder($1), $1) }
                .map { Row(date: day.date, model: displayName($0), cost: day.byModel[$0]!) }
            let rest = day.cost - day.byModel.values.reduce(0, +)
            if rest > 0.005 { out.append(Row(date: day.date, model: otherName, cost: rest)) }
            return out
        }
    }
}

/// A price in USD per million tokens for the Pricing table: `$0.3`, `$0.08`, `$15`, `$3.75`.
public func formatPerMillionPrice(_ v: Double) -> String {
    v < 1 ? String(format: "$%.3g", v) : String(format: "$%.2f", v).replacingOccurrences(of: ".00", with: "")
}

/// Whether a typed API key is an Admin key (which can manage the whole organization).
public func isAdminAPIKey(_ input: String) -> Bool {
    input.trimmingCharacters(in: .whitespaces).hasPrefix("sk-ant-admin")
}

/// `42%`, rounded to a whole percent.
public func wholePercent(_ fraction: Double, locale: Locale = .autoupdatingCurrent) -> String {
    fraction.formatted(.percent.precision(.fractionLength(0)).locale(locale))
}

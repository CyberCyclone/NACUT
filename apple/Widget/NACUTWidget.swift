import SwiftUI
import WidgetKit

struct UsageEntry: TimelineEntry {
    let date: Date
    let snapshot: WidgetSnapshot
    let isPlaceholder: Bool
}

struct UsageProvider: TimelineProvider {
    func placeholder(in context: Context) -> UsageEntry {
        UsageEntry(date: .now, snapshot: .placeholder, isPlaceholder: true)
    }

    func getSnapshot(in context: Context, completion: @escaping (UsageEntry) -> Void) {
        let snap = WidgetSnapshot.load()
        completion(UsageEntry(date: .now, snapshot: snap ?? .placeholder, isPlaceholder: snap == nil))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<UsageEntry>) -> Void) {
        let snap = WidgetSnapshot.load()
        let entry = UsageEntry(date: .now, snapshot: snap ?? .placeholder, isPlaceholder: snap == nil)
        // Refresh every 15 minutes, or right after a limit resets if that's sooner.
        var next = Date.now.addingTimeInterval(15 * 60)
        if let reset = snap?.limits.compactMap(\.resetsAt).filter({ $0 > .now }).min() {
            next = min(next, reset.addingTimeInterval(5))
        }
        completion(Timeline(entries: [entry], policy: .after(next)))
    }
}

@main
struct NACUTWidgetBundle: WidgetBundle {
    var body: some Widget {
        NACUTWidget()
    }
}

struct NACUTWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "NACUT", provider: UsageProvider()) { entry in
            WidgetView(entry: entry)
                .containerBackground(.fill.tertiary, for: .widget)
                .widgetURL(URL(string: "nacut://open"))
        }
        .configurationDisplayName("NACUT")
        .description("Your Claude plan limits or spend, with this cycle's total and forecast.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

// MARK: - Palette (mirrors the app theme)

private enum W {
    static let coral = Color(red: 0.851, green: 0.467, blue: 0.341)
    static let ramp: [Color] = [
        Color(red: 0.984, green: 0.914, blue: 0.886),
        Color(red: 0.961, green: 0.788, blue: 0.722),
        Color(red: 0.929, green: 0.647, blue: 0.545),
        Color(red: 0.878, green: 0.502, blue: 0.373),
        Color(red: 0.761, green: 0.369, blue: 0.239),
    ]
    static func status(_ s: SnapshotStatus) -> Color {
        switch s {
        case .noBudget, .underBudget: return .green
        case .tight: return .orange
        case .willExceed: return Color(red: 0.95, green: 0.45, blue: 0.09)
        case .exceeded: return .red
        }
    }
    static func symbol(_ s: SnapshotStatus) -> String {
        switch s {
        case .noBudget, .underBudget: return "checkmark.circle.fill"
        case .tight: return "exclamationmark.circle.fill"
        case .willExceed: return "exclamationmark.triangle.fill"
        case .exceeded: return "xmark.octagon.fill"
        }
    }
}

struct WidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: UsageEntry

    var body: some View {
        let s = entry.snapshot
        if s.showLimits == true && !s.limits.isEmpty {
            switch family {
            case .systemSmall: LimitsSmallView(s: s, date: entry.date)
            case .systemMedium: LimitsMediumView(s: s, date: entry.date)
            default: LimitsLargeView(s: s, date: entry.date)
            }
        } else {
            switch family {
            case .systemSmall: SmallView(s: s)
            case .systemMedium: MediumView(s: s)
            default: LargeView(s: s)
            }
        }
    }
}

private struct Ring: View {
    let s: WidgetSnapshot
    var lineWidth: CGFloat = 7

    var body: some View {
        let used = s.budget > 0 ? s.cycleSpent / s.budget : (s.limits.map(\.utilization).max() ?? 0)
        let projected = s.budget > 0 ? s.projected / s.budget : 0
        ZStack {
            Circle().stroke(.fill.tertiary, lineWidth: lineWidth)
            Circle().trim(from: 0, to: min(projected, 1))
                .stroke(W.status(s.status).opacity(0.25), style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Circle().trim(from: 0, to: min(used, 1))
                .stroke(used >= 1 ? .red : W.coral, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text(used.formatted(.percent.precision(.fractionLength(0))))
                .font(.system(size: 12, weight: .semibold, design: .rounded))
                .monospacedDigit()
        }
    }
}

private struct SmallView: View {
    let s: WidgetSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Today").font(.caption).foregroundStyle(.secondary)
                    Text(WidgetUSD.string(s.today))
                        .font(.system(size: 22, weight: .semibold, design: .rounded))
                        .monospacedDigit()
                        .minimumScaleFactor(0.6)
                }
                Spacer(minLength: 0)
                if s.budget > 0 || !s.limits.isEmpty { Ring(s: s, lineWidth: 5).frame(width: 40, height: 40) }
            }
            Spacer(minLength: 0)
            Text(s.isSubscription ? "API value this cycle" : "This cycle").font(.caption2).foregroundStyle(.secondary)
            Text(WidgetUSD.string(s.cycleSpent))
                .font(.system(.title3, design: .rounded).weight(.semibold))
                .monospacedDigit()
            Label(shortHeadline, systemImage: W.symbol(s.status))
                .font(.caption2.weight(.medium))
                .foregroundStyle(W.status(s.status))
                .lineLimit(1)
                .minimumScaleFactor(0.8)
        }
    }

    private var shortHeadline: String {
        s.budget > 0 ? "→ \(WidgetUSD.string(s.projected)) of \(WidgetUSD.string(s.budget))"
                     : "→ \(WidgetUSD.string(s.projected)) projected"
    }
}

private struct Sparkline: View {
    let values: [Double]

    var body: some View {
        let m = max(values.max() ?? 0, 0.0001)
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(Array(values.enumerated()), id: \.offset) { i, v in
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(i == values.count - 1 ? W.coral : W.coral.opacity(0.5))
                    .frame(height: max(CGFloat(v / m) * 34, v > 0 ? 2 : 1))
            }
        }
        .frame(height: 34, alignment: .bottom)
    }
}

private struct MediumView: View {
    let s: WidgetSnapshot

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text(s.isSubscription ? "API value this cycle" : "This cycle").font(.caption).foregroundStyle(.secondary)
                Text(WidgetUSD.string(s.cycleSpent))
                    .font(.system(size: 28, weight: .semibold, design: .rounded)).monospacedDigit()
                    .minimumScaleFactor(0.6)
                HStack(spacing: 4) {
                    Text("Today").foregroundStyle(.secondary)
                    Text(WidgetUSD.string(s.today)).fontWeight(.semibold)
                }
                .font(.callout).monospacedDigit()
                Spacer(minLength: 0)
                Label(s.headline, systemImage: W.symbol(s.status))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(W.status(s.status))
                    .lineLimit(2)
            }
            VStack(alignment: .trailing, spacing: 8) {
                if s.budget > 0 || !s.limits.isEmpty { Ring(s: s).frame(width: 56, height: 56) }
                Spacer(minLength: 0)
                Sparkline(values: s.last14)
                Text("Last 14 days").font(.caption2).foregroundStyle(.secondary)
            }
            .frame(width: 120)
        }
    }
}

private struct LargeView: View {
    let s: WidgetSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            MediumView(s: s).frame(height: 120)
            Divider()
            Text(s.monthTitle).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            MiniCalendar(s: s)
            if !s.topModels.isEmpty {
                HStack(spacing: 10) {
                    ForEach(s.topModels, id: \.name) { m in
                        VStack(alignment: .leading, spacing: 0) {
                            Text(m.name).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                            Text(WidgetUSD.string(m.cost)).font(.caption.weight(.semibold)).monospacedDigit()
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }
}

private struct MiniCalendar: View {
    let s: WidgetSnapshot

    var body: some View {
        let cal = Calendar.current
        let month = cal.dateInterval(of: .month, for: s.generatedAt)!.start
        let count = cal.range(of: .day, in: .month, for: month)!.count
        let lead = (cal.component(.weekday, from: month) - cal.firstWeekday + 7) % 7
        let maxV = max(s.monthDays.values.max() ?? 0, 0.0001)
        let today = cal.component(.day, from: s.generatedAt)
        let cells: [Int?] = Array(repeating: nil, count: lead) + (1...count).map { Optional($0) }
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 3), count: 7), spacing: 3) {
            ForEach(Array(cells.enumerated()), id: \.offset) { _, d in
                if let d {
                    let v = s.monthDays[d] ?? 0
                    let f = v / maxV
                    RoundedRectangle(cornerRadius: 3)
                        .fill(v > 0 ? W.ramp[min(Int((f * 5).rounded(.up)) - 1, 4).clamped] : Color.secondary.opacity(0.12))
                        .overlay(Text("\(d)").font(.system(size: 8)).foregroundStyle(f > 0.6 ? .white : .secondary))
                        .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(d == today ? W.coral : .clear, lineWidth: 1.2))
                        .frame(height: 16)
                } else {
                    Color.clear.frame(height: 16)
                }
            }
        }
    }
}

// MARK: - Plan-limit views

private extension WidgetSnapshot.Limit {
    var title: String {
        switch label {
        case "5h": return "5-hour"
        case "Week": return "Weekly"
        default: return label
        }
    }
    func resetText(at date: Date) -> String {
        guard let resetsAt, resetsAt > date else { return "reset" }
        let secs = resetsAt.timeIntervalSince(date)
        if secs >= 86_400 { return resetsAt.formatted(.dateTime.weekday(.abbreviated).hour()) }
        let h = Int(secs) / 3600, m = (Int(secs) % 3600) / 60
        return h > 0 ? "\(h)h \(m)m" : "\(m)m"
    }
}

private func limitColor(_ u: Double) -> Color {
    u >= 0.9 ? .red : u >= 0.7 ? .orange : W.coral
}

private struct LimitRingW: View {
    let limit: WidgetSnapshot.Limit
    let date: Date
    var size: CGFloat = 50
    var lineWidth: CGFloat = 6

    var body: some View {
        let u = limit.effectiveUtilization(at: date)
        VStack(spacing: 3) {
            ZStack {
                Circle().stroke(.fill.tertiary, lineWidth: lineWidth)
                Circle().trim(from: 0, to: min(max(u, 0.004), 1))
                    .stroke(limitColor(u), style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Text(u.formatted(.percent.precision(.fractionLength(0))))
                    .font(.system(size: size * 0.26, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .minimumScaleFactor(0.7)
            }
            .frame(width: size, height: size)
            Text(limit.title).font(.caption2.weight(.medium)).lineLimit(1)
            Text(limit.resetText(at: date)).font(.system(size: 9)).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity)
    }
}

private struct APIValueLine: View {
    let s: WidgetSnapshot
    var body: some View {
        HStack(spacing: 3) {
            Text("API value").foregroundStyle(.secondary)
            Text(WidgetUSD.string(s.cycleSpent)).fontWeight(.semibold)
            Text("· today").foregroundStyle(.secondary)
            Text(WidgetUSD.string(s.today)).fontWeight(.semibold)
        }
        .font(.caption2)
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }
}

private struct LimitsSmallView: View {
    let s: WidgetSnapshot
    let date: Date

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Plan limits").font(.caption.weight(.medium)).foregroundStyle(.secondary)
            Spacer(minLength: 0)
            HStack(spacing: 6) {
                ForEach(s.limits.prefix(2), id: \.label) { LimitRingW(limit: $0, date: date, size: 52) }
            }
            Spacer(minLength: 0)
            HStack(spacing: 3) {
                Text("Today").foregroundStyle(.secondary)
                Text(WidgetUSD.string(s.today)).fontWeight(.semibold)
                Text("API value").foregroundStyle(.tertiary)
            }
            .font(.caption2).monospacedDigit().lineLimit(1).minimumScaleFactor(0.8)
        }
    }
}

private struct LimitsMediumView: View {
    let s: WidgetSnapshot
    let date: Date

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Plan limits").font(.caption.weight(.medium)).foregroundStyle(.secondary)
            HStack(spacing: 8) {
                ForEach(s.limits.prefix(3), id: \.label) { LimitRingW(limit: $0, date: date, size: 58, lineWidth: 7) }
            }
            Spacer(minLength: 0)
            APIValueLine(s: s)
        }
    }
}

private struct LimitsLargeView: View {
    let s: WidgetSnapshot
    let date: Date

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            LimitsMediumView(s: s, date: date).frame(height: 140)
            Divider()
            Text("\(s.monthTitle) · API value").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            MiniCalendar(s: s)
            Spacer(minLength: 0)
        }
    }
}

private extension Int {
    var clamped: Int { Swift.max(self, 0) }
}

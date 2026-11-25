import SwiftUI
import NacutCore

/// Budget ring: solid arc for spent, faint arc to the projected total.
struct BudgetRing<Center: View>: View {
    var spent: Double
    var projected: Double
    var status: BudgetStatus
    @ViewBuilder var center: Center
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            Circle().stroke(.fill.tertiary, lineWidth: 9)
            Circle()
                .trim(from: 0, to: min(projected, 1))
                .stroke(Theme.statusColor(status).opacity(0.22), style: StrokeStyle(lineWidth: 9, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Circle()
                .trim(from: 0, to: min(spent, 1))
                .stroke(spent >= 1 ? Theme.statusColor(.exceeded) : Theme.coral,
                        style: StrokeStyle(lineWidth: 9, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(reduceMotion ? nil : .smooth(duration: 0.6), value: spent)
            center
        }
        .padding(5)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Budget \(spent.percent) used, projected \(projected.percent)")
    }
}

struct StatTile: View {
    var title: String
    var value: String
    var detail: String?
    var symbol: String
    var tint: Color = Theme.coral
    var help: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label(title, systemImage: symbol)
                .font(.caption)
                .foregroundStyle(.secondary)
                .labelStyle(TintedIconLabel(tint: tint))
            Text(value)
                .font(.system(.title3, design: .rounded).weight(.semibold))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            if let detail {
                Text(detail).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(.separator.opacity(0.5), lineWidth: 0.5))
        .help(help ?? "")
        .accessibilityElement(children: .combine)
    }
}

struct TintedIconLabel: LabelStyle {
    var tint: Color
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.icon.foregroundStyle(tint).imageScale(.small)
            configuration.title
        }
    }
}

struct LimitGauge: View {
    let limit: LimitView

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(limit.id == "seven_day" ? "Weekly" : limit.label).font(.caption.weight(.medium)).lineLimit(1)
                Spacer(minLength: 4)
                Text(limit.utilization.percent).font(.caption.weight(.semibold)).monospacedDigit()
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(.fill.tertiary)
                    if let p = limit.projectedAtReset, p > limit.utilization {
                        Capsule().fill(Theme.limitColor(p).opacity(0.25))
                            .frame(width: geo.size.width * min(p, 1))
                    }
                    Capsule().fill(Theme.limitColor(limit.utilization))
                        .frame(width: max(geo.size.width * min(limit.utilization, 1), 4))
                }
            }
            .frame(height: 6)
            Text(limit.subtitle)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(8)
        .frame(maxWidth: .infinity)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .help(limit.helpText)
        .accessibilityElement(children: .combine)
    }
}

extension LimitView {
    var helpText: String {
        var s = "\(label): \(utilization.percent) used. \(resetText)."
        if let p = projectedAtReset { s += " On pace for \(p.percent) by reset." }
        return s
    }
}

/// Horizontal share bar with a leading color swatch.
struct ShareBar: View {
    var fraction: Double
    var color: Color

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(.fill.quaternary)
                Capsule().fill(color).frame(width: max(geo.size.width * min(max(fraction, 0), 1), 3))
            }
        }
        .frame(height: 5)
    }
}

struct RangePicker: View {
    @Binding var range: RangeOption

    var body: some View {
        Picker("Range", selection: $range) {
            ForEach(RangeOption.allCases) { Text($0.title).tag($0) }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .controlSize(.small)
    }
}

struct EmptyHint: View {
    var text: String
    var body: some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, minHeight: 60)
    }
}

import Charts
import SwiftUI
import UsageCore

struct ModelsTab: View {
    @Environment(AppModel.self) private var model
    @AppStorage("modelsRange") private var range: StatRange = .cycle
    @State private var selectedAngle: Double?

    var body: some View {
        let interval = range.interval(model)
        let stats = model.store.modelStats(in: interval)
        let total = stats.reduce(0) { $0 + $1.cost }
        VStack(spacing: 12) {
            RangePicker(range: $range)

            if stats.isEmpty {
                Card { EmptyHint(text: "No model usage in this range") }
            } else {
                Card("Cost by model") {
                    HStack(spacing: 16) {
                        Chart(stats) { s in
                            SectorMark(angle: .value("Cost", s.cost), innerRadius: .ratio(0.64), angularInset: 1.5)
                                .cornerRadius(3)
                                .foregroundStyle(Theme.color(forModel: s.id))
                                .opacity(selectedModel(stats)?.id == nil || selectedModel(stats)?.id == s.id ? 1 : 0.35)
                        }
                        .chartAngleSelection(value: $selectedAngle)
                        .chartBackground { proxy in
                            GeometryReader { geo in
                                if let frame = proxy.plotFrame.map({ geo[$0] }) {
                                    let sel = selectedModel(stats)
                                    VStack(spacing: 0) {
                                        Text(sel?.name ?? "Total").font(.caption2).foregroundStyle(.secondary)
                                        Text((sel?.cost ?? total).usdCompact)
                                            .font(.system(.headline, design: .rounded)).monospacedDigit()
                                    }
                                    .position(x: frame.midX, y: frame.midY)
                                }
                            }
                        }
                        .frame(width: 130, height: 130)

                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(stats.prefix(6)) { s in
                                HStack(spacing: 6) {
                                    RoundedRectangle(cornerRadius: 2).fill(Theme.color(forModel: s.id)).frame(width: 10, height: 10)
                                    Text(s.name).font(.callout).lineLimit(1)
                                    Spacer(minLength: 4)
                                    Text(s.share.percent).font(.callout).foregroundStyle(.secondary).monospacedDigit()
                                }
                            }
                        }
                    }
                }

                ForEach(stats) { ModelRow(stat: $0) }

                TokenClassCard(interval: interval)

                if stats.contains(where: \.estimated) {
                    Label("Some models aren't in the bundled price list, so they're priced like the newest model in their family. Add exact prices in Settings → Pricing.",
                          systemImage: "info.circle")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }

            if !model.reconciliation.isEmpty {
                ReconciliationCard()
            }
        }
    }

    private func selectedModel(_ stats: [ModelStat]) -> ModelStat? {
        guard let a = selectedAngle else { return nil }
        var running = 0.0
        for s in stats {
            running += s.cost
            if a <= running { return s }
        }
        return nil
    }
}

struct ModelRow: View {
    let stat: ModelStat

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Circle().fill(Theme.color(forModel: stat.id)).frame(width: 9, height: 9)
                Text(stat.name).font(.body.weight(.semibold))
                if stat.estimated {
                    Image(systemName: "questionmark.circle").foregroundStyle(.secondary).help("Estimated price")
                }
                Spacer()
                Text(stat.cost.usd).font(.body.weight(.semibold)).monospacedDigit()
                    .help(CurrencyFormat.exact(stat.cost))
            }
            ShareBar(fraction: stat.share, color: Theme.color(forModel: stat.id))
            HStack(spacing: 10) {
                metric("Messages", stat.messages.formatted())
                metric("Tokens", CurrencyFormat.tokens(stat.tokens))
                metric("Per msg", CurrencyFormat.exact(stat.costPerMessage))
                metric("Fast", stat.cost > 0 ? (stat.fastCost / stat.cost).percent : "0%")
            }
            HStack(spacing: 10) {
                metric("Input", CurrencyFormat.tokens(stat.input))
                metric("Output", CurrencyFormat.tokens(stat.output))
                metric("Cache write", CurrencyFormat.tokens(stat.cacheWrite))
                metric("Cache read", CurrencyFormat.tokens(stat.cacheRead))
            }
        }
        .padding(12)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(.separator.opacity(0.5), lineWidth: 0.5))
    }

    private func metric(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(title).font(.caption2).foregroundStyle(.secondary)
            Text(value).font(.caption.weight(.medium)).monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct TokenClassCard: View {
    @Environment(AppModel.self) private var model
    let interval: DateInterval

    var body: some View {
        let classes = model.store.tokenClasses(in: interval)
        let total = max(classes.reduce(0) { $0 + $1.value }, 0.0001)
        Card("Where the money goes") {
            Chart(classes) { c in
                BarMark(x: .value("Cost", c.value), y: .value("Type", c.name))
                    .foregroundStyle(Theme.coral)
                    .cornerRadius(3)
                    .annotation(position: .trailing) {
                        Text("\(c.value.usdCompact) · \(CurrencyFormat.tokens(c.tokens)) tok")
                            .font(.caption2).foregroundStyle(.secondary).monospacedDigit()
                    }
            }
            .chartXAxis(.hidden)
            .chartXScale(domain: 0...(classes.map(\.value).max() ?? 1) * 1.6)
            .frame(height: 120)
            Text(insight(classes, total: total)).font(.caption).foregroundStyle(.secondary)
        }
    }

    private func insight(_ c: [NamedValue], total: Double) -> String {
        guard let top = c.max(by: { $0.value < $1.value }), top.value > 0 else { return "" }
        return "\(top.name) is \((top.value / total).percent) of cost in this range."
    }
}

struct ReconciliationCard: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let days = model.reconciliation
        let api = days.reduce(0) { $0 + $1.api }
        let logs = days.reduce(0) { $0 + $1.logs }
        Card("Billed vs local estimate") {
            Chart {
                ForEach(days) { d in
                    BarMark(x: .value("Day", d.date, unit: .day), y: .value("Cost", d.api))
                        .foregroundStyle(by: .value("Source", "Billed (API)"))
                        .position(by: .value("Source", "Billed (API)"))
                    BarMark(x: .value("Day", d.date, unit: .day), y: .value("Cost", d.logs))
                        .foregroundStyle(by: .value("Source", "Local logs"))
                        .position(by: .value("Source", "Local logs"))
                }
            }
            .chartForegroundStyleScale(domain: ["Billed (API)", "Local logs"], range: [Theme.categorical[0], Theme.coral])
            .chartLegend(position: .bottom, alignment: .leading)
            .chartYAxis {
                AxisMarks(position: .leading) { v in
                    AxisGridLine().foregroundStyle(.separator.opacity(0.5))
                    AxisValueLabel { if let d = v.as(Double.self) { Text(d.usdCompact) } }
                }
            }
            .frame(height: 130)
            Text(logs > 0
                 ? "Over \(days.count) billed days the API shows \(api.usd) vs \(logs.usd) from local logs (\((api / logs).percent)). Live estimates are scaled by ×\(String(format: "%.2f", model.calibration.ratio))."
                 : "No overlapping days yet.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }
}

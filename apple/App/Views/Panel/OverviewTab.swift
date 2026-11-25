import Charts
import SwiftUI
import NacutCore

struct OverviewTab: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let f = model.forecast
        let i = model.insights
        VStack(spacing: 12) {
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 3), spacing: 8) {
                StatTile(title: "Today", value: f.today.usd,
                         detail: todayDetail, symbol: "sun.max.fill")
                StatTile(title: "Daily average", value: f.dailyAverage.usd,
                         detail: "7-day \(f.trailing7Average.usd)", symbol: "chart.line.flattrend.xyaxis",
                         help: "Cycle-to-date spend divided by days elapsed. The 7-day figure is the recent run rate.")
                StatTile(title: "Projected", value: f.projected.usd,
                         detail: "by \(f.cycle.lastDay.formatted(.dateTime.month(.abbreviated).day()))",
                         symbol: "arrow.up.right", tint: Theme.statusColor(f.status),
                         help: "Spend so far plus the blended daily rate × days remaining")
                StatTile(title: "Last 7 days", value: i.last7Days.usd,
                         detail: "30 days \(i.last30Days.usdCompact)", symbol: "calendar")
                StatTile(title: "vs last cycle", value: vsText(i.vsPreviousCycle),
                         detail: "Last cycle \(i.previousCycleTotal.usdCompact)",
                         symbol: (i.vsPreviousCycle ?? 0) >= 0 ? "arrow.up.forward" : "arrow.down.forward",
                         help: "This cycle so far compared with the same point in the previous cycle")
                StatTile(title: "All time", value: i.allTime.usdCompact,
                         detail: model.firstRecordDate.map { "since \($0.formatted(.dateTime.month(.abbreviated).year()))" },
                         symbol: "sum")
            }

            if let through = model.billedThrough {
                Label("Billed data through \(through.formatted(.dateTime.month(.abbreviated).day().hour().minute())). Later usage is a live estimate\(model.calibrationRatio != 1 ? String(format: " (×%.2f calibrated)", model.calibrationRatio) : "").",
                      systemImage: "checkmark.seal")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            CumulativeChart(forecast: f)
            DailyBarsChart()
            CacheCard()
            InsightsRow()
        }
    }

    private var todayDetail: String {
        let msgs = model.series[Calendar.current.startOfDay(for: .now)]?.messages ?? 0
        return msgs > 0 ? "\(msgs) messages" : "No usage yet"
    }

    private func vsText(_ v: Double?) -> String {
        guard let v else { return "—" }
        return (v >= 0 ? "+" : "") + v.formatted(.percent.precision(.fractionLength(0)))
    }
}

// MARK: - Cumulative spend vs budget

struct CumulativeChart: View {
    let forecast: ForecastView
    @State private var selected: Date?

    private let kinds: [String] = ["Actual", "Projected", "Budget pace"]

    var body: some View {
        Card("Cycle spend", trailing: AnyView(legend)) {
            Chart {
                ForEach(forecast.points) { p in
                    LineMark(
                        x: .value("Day", p.day, unit: .day),
                        y: .value("Cumulative", p.cumulative),
                        series: .value("Series", name(p.kind))
                    )
                    .foregroundStyle(by: .value("Series", name(p.kind)))
                    .lineStyle(p.kind == .actual ? StrokeStyle(lineWidth: 2.5, lineCap: .round)
                               : StrokeStyle(lineWidth: 2, lineCap: .round, dash: [4, 4]))
                    .interpolationMethod(.monotone)
                }
                ForEach(forecast.points.filter { $0.kind == .actual }) { p in
                    AreaMark(x: .value("Day", p.day, unit: .day), y: .value("Cumulative", p.cumulative))
                        .foregroundStyle(LinearGradient(colors: [Theme.coral.opacity(0.22), Theme.coral.opacity(0.02)],
                                                        startPoint: .top, endPoint: .bottom))
                        .interpolationMethod(.monotone)
                }
                if forecast.budget > 0 {
                    RuleMark(y: .value("Budget", forecast.budget))
                        .foregroundStyle(.secondary.opacity(0.6))
                        .lineStyle(StrokeStyle(lineWidth: 1))
                        .annotation(position: .top, alignment: .leading) {
                            Text("Budget \(forecast.budget.usdCompact)").font(.caption2).foregroundStyle(.secondary)
                        }
                }
                if let selected {
                    RuleMark(x: .value("Selected", selected, unit: .day))
                        .foregroundStyle(.secondary.opacity(0.4))
                        .annotation(position: .top, overflowResolution: .init(x: .fit(to: .chart), y: .disabled)) {
                            tooltip(for: selected)
                        }
                }
            }
            .chartForegroundStyleScale(domain: kinds, range: [Theme.coral, Theme.coral.opacity(0.55), Color.secondary.opacity(0.5)])
            .chartLegend(.hidden)
            .chartXSelection(value: $selected)
            .chartYAxis {
                AxisMarks(position: .leading) { v in
                    AxisGridLine().foregroundStyle(.separator.opacity(0.5))
                    AxisValueLabel { if let d = v.as(Double.self) { Text(d.usdCompact) } }
                }
            }
            .chartXAxis {
                AxisMarks(values: .stride(by: .day, count: 7)) { _ in
                    AxisValueLabel(format: .dateTime.month(.abbreviated).day(), collisionResolution: .greedy(minimumSpacing: 4))
                }
            }
            .chartXScale(domain: forecast.cycle.start...forecast.cycle.end)
            .frame(height: 160)
            .accessibilityLabel("Cumulative spend this cycle with projection")
        }
    }

    private var legend: some View {
        HStack(spacing: 10) {
            LegendSwatch(color: Theme.coral, label: "Actual")
            LegendSwatch(color: Theme.coral.opacity(0.55), label: "Projected", dashed: true)
            if forecast.budget > 0 { LegendSwatch(color: .secondary.opacity(0.5), label: "Pace", dashed: true) }
        }
    }

    private func name(_ k: PointKind) -> String {
        switch k {
        case .actual: return "Actual"
        case .projected: return "Projected"
        case .pace: return "Budget pace"
        }
    }

    @ViewBuilder private func tooltip(for date: Date) -> some View {
        let cal = Calendar.current
        let day = cal.startOfDay(for: date)
        let actual = forecast.points.first { $0.kind == .actual && $0.day == day }
        let projected = forecast.points.first { $0.kind == .projected && $0.day == day }
        let pace = forecast.points.first { $0.kind == .pace && $0.day == day }
        VStack(alignment: .leading, spacing: 2) {
            Text(day.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day())).font(.caption.weight(.semibold))
            if let a = actual { Text("Spent \(a.cumulative.usd)") }
            else if let p = projected { Text("Projected \(p.cumulative.usd)") }
            if let p = pace { Text("Pace \(p.cumulative.usd)").foregroundStyle(.secondary) }
        }
        .font(.caption)
        .monospacedDigit()
        .padding(6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
    }
}

struct LegendSwatch: View {
    var color: Color
    var label: String
    var dashed = false

    var body: some View {
        HStack(spacing: 4) {
            if dashed {
                Path { p in p.move(to: .init(x: 0, y: 3)); p.addLine(to: .init(x: 12, y: 3)) }
                    .stroke(color, style: StrokeStyle(lineWidth: 2, dash: [3, 2]))
                    .frame(width: 12, height: 6)
            } else {
                RoundedRectangle(cornerRadius: 2).fill(color).frame(width: 10, height: 10)
            }
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
    }
}

// MARK: - Last 30 days by model

struct DailyBarsChart: View {
    @Environment(AppModel.self) private var model
    @State private var selected: Date?

    struct Row: Identifiable {
        var id: String { "\(date.timeIntervalSince1970)-\(model)" }
        var date: Date
        var model: String
        var cost: Double
    }

    var body: some View {
        let days = model.lastDays(30)
        let rows: [Row] = days.flatMap { day in
            var out = day.byModel.map { Row(date: day.day, model: model.displayName($0.key), cost: $0.value) }
            let rest = day.cost - day.byModel.values.reduce(0, +)
            if rest > 0.005 { out.append(Row(date: day.day, model: "Other", cost: rest)) }
            return out
        }
        let modelIds = Array(Set(days.flatMap { $0.byModel.keys }))
            .sorted { (Theme.slotOrder($0), $0) < (Theme.slotOrder($1), $1) }
        var domain = modelIds.map { model.displayName($0) }
        var range = modelIds.map { Theme.color(forModel: $0) }
        if rows.contains(where: { $0.model == "Other" }) { domain.append("Other"); range.append(Theme.other) }
        let avg = days.map(\.cost).reduce(0, +) / 30

        return Card("Last 30 days", trailing: AnyView(Text("avg \(avg.usd)/day").font(.caption).foregroundStyle(.secondary))) {
            if rows.isEmpty {
                EmptyHint(text: "No usage in the last 30 days")
            } else {
                Chart {
                    ForEach(rows) { r in
                        BarMark(x: .value("Day", r.date, unit: .day), y: .value("Cost", r.cost))
                            .foregroundStyle(by: .value("Model", r.model))
                            .cornerRadius(2)
                    }
                    RuleMark(y: .value("Average", avg))
                        .foregroundStyle(.secondary.opacity(0.5))
                        .lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                    if let selected {
                        RuleMark(x: .value("Selected", selected, unit: .day))
                            .foregroundStyle(.secondary.opacity(0.25))
                            .annotation(position: .top, overflowResolution: .init(x: .fit(to: .chart), y: .disabled)) {
                                dayTooltip(Calendar.current.startOfDay(for: selected), rows: rows, domain: domain, range: range)
                            }
                    }
                }
                .chartForegroundStyleScale(domain: domain, range: range)
                .chartLegend(position: .bottom, alignment: .leading, spacing: 8)
                .chartXSelection(value: $selected)
                .chartYAxis {
                    AxisMarks(position: .leading) { v in
                        AxisGridLine().foregroundStyle(.separator.opacity(0.5))
                        AxisValueLabel { if let d = v.as(Double.self) { Text(d.usdCompact) } }
                    }
                }
                .chartXAxis {
                    AxisMarks(values: .stride(by: .day, count: 7)) { _ in
                        AxisValueLabel(format: .dateTime.month(.abbreviated).day())
                    }
                }
                .frame(height: 170)
            }
        }
    }

    @ViewBuilder
    private func dayTooltip(_ day: Date, rows: [Row], domain: [String], range: [Color]) -> some View {
        let items = rows.filter { $0.date == day }.sorted { $0.cost > $1.cost }
        VStack(alignment: .leading, spacing: 2) {
            Text(day.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day()))
                .font(.caption.weight(.semibold))
            Text("Total \(items.reduce(0) { $0 + $1.cost }.usd)").font(.caption.weight(.medium))
            ForEach(items) { r in
                HStack(spacing: 4) {
                    Circle().fill(range[domain.firstIndex(of: r.model) ?? 0]).frame(width: 6, height: 6)
                    Text("\(r.model) \(r.cost.usd)")
                }
            }
        }
        .font(.caption2)
        .monospacedDigit()
        .padding(6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
    }
}

// MARK: - Cache & insights

struct CacheCard: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let savings = model.cacheSavings(.cycle)
        let classes = model.tokenClasses(.cycle)
        let totalTokens = max(classes.reduce(0) { $0 + $1.tokens }, 1)
        let readShare = Double(classes.first { $0.name == "Cache read" }?.tokens ?? 0) / Double(totalTokens)
        Card("Prompt caching") {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(savings.usd).font(.system(.title2, design: .rounded).weight(.semibold)).monospacedDigit()
                        .foregroundStyle(Theme.statusColor(.underBudget))
                    Text("saved this cycle by cache reads").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 2) {
                    Text(readShare.percent).font(.system(.title3, design: .rounded).weight(.semibold)).monospacedDigit()
                    Text("of tokens were cache hits").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .help("What cache-read tokens would have cost at the normal input price, minus what they actually cost")
    }
}

struct InsightsRow: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let i = model.insights
        let hour = model.snap.busiestHourLast30.map(Int.init)
        let messages = model.modelStats(.cycle).reduce(0) { $0 + $1.messages }
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 2), spacing: 8) {
            StatTile(title: "Peak day", value: i.peakDay == nil ? "—" : i.peakCost.usd,
                     detail: i.peakDay?.date.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day().year()),
                     symbol: "flame.fill")
            StatTile(title: "Active streak", value: "\(i.activeStreak) day\(i.activeStreak == 1 ? "" : "s")",
                     detail: "\(i.activeDaysThisCycle) active days this cycle", symbol: "bolt.fill")
            StatTile(title: "Busiest hour", value: hour.map(Self.hourLabel) ?? "—",
                     detail: "last 30 days", symbol: "clock.fill")
            StatTile(title: "Messages", value: messages.formatted(),
                     detail: "this cycle", symbol: "bubble.left.and.text.bubble.right.fill")
        }
    }

    static func hourLabel(_ h: Int) -> String {
        let d = Calendar.current.date(bySettingHour: h, minute: 0, second: 0, of: .now)!
        return d.formatted(.dateTime.hour())
    }
}

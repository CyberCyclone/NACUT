import Charts
import SwiftUI
import NacutCore

struct CalendarTab: View {
    @Environment(AppModel.self) private var model
    @State private var visibleMonth: Date?
    @State private var selectedDay: Date?
    @FocusState private var focused: Bool

    private var cal: Calendar { .current }

    private var months: [Date] {
        let now = cal.dateInterval(of: .month, for: .now)!.start
        let first = model.series.keys.min().flatMap { cal.dateInterval(of: .month, for: $0)?.start } ?? now
        var out: [Date] = []
        var m = min(first, now)
        while m <= now {
            out.append(m)
            m = cal.date(byAdding: .month, value: 1, to: m)!
        }
        return out
    }

    /// Scale shared by all months so colors are comparable: the 95th percentile active day.
    private var scaleMax: Double {
        let costs = model.series.values.map(\.cost).filter { $0 > 0 }.sorted()
        guard !costs.isEmpty else { return 1 }
        return max(costs[min(Int(Double(costs.count) * 0.95), costs.count - 1)], 0.01)
    }

    var body: some View {
        let months = months
        let current = visibleMonth ?? months.last!
        VStack(spacing: 12) {
            HStack {
                Text(current.formatted(.dateTime.month(.wide).year()))
                    .font(.title3.weight(.semibold))
                    .contentTransition(.numericText())
                Spacer()
                Button("Today") { go(to: months.last!) }
                    .controlSize(.small)
                    .disabled(current == months.last)
                ControlGroup {
                    Button { step(-1, months) } label: { Image(systemName: "chevron.left") }
                        .disabled(current == months.first)
                    Button { step(1, months) } label: { Image(systemName: "chevron.right") }
                        .disabled(current == months.last)
                }
                .controlSize(.small)
                .frame(width: 70)
            }

            ScrollView(.horizontal) {
                LazyHStack(spacing: 0) {
                    ForEach(months, id: \.self) { m in
                        MonthGrid(month: m, scaleMax: scaleMax, selected: $selectedDay)
                            .containerRelativeFrame(.horizontal)
                            .id(m)
                    }
                }
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.paging)
            .scrollPosition(id: $visibleMonth)
            .scrollIndicators(.never)
            .frame(height: 290)
            .focusable()
            .focused($focused)
            .focusEffectDisabled()
            .onKeyPress(.leftArrow) { step(-1, months); return .handled }
            .onKeyPress(.rightArrow) { step(1, months); return .handled }
            .onAppear {
                if visibleMonth == nil { visibleMonth = months.last }
                focused = true
            }

            MonthSummary(month: current)

            if let day = selectedDay {
                DayDetailView(day: day) { selectedDay = nil }
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            } else {
                Text("Swipe with two fingers or use ← → to change month. Click a day for details.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .animation(.smooth(duration: 0.25), value: selectedDay)
    }

    private func go(to m: Date) {
        withAnimation(.smooth) { visibleMonth = m }
    }

    private func step(_ delta: Int, _ months: [Date]) {
        let current = visibleMonth ?? months.last!
        guard let i = months.firstIndex(of: current) else { return }
        let j = min(max(i + delta, 0), months.count - 1)
        go(to: months[j])
    }
}

struct MonthGrid: View {
    @Environment(AppModel.self) private var model
    let month: Date
    let scaleMax: Double
    @Binding var selected: Date?

    private var cal: Calendar { .current }

    var body: some View {
        let days = gridDays()
        let symbols = weekdaySymbols()
        VStack(spacing: 4) {
            HStack(spacing: 4) {
                ForEach(symbols, id: \.self) { s in
                    Text(s).font(.caption2.weight(.medium)).foregroundStyle(.secondary).frame(maxWidth: .infinity)
                }
            }
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 4), count: 7), spacing: 4) {
                ForEach(Array(days.enumerated()), id: \.offset) { _, day in
                    if let day {
                        DayCell(day: day, cost: model.series[day], scaleMax: scaleMax,
                                isSelected: selected == day,
                                isToday: cal.isDateInToday(day),
                                isFuture: day > .now)
                            .onTapGesture { selected = (selected == day ? nil : day) }
                    } else {
                        Color.clear.frame(height: 40)
                    }
                }
            }
        }
        .padding(.horizontal, 1)
    }

    private func weekdaySymbols() -> [String] {
        let s = cal.veryShortStandaloneWeekdaySymbols
        let first = cal.firstWeekday - 1
        return Array(s[first...] + s[..<first]).enumerated().map { "\($0.element)\(String(repeating: "\u{200B}", count: $0.offset))" }
    }

    private func gridDays() -> [Date?] {
        let range = cal.range(of: .day, in: .month, for: month)!
        let weekday = cal.component(.weekday, from: month)
        let lead = (weekday - cal.firstWeekday + 7) % 7
        var out: [Date?] = Array(repeating: nil, count: lead)
        for d in range { out.append(cal.date(byAdding: .day, value: d - 1, to: month)) }
        while out.count % 7 != 0 { out.append(nil) }
        while out.count < 42 { out.append(nil) }
        return out
    }
}

struct DayCell: View {
    let day: Date
    let cost: DayEntry?
    let scaleMax: Double
    let isSelected: Bool
    let isToday: Bool
    let isFuture: Bool
    @State private var hovering = false

    var body: some View {
        let value = cost?.cost ?? 0
        let fraction = min(value / scaleMax, 1)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 2) {
                Text("\(Calendar.current.component(.day, from: day))")
                    .font(.caption2.weight(isToday ? .bold : .regular))
                if cost?.source == .estimated {
                    Circle().fill(.secondary).frame(width: 3, height: 3).help("Live estimate (not yet billed)")
                }
            }
            .foregroundStyle(isFuture ? Color.secondary.opacity(0.5) : Theme.inkOnRamp(fraction).opacity(0.75))
            Spacer(minLength: 0)
            if value > 0 {
                Text(value.usdCompact)
                    .font(.system(size: 10.5, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(Theme.inkOnRamp(fraction))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
        }
        .padding(5)
        .frame(maxWidth: .infinity, minHeight: 40, maxHeight: 40, alignment: .topLeading)
        .background(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(value > 0 ? AnyShapeStyle(Theme.rampColor(fraction)) : AnyShapeStyle(.fill.quinary))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .strokeBorder(isSelected ? Color.primary : isToday ? Theme.coral : (hovering ? Color.secondary.opacity(0.5) : .clear),
                              lineWidth: isSelected || isToday ? 1.5 : 1)
        )
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .help("\(day.formatted(.dateTime.weekday(.wide).month().day())): \(USD.exact(value))\(cost.map { " · \($0.messages) messages" } ?? "")")
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(day.formatted(date: .complete, time: .omitted)), \(value.usd)")
        .accessibilityAddTraits(.isButton)
    }
}

struct MonthSummary: View {
    @Environment(AppModel.self) private var model
    let month: Date

    var body: some View {
        let cal = Calendar.current
        let interval = cal.dateInterval(of: .month, for: month)!
        let days = model.series.filter { interval.contains($0.key) && $0.key < interval.end && $0.value.cost > 0 }
        let total = days.values.reduce(0) { $0 + $1.cost }
        let busiest = days.max { $0.value.cost < $1.value.cost }
        HStack(spacing: 8) {
            summary("Month total", total.usd)
            summary("Avg / active day", days.isEmpty ? "—" : (total / Double(days.count)).usd)
            summary("Active days", "\(days.count)")
            summary("Busiest", busiest.map { "\($0.key.formatted(.dateTime.day())) · \($0.value.cost.usdCompact)" } ?? "—")
        }
    }

    private func summary(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            Text(value).font(.callout.weight(.semibold)).monospacedDigit().lineLimit(1).minimumScaleFactor(0.8)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

struct DayDetailView: View {
    @Environment(AppModel.self) private var model
    let day: Date
    var onClose: () -> Void

    var body: some View {
        let models = model.dayModels(day)
        let hourly = model.hourly(day)
        let sessions = model.daySessions(day, limit: 5)
        let total = model.series[day]?.cost ?? 0
        let domain = models.map(\.name)
        let range = models.map { Theme.color(forModel: $0.id) }

        Card(day.formatted(.dateTime.weekday(.wide).month(.wide).day()),
             trailing: AnyView(HStack {
                 Text(total.usd).font(.headline).monospacedDigit()
                 Button(action: onClose) { Image(systemName: "xmark.circle.fill") }
                     .buttonStyle(.borderless).foregroundStyle(.secondary)
             })) {
            if models.isEmpty {
                EmptyHint(text: total > 0 ? "Billed via API · no local detail for this day" : "No usage this day")
            } else {
                Chart {
                    ForEach(Array(hourly.enumerated()), id: \.offset) { _, h in
                        BarMark(x: .value("Hour", Int(h.hour)), y: .value("Cost", h.cost), width: .fixed(9))
                            .foregroundStyle(by: .value("Model", h.model))
                            .cornerRadius(2)
                    }
                }
                .chartForegroundStyleScale(domain: domain, range: range)
                .chartXScale(domain: -0.5...23.5)
                .chartXAxis {
                    AxisMarks(values: [0, 6, 12, 18, 23]) { v in
                        AxisValueLabel { if let h = v.as(Int.self) { Text(InsightsRow.hourLabel(h)) } }
                    }
                }
                .chartYAxis {
                    AxisMarks(position: .leading) { v in
                        AxisGridLine().foregroundStyle(.separator.opacity(0.5))
                        AxisValueLabel { if let d = v.as(Double.self) { Text(d.usdCompact) } }
                    }
                }
                .chartLegend(.hidden)
                .frame(height: 110)

                ForEach(models) { m in
                    HStack {
                        Circle().fill(Theme.color(forModel: m.id)).frame(width: 8, height: 8)
                        Text(m.name).font(.callout)
                        Spacer()
                        Text("\(m.messages) msgs").font(.caption).foregroundStyle(.secondary)
                        Text(m.cost.usd).font(.callout.weight(.medium)).monospacedDigit().frame(width: 70, alignment: .trailing)
                    }
                }
                if !sessions.isEmpty {
                    Divider()
                    Text("Sessions").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                    ForEach(sessions) { SessionRow(session: $0) }
                }
            }
        }
    }
}

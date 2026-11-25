import Charts
import SwiftUI
import UsageCore

struct ProjectsTab: View {
    @Environment(AppModel.self) private var model
    @AppStorage("projectsRange") private var range: StatRange = .cycle
    @State private var showAllProjects = false

    var body: some View {
        let interval = range.interval(model)
        let projects = model.store.projectStats(in: interval)
        let sessions = model.store.sessions(in: interval, limit: 8)
        let entry = model.store.entrypointStats(in: interval)
        let split = model.store.agentSplit(in: interval)

        VStack(spacing: 12) {
            RangePicker(range: $range)

            Card("Projects", trailing: projects.count > 6 ? AnyView(
                Button(showAllProjects ? "Show less" : "Show all \(projects.count)") { showAllProjects.toggle() }
                    .buttonStyle(.link).font(.caption)) : nil) {
                if projects.isEmpty {
                    EmptyHint(text: "No project usage in this range")
                } else {
                    ForEach(showAllProjects ? projects : Array(projects.prefix(6))) { p in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Image(systemName: "folder.fill").foregroundStyle(Theme.coral).imageScale(.small)
                                Text(p.name).font(.callout.weight(.medium)).lineLimit(1)
                                Spacer()
                                Text(p.cost.usd).font(.callout.weight(.semibold)).monospacedDigit()
                            }
                            ShareBar(fraction: p.share, color: Theme.coral)
                            Text("\(p.sessions) session\(p.sessions == 1 ? "" : "s") · \(p.messages) msgs · last active \(p.lastActive.formatted(.relative(presentation: .named)))")
                                .font(.caption2).foregroundStyle(.secondary)
                        }
                        .help(p.path)
                        .padding(.vertical, 2)
                    }
                }
            }

            Card("Top sessions") {
                if sessions.isEmpty {
                    EmptyHint(text: "No sessions in this range")
                } else {
                    ForEach(sessions) { SessionRow(session: $0) }
                }
            }

            HStack(alignment: .top, spacing: 12) {
                Card("Where you use Claude") {
                    SegmentedShare(items: entry)
                }
                Card("Agents") {
                    SegmentedShare(items: [NamedValue(name: "Main agent", value: split.main),
                                           NamedValue(name: "Subagents", value: split.sub)])
                }
            }

            HeatmapCard(interval: interval)
        }
    }
}

struct SessionRow: View {
    let session: SessionStat

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(session.title).font(.callout.weight(.medium)).lineLimit(1)
                Text("\(session.project) · \(session.start.formatted(.dateTime.month(.abbreviated).day().hour().minute())) · \(duration) · \(session.entrypoint)")
                    .font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                HStack(spacing: 4) {
                    ForEach(session.models, id: \.self) { m in
                        Text(m).font(.caption2.weight(.medium))
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(.fill.tertiary, in: Capsule())
                    }
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text(session.cost.usd).font(.callout.weight(.semibold)).monospacedDigit()
                Text("\(session.messages) msgs").font(.caption2).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 3)
    }

    private var duration: String {
        let s = Int(session.duration)
        if s < 60 { return "<1m" }
        let h = s / 3600, m = (s % 3600) / 60
        if h >= 48 { return "\(h / 24)d \(h % 24)h" }
        return h > 0 ? "\(h)h \(m)m" : "\(m)m"
    }
}

/// Part-to-whole as one segmented bar with a legend (≤ 4 parts).
struct SegmentedShare: View {
    let items: [NamedValue]

    var body: some View {
        let total = max(items.reduce(0) { $0 + $1.value }, 0.0001)
        let shown = Array(items.prefix(4))
        VStack(alignment: .leading, spacing: 8) {
            GeometryReader { geo in
                HStack(spacing: 2) {
                    ForEach(Array(shown.enumerated()), id: \.offset) { i, item in
                        if item.value > 0 {
                            RoundedRectangle(cornerRadius: 3)
                                .fill(Theme.categorical[i])
                                .frame(width: max((geo.size.width - CGFloat(shown.count - 1) * 2) * item.value / total, 3))
                        }
                    }
                }
            }
            .frame(height: 10)
            ForEach(Array(shown.enumerated()), id: \.offset) { i, item in
                HStack(spacing: 6) {
                    RoundedRectangle(cornerRadius: 2).fill(Theme.categorical[i]).frame(width: 8, height: 8)
                    Text(item.name).font(.caption).lineLimit(1)
                    Spacer(minLength: 2)
                    Text((item.value / total).percent).font(.caption).foregroundStyle(.secondary).monospacedDigit()
                }
                .help(item.value.usd)
            }
        }
    }
}

struct HeatmapCard: View {
    @Environment(AppModel.self) private var model
    let interval: DateInterval
    @State private var hover: (row: Int, col: Int)?

    private let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

    var body: some View {
        let grid = model.store.weekdayHourHeatmap(in: interval)
        let maxV = max(grid.flatMap { $0 }.max() ?? 0, 0.0001)
        Card("When you use Claude", trailing: AnyView(hoverText(grid))) {
            VStack(alignment: .leading, spacing: 3) {
                ForEach(0..<7, id: \.self) { r in
                    HStack(spacing: 3) {
                        Text(days[r]).font(.caption2).foregroundStyle(.secondary).frame(width: 26, alignment: .leading)
                        ForEach(0..<24, id: \.self) { c in
                            let v = grid[r][c]
                            RoundedRectangle(cornerRadius: 2.5)
                                .fill(v > 0 ? AnyShapeStyle(Theme.rampColor(v / maxV)) : AnyShapeStyle(.fill.quaternary))
                                .frame(height: 12)
                                .overlay(RoundedRectangle(cornerRadius: 2.5)
                                    .strokeBorder(hover?.row == r && hover?.col == c ? Color.primary : .clear, lineWidth: 1))
                                .onHover { inside in hover = inside ? (r, c) : (hover?.row == r && hover?.col == c ? nil : hover) }
                        }
                    }
                }
                HStack(spacing: 3) {
                    Spacer().frame(width: 26)
                    ForEach([0, 6, 12, 18], id: \.self) { h in
                        Text(InsightsRow.hourLabel(h)).font(.caption2).foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }

    @ViewBuilder private func hoverText(_ grid: [[Double]]) -> some View {
        if let h = hover {
            Text("\(days[h.row]) \(InsightsRow.hourLabel(h.col)): \(grid[h.row][h.col].usd)")
                .font(.caption).monospacedDigit().foregroundStyle(.secondary)
        } else {
            HStack(spacing: 2) {
                Text("Less").font(.caption2).foregroundStyle(.secondary)
                ForEach(0..<Theme.ramp.count, id: \.self) { i in
                    RoundedRectangle(cornerRadius: 2).fill(Theme.ramp[i]).frame(width: 9, height: 9)
                }
                Text("More").font(.caption2).foregroundStyle(.secondary)
            }
        }
    }
}

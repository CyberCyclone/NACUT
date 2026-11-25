import SwiftUI
import UsageCore

enum PanelTab: String, CaseIterable, Identifiable {
    case overview = "Overview"
    case calendar = "Calendar"
    case models = "Models"
    case projects = "Projects"
    var id: String { rawValue }
}

struct PanelView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("panelTab") private var tab: PanelTab = .overview

    var body: some View {
        VStack(spacing: 0) {
            HeaderView()
                .padding(.horizontal, 16)
                .padding(.top, 16)
                .padding(.bottom, 12)

            Picker("View", selection: $tab) {
                ForEach(PanelTab.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(.horizontal, 16)
            .padding(.bottom, 10)

            ScrollView {
                Group {
                    switch tab {
                    case .overview: OverviewTab()
                    case .calendar: CalendarTab()
                    case .models: ModelsTab()
                    case .projects: ProjectsTab()
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 14)
            }
            .scrollIndicators(.automatic)
            .frame(height: 480)

            Divider()
            FooterView()
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
        }
        .frame(width: 452)
        .overlay {
            if !model.hasLoaded && model.store.records.isEmpty {
                VStack(spacing: 10) {
                    ProgressView()
                    Text("Reading your Claude Code history…").font(.callout).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(.regularMaterial)
            }
        }
    }
}

struct FooterView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        HStack(spacing: 8) {
            TimelineView(.periodic(from: .now, by: 10)) { ctx in
                HStack(spacing: 5) {
                    Circle()
                        .fill(model.isScanning ? Theme.coral : Color.green.opacity(0.8))
                        .frame(width: 6, height: 6)
                    Text(statusText(now: ctx.date))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
            Button {
                Task {
                    await model.scan()
                    if model.settings.limitsEnabled { await model.refreshLimits() }
                    if model.settings.dataSource != .logs { await model.refreshAPI() }
                }
            } label: {
                Image(systemName: "arrow.clockwise")
                    .symbolEffect(.rotate, isActive: model.isScanning || model.isFetchingAPI)
            }
            .help("Refresh now")
            Button {
                NSApp.activate()
                openSettings()
            } label: { Image(systemName: "gearshape") }
            .help("Settings…")
            .keyboardShortcut(",", modifiers: .command)
            Button { NSApp.terminate(nil) } label: { Image(systemName: "power") }
                .help("Quit NACUT")
        }
        .buttonStyle(.borderless)
        .imageScale(.medium)
    }

    private func statusText(now: Date) -> String {
        var parts: [String] = []
        if let t = model.lastUpdated {
            let s = Int(now.timeIntervalSince(t))
            parts.append(s < 60 ? "Updated just now" : "Updated \(s / 60)m ago")
        } else {
            parts.append("Loading…")
        }
        switch model.settings.dataSource {
        case .logs: parts.append("local logs")
        case .api: parts.append("Anthropic API")
        case .hybrid: parts.append("API + logs")
        }
        return parts.joined(separator: " · ")
    }
}

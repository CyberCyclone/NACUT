import AppKit
import SwiftUI
import UsageCore

/// `NACUT --render-to <dir>` renders each panel tab to PNG (light and dark) and quits.
/// Used to check the UI without opening the menu bar panel.
@MainActor
enum DebugRenderer {
    static var outputDir: URL? {
        let args = CommandLine.arguments
        guard let i = args.firstIndex(of: "--render-to"), i + 1 < args.count else { return nil }
        return URL(filePath: args[i + 1], directoryHint: .isDirectory)
    }

    static func runIfRequested(model: AppModel) {
        guard let dir = outputDir else { return }
        Task { @MainActor in
            while !model.hasLoaded { try? await Task.sleep(for: .milliseconds(200)) }
            // Render with a saved limits response instead of reading the Keychain.
            if let i = CommandLine.arguments.firstIndex(of: "--limits-json"), i + 1 < CommandLine.arguments.count,
               let data = try? Data(contentsOf: URL(filePath: CommandLine.arguments[i + 1])) {
                model.debugSetLimits(PlanLimit.parse(data))
            }
            let limitsDeadline = Date.now.addingTimeInterval(15)
            while model.settings.limitsEnabled, model.limits.isEmpty, model.limitsError == nil, Date.now < limitsDeadline {
                try? await Task.sleep(for: .milliseconds(200))
            }
            try? await Task.sleep(for: .milliseconds(500))
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            for scheme in [ColorScheme.light, .dark] {
                for tab in PanelTab.allCases {
                    let view = VStack(spacing: 12) {
                        HeaderView()
                        Picker("", selection: .constant(tab)) {
                            ForEach(PanelTab.allCases) { Text($0.rawValue).tag($0) }
                        }
                        .pickerStyle(.segmented).labelsHidden()
                        Group {
                            switch tab {
                            case .overview: OverviewTab()
                            case .calendar: CalendarTabStatic()
                            case .models: ModelsTab()
                            case .projects: ProjectsTab()
                            }
                        }
                        Divider()
                        FooterView()
                    }
                    .padding(16)
                    .frame(width: 452)
                    .background(scheme == .dark ? Color(white: 0.13) : Color(white: 0.97))
                    .environment(model)
                    .environment(\.colorScheme, scheme)

                    let r = ImageRenderer(content: view)
                    r.scale = 2
                    r.proposedSize = .init(width: 452, height: nil)
                    if let img = r.nsImage, let tiff = img.tiffRepresentation,
                       let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) {
                        try? png.write(to: dir.appending(path: "\(tab.rawValue)-\(scheme == .dark ? "dark" : "light").png"))
                    }
                }
            }
            NSApp.terminate(nil)
        }
    }
}

/// Calendar without the paging scroll view (ImageRenderer can't draw scroll views).
private struct CalendarTabStatic: View {
    @Environment(AppModel.self) private var model
    @State private var selected: Date? = Calendar.current.startOfDay(for: .now)

    var body: some View {
        let month = Calendar.current.dateInterval(of: .month, for: .now)!.start
        let costs = model.series.values.map(\.cost).filter { $0 > 0 }.sorted()
        let scale = costs.isEmpty ? 1 : costs[min(Int(Double(costs.count) * 0.95), costs.count - 1)]
        VStack(spacing: 12) {
            Text(month.formatted(.dateTime.month(.wide).year())).font(.title3.weight(.semibold))
                .frame(maxWidth: .infinity, alignment: .leading)
            MonthGrid(month: month, scaleMax: scale, selected: $selected)
            MonthSummary(month: month)
            if let selected { DayDetailView(day: selected) {} }
        }
    }
}

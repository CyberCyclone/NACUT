import AppKit
import SwiftUI
import NacutCore
import NacutKit

/// `NACUT --render-to <dir>` renders each panel tab to PNG (light and dark) and quits.
/// Used to check the UI without opening the menu bar panel.
@MainActor
enum DebugRenderer {
    static let options = RenderOptions.parse(arguments: CommandLine.arguments)
    static var outputDir: URL? { options?.outputDir }

    static func runIfRequested(model: AppModel) {
        guard let options else { return }
        let dir = options.outputDir
        Task { @MainActor in
            while !model.hasLoaded { try? await Task.sleep(for: .milliseconds(200)) }
            // Render with a saved limits response instead of reading the Keychain.
            if let url = options.limitsJSON, let json = try? String(contentsOf: url, encoding: .utf8) {
                model.debugSetLimits(json: json)
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
        let scale = CalendarMath.scaleMax(model.series.values.map(\.cost))
        VStack(spacing: 12) {
            Text(month.formatted(.dateTime.month(.wide).year())).font(.title3.weight(.semibold))
                .frame(maxWidth: .infinity, alignment: .leading)
            MonthGrid(month: month, scaleMax: scale, selected: $selected)
            MonthSummary(month: month)
            if let selected { DayDetailView(day: selected) {} }
        }
    }
}

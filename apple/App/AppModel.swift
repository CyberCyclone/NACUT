import AppKit
import Foundation
import NacutCore
import Observation
import ServiceManagement
import WidgetKit

/// The Mac app's view of the shared Rust core (`NacutEngine`): republishes its snapshots on the main
/// actor for SwiftUI, forwards settings, posts alerts, and writes the widget snapshot.
/// Scanning, watching, plan limits, the API, pricing and forecasts all live in the core.
@MainActor
@Observable
final class AppModel {
    let settings = AppSettings()
    @ObservationIgnored let engine: NacutEngine
    @ObservationIgnored private var observer: EngineBridge?
    @ObservationIgnored private let notifier = Notifier()
    @ObservationIgnored private var lastSnapshotWrite = Date.distantPast
    @ObservationIgnored private var reloadScheduled = false

    /// The latest core snapshot. Views read it through the conveniences below.
    private(set) var snap: Snapshot
    /// Bumped on every reload so range queries (which call the engine directly) re-run.
    private(set) var revision = 0

    nonisolated static let supportDir: URL = {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appending(path: "NACUT", directoryHint: .isDirectory)
    }()

    init() {
        engine = NacutEngine(dataDir: Self.supportDir.path, settingsPath: nil, settings: settings.core)
        snap = engine.snapshot()
        let bridge = EngineBridge(model: self)
        observer = bridge
        engine.setObserver(observer: bridge)
        settings.onChange = { [weak self] in self?.pushSettings() }
        if DebugRenderer.outputDir != nil {
            // Screenshots: read the logs once; no background work, network or Keychain access.
            engine.scanNow()
            snap = engine.snapshot()
            return
        }
        engine.start()
        if settings.alertsEnabled { Task { await notifier.requestAuthorization() } }
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [engine] _ in engine.stop() }
    }

    // MARK: Engine plumbing

    /// Coalesces bursts of change notifications into one snapshot read.
    func scheduleReload() {
        guard !reloadScheduled else { return }
        reloadScheduled = true
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(50))
            reloadScheduled = false
            reload()
        }
    }

    func reload() {
        snap = engine.snapshot()
        revision &+= 1
        settings.absorb(snap.settings)
        writeSnapshot()
    }

    private func pushSettings() {
        engine.setSettings(settings: settings.core)
        reload()
    }

    fileprivate func post(_ alert: Alert) {
        notifier.post(alert)
    }

    // MARK: State for views

    var status: StatusView { snap.status }
    var forecast: ForecastView { snap.forecast }
    var insights: InsightsView { snap.insights }
    var api: ApiView { snap.api }
    var reconciliation: [ReconciliationView] { snap.reconciliation }

    var hasLoaded: Bool { snap.status.hasLoaded }
    var isScanning: Bool { snap.status.isScanning }
    var isFetchingAPI: Bool { snap.api.isFetching }
    var lastUpdated: Date? { snap.status.lastUpdatedMs.map(Date.init(ms:)) }
    var recordCount: Int { Int(snap.recordCount) }
    var firstRecordDate: Date? { snap.firstRecordMs.map(Date.init(ms:)) }

    var limits: [LimitView] { snap.status.limits }
    var limitsError: String? { snap.status.limitsError }
    var limitsVerdict: Verdict? { snap.status.limitsVerdict }
    var showsLimits: Bool { snap.status.showsLimits }
    var menuBarText: String { snap.status.menuBarText }

    var today: Double { snap.forecast.today }
    var cycleSpent: Double { snap.forecast.spent }
    var allTime: Double { snap.insights.allTime }
    var isSubscription: Bool { settings.accountMode == .subscription }

    /// Daily series keyed by local midnight.
    var series: [Date: DayEntry] {
        Dictionary(snap.series.map { ($0.day, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// In hybrid/API mode, data newer than this is a live estimate.
    var billedThrough: Date? { snap.api.billedThroughMs.map(Date.init(ms:)) }
    var calibrationRatio: Double { snap.api.calibration.ratio }
    var hasAPIKey: Bool { snap.api.hasKey }
    var apiStatusMessage: String { snap.api.message }
    var apiUsable: Bool { snap.api.usable }
    var apiActors: [ApiActor] { snap.api.actors }

    // MARK: Queries (re-run when `revision` changes)

    func modelStats(_ r: StatRange) -> [ModelView] { _ = revision; return engine.modelStats(range: r) }
    func projectStats(_ r: StatRange) -> [ProjectView] { _ = revision; return engine.projectStats(range: r) }
    func sessions(_ r: StatRange, limit: Int) -> [SessionView] { _ = revision; return engine.sessions(range: r, limit: UInt32(limit)) }
    func entrypointStats(_ r: StatRange) -> [NamedValue] { _ = revision; return engine.entrypointStats(range: r) }
    func agentSplit(_ r: StatRange) -> AgentSplit { _ = revision; return engine.agentSplit(range: r) }
    func tokenClasses(_ r: StatRange) -> [NamedValue] { _ = revision; return engine.tokenClasses(range: r) }
    func cacheSavings(_ r: StatRange) -> Double { _ = revision; return engine.cacheSavings(range: r) }
    func heatmap(_ r: StatRange) -> [[Double]] { _ = revision; return engine.weekdayHourHeatmap(range: r) }
    func hourly(_ day: Date) -> [HourCost] { _ = revision; return engine.hourly(day: DayKey(day)) }
    func dayModels(_ day: Date) -> [ModelView] { _ = revision; return engine.dayModels(day: DayKey(day)) }
    func daySessions(_ day: Date, limit: Int) -> [SessionView] { _ = revision; return engine.daySessions(day: DayKey(day), limit: UInt32(limit)) }
    func lastDays(_ count: Int) -> [DayEntry] { _ = revision; return engine.lastDays(count: UInt32(count)) }
    func displayName(_ model: String) -> String { engine.displayName(model: model) }
    var logRoots: [String] { _ = revision; return engine.logRoots() }

    // MARK: Actions

    func refreshAll() {
        engine.refresh()
        if settings.limitsEnabled { engine.refreshLimits() }
        if settings.dataSource != .logs { engine.refreshApi(full: false) }
    }

    func rebuildCache() { engine.rebuildCache() }

    func refreshLimits() { engine.refreshLimits() }

    func refreshAPI(full: Bool = false) { engine.refreshApi(full: full) }

    func setLimitsEnabled(_ on: Bool) { settings.limitsEnabled = on }

    func setUsageDisplay(_ display: UsageDisplay) {
        if display == .limits && !settings.limitsEnabled { settings.limitsEnabled = true }
        settings.usageDisplay = display
    }

    /// Menu bar text the current settings would produce (Settings preview).
    var menuBarPreview: String { _ = revision; return engine.previewMenuBarText(settings: settings.core) }

    /// Tests the key and stores it if usable (network; runs off the main actor).
    func saveAPIKey(_ key: String) async -> ApiResult {
        let engine = self.engine
        let result = await Task.detached { engine.saveApiKey(key: key) }.value
        reload()
        return result
    }

    func removeAPIKey() {
        engine.removeApiKey()
        reload()
    }

    func testAPI() async {
        let engine = self.engine
        _ = await Task.detached { engine.testApi() }.value
        reload()
    }

    func loadActors() async {
        let engine = self.engine
        _ = await Task.detached { engine.loadActors() }.value
        reload()
    }

    func setActor(_ actor: ApiActor?) {
        engine.setActor(actor: actor)
        reload()
    }

    func settingsChanged() { pushSettings() }

    func sendTestAlert() {
        Task {
            await notifier.requestAuthorization()
            engine.sendTestAlert()
        }
    }

    func debugSetLimits(json: String) { _ = engine.debugSetLimitsJson(json: json); reload() }

    var launchAtLogin: Bool {
        get { SMAppService.mainApp.status == .enabled }
        set {
            do {
                if newValue { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            } catch {
                NSLog("Launch at login failed: \(error)")
            }
        }
    }

    // MARK: Widget snapshot

    private func writeSnapshot(force: Bool = false) {
        guard force || Date.now.timeIntervalSince(lastSnapshotWrite) > 60 else { return }
        lastSnapshotWrite = .now
        let cal = Calendar.current
        let now = Date.now
        var monthDays: [Int: Double] = [:]
        for d in snap.series where Int(d.date.year) == cal.component(.year, from: now) && Int(d.date.month) == cal.component(.month, from: now) {
            monthDays[Int(d.date.day)] = d.cost
        }
        let models = engine.modelStats(range: .cycle).prefix(3).map { WidgetSnapshot.ModelSlice(name: $0.name, cost: $0.cost) }
        let f = snap.forecast
        let snapshot = WidgetSnapshot(
            generatedAt: now, isSubscription: isSubscription, today: f.today, cycleSpent: f.spent,
            cycleStart: f.cycle.start, cycleEnd: f.cycle.end, budget: f.budget, projected: f.projected,
            status: SnapshotStatus(f.status), headline: f.headline,
            last14: engine.lastDays(count: 14).map(\.cost), monthDays: monthDays,
            monthTitle: now.formatted(.dateTime.month(.wide)), topModels: Array(models),
            limits: limits.map { .init(label: $0.shortLabel, utilization: $0.utilization, resetsAt: $0.resetsAt) },
            showLimits: showsLimits)
        do {
            try snapshot.write()
            WidgetCenter.shared.reloadAllTimelines()
        } catch {
            NSLog("Widget snapshot write failed: \(error)")
        }
    }
}

extension SnapshotStatus {
    init(_ s: BudgetStatus) {
        switch s {
        case .noBudget: self = .noBudget
        case .underBudget: self = .underBudget
        case .tight: self = .tight
        case .willExceed: self = .willExceed
        case .exceeded: self = .exceeded
        }
    }
}

/// Receives core callbacks on background threads and hops to the main actor.
private final class EngineBridge: NacutObserver, @unchecked Sendable {
    private weak var model: AppModel?

    @MainActor init(model: AppModel) { self.model = model }

    func onChange() {
        Task { @MainActor [weak self] in self?.model?.scheduleReload() }
    }

    func onAlert(alert: Alert) {
        Task { @MainActor [weak self] in self?.model?.post(alert) }
    }
}

import AppKit
import Foundation
import Observation
import ServiceManagement
import UsageCore
import WidgetKit

/// Owns the data pipeline: scanning logs, the optional API and limits sources,
/// derived analytics, alerts, and the widget snapshot.
@MainActor
@Observable
final class AppModel {
    let settings = AppSettings()

    // Derived state the UI reads.
    private(set) var store: UsageStore = .empty
    private(set) var series: DailySeries = [:]
    private(set) var forecast: Forecast
    private(set) var insights: Insights
    private(set) var calibration: Calibration = .identity
    private(set) var reconciliation: [ReconciliationDay] = []

    private(set) var isScanning = false
    private(set) var hasLoaded = false
    private(set) var lastUpdated: Date?

    private(set) var limits: [PlanLimit] = []
    private(set) var limitsError: String?
    private(set) var limitsUpdated: Date?

    private(set) var apiStatus: ConnectionStatus = .notConfigured
    private(set) var apiData: APICostData?
    private(set) var apiActors: [APIActor] = []
    private(set) var isFetchingAPI = false

    private var cache = LogCache()
    private var cacheDirty = false
    private var watcher: FileWatcher?
    private var timers: [Timer] = []
    private var pendingScan = false
    private var lastSnapshotWrite = Date.distantPast
    private var lastAPIFetch = Date.distantPast
    private let alerts = AlertService()
    private var started = false

    nonisolated static let supportDir: URL = {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appending(path: "NACUT", directoryHint: .isDirectory)
    }()
    nonisolated private static let cacheURL = supportDir.appending(path: "log-cache.json")
    nonisolated private static let apiCacheURL = supportDir.appending(path: "api-cache.json")

    init() {
        let now = Date.now
        forecast = Forecast(series: [:], budget: 0, cycleStartDay: 1, now: now)
        insights = Insights(series: [:], cycle: BillingCycle.containing(now, startDay: 1), now: now)
        start()
    }

    // MARK: Lifecycle

    func start() {
        guard !started else { return }
        started = true
        Task {
            let (loaded, api) = await Task.detached(priority: .userInitiated) {
                (LogCache.load(from: Self.cacheURL), Self.loadAPICache())
            }.value
            cache = loaded
            apiData = api
            if !cache.records.isEmpty { rebuild() }
            await scan()
            hasLoaded = true
            startWatching()
            if settings.limitsEnabled { await refreshLimits() }
            if settings.dataSource != .logs { await refreshAPI() }
            if settings.alertsEnabled { await alerts.requestAuthorization() }
        }

        // Fallback rescan, minute tick for "today", periodic limits/API refresh.
        timers.append(Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in await self?.scan() }
        })
        timers.append(Timer.scheduledTimer(withTimeInterval: 300, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                if self.settings.limitsEnabled { await self.refreshLimits() }
                self.saveCacheIfNeeded()
            }
        })
        timers.append(Timer.scheduledTimer(withTimeInterval: 900, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, self.settings.dataSource != .logs else { return }
                await self.refreshAPI()
            }
        })
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.saveCacheIfNeeded(force: true) }
        }
    }

    private func startWatching() {
        watcher?.stop()
        watcher = FileWatcher(paths: settings.logRoots) { [weak self] in
            Task { await self?.scan() }
        }
        watcher?.start()
    }

    // MARK: Logs

    func scan() async {
        if isScanning { pendingScan = true; return }
        isScanning = true
        defer { isScanning = false }
        repeat {
            pendingScan = false
            let scanner = LogScanner(roots: settings.logRoots)
            let checkpoints = cache.checkpoints
            let results = await scanner.scan(checkpoints: checkpoints)
            if !results.isEmpty {
                _ = cache.merge(results)
                cacheDirty = true  // checkpoints moved even if no new records
            }
            rebuild()
            lastUpdated = .now
        } while pendingScan
        saveCacheIfNeeded()
    }

    func rebuildCache() async {
        cache = LogCache()
        cacheDirty = true
        await scan()
        saveCacheIfNeeded(force: true)
    }

    private var lastSave = Date.distantPast
    private func saveCacheIfNeeded(force: Bool = false) {
        guard cacheDirty, force || Date.now.timeIntervalSince(lastSave) > 30 else { return }
        cacheDirty = false
        lastSave = .now
        let snapshot = cache
        Task.detached(priority: .background) {
            try? snapshot.save(to: Self.cacheURL)
        }
    }

    // MARK: Derived state

    func rebuild() {
        let now = Date.now
        let book = PriceBook.bundled.withOverrides(settings.priceOverrides)
        store = UsageStore(records: Array(cache.records.values), priceBook: book)
        let merger = SourceMerger()
        let store = self.store
        let merged = merger.merge(mode: settings.dataSource, logs: store.logSeries, api: apiData) { day, after in
            let end = Calendar.current.date(byAdding: .day, value: 1, to: day)!
            var d = DayCost()
            for r in store.records(in: DateInterval(start: after, end: max(after, end))) {
                d.cost += r.cost
                d.byModel[PriceBook.normalize(r.model), default: 0] += r.cost
                d.messages += 1
                d.tokens += r.totalTokens
            }
            return d
        }
        series = merged.series
        calibration = merged.calibration
        reconciliation = apiData.map { merger.reconciliation(logs: store.logSeries, api: $0) } ?? []
        forecast = Forecast(series: series, budget: settings.monthlyBudget, cycleStartDay: settings.cycleStartDay, now: now)
        insights = Insights(series: series, cycle: forecast.cycle, now: now)
        if hasLoaded { alerts.evaluate(forecast: forecast, limits: limits, settings: settings) }
        writeSnapshot()
    }

    // MARK: Limits

    func refreshLimits() async {
        do {
            limits = try await LimitsService.fetch()
            limitsError = nil
            limitsUpdated = .now
            alerts.evaluate(forecast: forecast, limits: limits, settings: settings)
            writeSnapshot(force: true)
        } catch {
            limitsError = error.localizedDescription
        }
    }

    func setLimitsEnabled(_ on: Bool) {
        settings.limitsEnabled = on
        if on { Task { await refreshLimits() } } else { limits = []; limitsError = nil }
        writeSnapshot(force: true)
    }

    func debugSetLimits(_ l: [PlanLimit]) { limits = l; limitsError = nil }

    func setUsageDisplay(_ display: UsageDisplay) {
        settings.usageDisplay = display
        if display == .limits && !settings.limitsEnabled { setLimitsEnabled(true) }
        writeSnapshot(force: true)
    }

    /// Subscriptions lead with plan limits rather than API-equivalent cost, when the user prefers it.
    var showsLimits: Bool {
        isSubscription && settings.usageDisplay == .limits && settings.limitsEnabled
    }

    var fiveHourLimit: PlanLimit? { limits.first { $0.id == "five_hour" } }
    var weeklyLimit: PlanLimit? { limits.first { $0.id == "seven_day" } }

    /// The menu bar text, shared with the Settings preview.
    var menuBarText: String {
        let s = settings
        if showsLimits, let five = fiveHourLimit ?? limits.first {
            let fiveText = "\(five.shortLabel) \(five.effectiveUtilization().percent)"
            guard !s.compactMenuBar, let week = weeklyLimit, week.id != five.id else { return fiveText }
            return "\(fiveText) · \(week.shortLabel) \(week.effectiveUtilization().percent)"
        }
        if s.compactMenuBar || s.secondFigure == .none { return today.usd }
        let second = s.secondFigure == .allTime ? allTime : cycleSpent
        return "\(today.usd) · \(second.usdCompact)"
    }

    // MARK: API

    var hasAPIKey: Bool { KeychainStore.analyticsKey != nil }

    func saveAPIKey(_ key: String) async -> ConnectionStatus {
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return .notConfigured }
        let status = await AnalyticsClient(key: trimmed).testConnection()
        if status.isUsable || KeychainStore.analyticsKey == nil {
            KeychainStore.save(trimmed, account: KeychainStore.analyticsAccount)
        }
        apiStatus = status
        if status.isUsable {
            if settings.dataSource == .logs { settings.dataSource = .hybrid }
            await loadActors()
            await refreshAPI(full: true)
        }
        return status
    }

    func removeAPIKey() {
        KeychainStore.delete(account: KeychainStore.analyticsAccount)
        apiStatus = .notConfigured
        apiData = nil
        apiActors = []
        settings.dataSource = .logs
        settings.apiActorId = nil
        settings.apiActorName = nil
        try? FileManager.default.removeItem(at: Self.apiCacheURL)
        rebuild()
    }

    func testAPI() async {
        guard let key = KeychainStore.analyticsKey else { apiStatus = .notConfigured; return }
        apiStatus = await AnalyticsClient(key: key).testConnection()
    }

    func loadActors() async {
        guard let key = KeychainStore.analyticsKey else { return }
        apiActors = (try? await AnalyticsClient(key: key).actors()) ?? []
    }

    func setActor(_ actor: APIActor?) {
        settings.apiActorId = actor?.id
        settings.apiActorName = actor?.display
        Task { await refreshAPI(full: true) }
    }

    /// Fetches billed cost. Incremental by default: refreshes the last 40 days, keeping older cached rows.
    func refreshAPI(full: Bool = false) async {
        guard let key = KeychainStore.analyticsKey, !isFetchingAPI else {
            if KeychainStore.analyticsKey == nil { apiStatus = .notConfigured }
            return
        }
        isFetchingAPI = true
        defer { isFetchingAPI = false }
        let now = Date.now
        let client = AnalyticsClient(key: key)
        let historyStart = min(store.firstDate ?? now, now.addingTimeInterval(-40 * 86_400))
        let actorChanged = apiData.map { _ in UserDefaults.standard.string(forKey: "apiCacheActor") != settings.apiActorId } ?? true
        let incremental = !full && !actorChanged && apiData != nil
        let from = incremental ? now.addingTimeInterval(-40 * 86_400) : historyStart
        do {
            let fresh = try await client.hourlyCost(from: from, to: now, actorId: settings.apiActorId)
            var rows = fresh.rows
            var coverageStart = fresh.coverageStart
            if incremental, let old = apiData {
                rows = old.rows.filter { $0.start < from } + rows
                coverageStart = min(old.coverageStart, from)
            }
            apiData = APICostData(rows: rows, refreshedAt: fresh.refreshedAt ?? apiData?.refreshedAt,
                                  coverageStart: coverageStart, coverageEnd: now)
            UserDefaults.standard.set(settings.apiActorId, forKey: "apiCacheActor")
            apiStatus = fresh.refreshedAt == nil && rows.isEmpty ? .noDataYet : .connected(refreshedAt: apiData?.refreshedAt)
            lastAPIFetch = now
            if let data = apiData {
                Task.detached(priority: .background) { Self.saveAPICache(data) }
            }
            rebuild()
        } catch {
            apiStatus = AnalyticsClient.status(for: error)
        }
    }

    nonisolated private static func loadAPICache() -> APICostData? {
        guard let data = try? Data(contentsOf: apiCacheURL) else { return nil }
        let d = JSONDecoder()
        d.dateDecodingStrategy = .secondsSince1970
        return try? d.decode(APICostData.self, from: data)
    }

    nonisolated private static func saveAPICache(_ value: APICostData) {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .secondsSince1970
        try? FileManager.default.createDirectory(at: supportDir, withIntermediateDirectories: true)
        try? e.encode(value).write(to: apiCacheURL, options: .atomic)
    }

    // MARK: Settings side effects

    func settingsChanged() {
        rebuild()
        startWatching()
    }

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

    func sendTestAlert() {
        Task {
            await alerts.requestAuthorization()
            alerts.sendTest()
        }
    }

    // MARK: Convenience for views

    var today: Double { forecast.today }
    var cycleSpent: Double { forecast.spent }
    var allTime: Double { insights.allTime }
    var isSubscription: Bool { settings.accountMode == .subscription }

    var todayInterval: DateInterval {
        let cal = Calendar.current
        let s = cal.startOfDay(for: .now)
        return DateInterval(start: s, end: cal.date(byAdding: .day, value: 1, to: s)!)
    }

    var cycleInterval: DateInterval { forecast.cycle.interval }

    /// Data newer than this point is a live estimate (hybrid mode).
    var billedThrough: Date? {
        settings.dataSource == .logs ? nil : apiData?.refreshedAt
    }

    // MARK: Widget snapshot

    private func writeSnapshot(force: Bool = false) {
        guard force || Date.now.timeIntervalSince(lastSnapshotWrite) > 60 else { return }
        lastSnapshotWrite = .now
        let cal = Calendar.current
        let now = Date.now
        let month = cal.dateInterval(of: .month, for: now)!
        var monthDays: [Int: Double] = [:]
        for (d, v) in series where month.contains(d) { monthDays[cal.component(.day, from: d)] = v.cost }
        let models = store.modelStats(in: cycleInterval).prefix(3).map { WidgetSnapshot.ModelSlice(name: $0.name, cost: $0.cost) }
        let snap = WidgetSnapshot(
            generatedAt: now, isSubscription: isSubscription, today: today, cycleSpent: cycleSpent,
            cycleStart: forecast.cycle.start, cycleEnd: forecast.cycle.end, budget: forecast.budget,
            projected: forecast.projected, status: forecast.status, headline: forecast.headline,
            last14: series.days(endingAt: now, count: 14).map(\.cost.cost), monthDays: monthDays,
            monthTitle: now.formatted(.dateTime.month(.wide)), topModels: Array(models),
            limits: limits.map { .init(label: $0.shortLabel, utilization: $0.utilization, resetsAt: $0.resetsAt) },
            showLimits: showsLimits)
        do {
            try snap.write()
            WidgetCenter.shared.reloadAllTimelines()
        } catch {
            NSLog("Widget snapshot write failed: \(error)")
        }
    }
}

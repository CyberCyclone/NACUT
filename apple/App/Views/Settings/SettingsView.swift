import SwiftUI
import NacutCore

struct SettingsView: View {
    var body: some View {
        TabView {
            Tab("General", systemImage: "gearshape") { GeneralSettings() }
            Tab("Budget", systemImage: "dollarsign.circle") { BudgetSettings() }
            Tab("Data Sources", systemImage: "externaldrive.connected.to.line.below") { DataSourceSettings() }
            Tab("Alerts", systemImage: "bell.badge") { AlertSettings() }
            Tab("Pricing", systemImage: "tag") { PricingSettings() }
        }
        .frame(width: 560, height: 520)
    }
}

// MARK: - General

struct GeneralSettings: View {
    @Environment(AppModel.self) private var model
    @State private var launch = false

    var body: some View {
        @Bindable var s = model.settings
        Form {
            if model.isSubscription {
                Section {
                    Picker("Show usage as", selection: Binding(
                        get: { s.usageDisplay }, set: { model.setUsageDisplay($0) })) {
                        ForEach(UsageDisplay.allCases) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    Text(s.usageDisplay == .limits
                         ? "The menu bar, panel header and widget show your 5-hour and weekly plan limits. What the same usage would cost at API prices is still in the panel."
                         : "The menu bar, panel header and widget show what your usage would cost at API prices. On a subscription you don't pay this; it shows how much value you get from your plan.")
                        .font(.caption).foregroundStyle(.secondary)
                } footer: {
                    if s.usageDisplay == .limits, let err = model.limitsError, model.limits.isEmpty {
                        Label("Plan limits unavailable (\(err)), so the menu bar shows cost for now.", systemImage: "exclamationmark.triangle")
                            .font(.caption).foregroundStyle(.orange)
                    }
                }
            }
            Section("Menu bar") {
                Picker("Second figure", selection: $s.secondFigure) {
                    ForEach(SecondFigure.allCases) { Text($0.title).tag($0) }
                }
                .disabled(model.showsLimits)
                Toggle(model.showsLimits ? "Compact (5-hour limit only)" : "Compact (today only)", isOn: $s.compactMenuBar)
                LabeledContent("Preview") {
                    Text(model.menuBarText).monospacedDigit()
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(.fill.tertiary, in: RoundedRectangle(cornerRadius: 5))
                }
            }
            Section {
                Toggle("Open at login", isOn: Binding(get: { launch }, set: {
                    model.launchAtLogin = $0
                    launch = model.launchAtLogin
                }))
            }
            Section("About") {
                LabeledContent("Messages tracked", value: model.recordCount.formatted())
                LabeledContent("History since", value: model.firstRecordDate?.formatted(date: .abbreviated, time: .omitted) ?? "—")
                LabeledContent("Core", value: "nacut-core \(coreVersion())")
                Text("Costs are computed from Claude Code transcripts (desktop app Code tab, VS Code, and CLI) at Anthropic list prices. Desktop-app chats aren't in those logs; turn on plan limits or an API connection in Data Sources to cover them.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear { launch = model.launchAtLogin }
    }
}

// MARK: - Budget

struct BudgetSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var s = model.settings
        Form {
            Section {
                Picker("This Mac's account", selection: $s.accountMode) {
                    ForEach(AccountMode.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.radioGroup)
                Text(s.accountMode == .subscription
                     ? "Costs show what your usage would cost at API prices, so you can see the value you get from your plan."
                     : "Costs are what you pay per token. The budget forecast tells you if you'll go over.")
                    .font(.caption).foregroundStyle(.secondary)
                if s.accountMode == .subscription {
                    Picker("Plan", selection: $s.planPrice) {
                        Text("Pro · $20/mo").tag(20.0)
                        Text("Max 5x · $100/mo").tag(100.0)
                        Text("Max 20x · $200/mo").tag(200.0)
                        if ![20.0, 100.0, 200.0].contains(s.planPrice) {
                            Text("Custom · \(s.planPrice.usd)").tag(s.planPrice)
                        }
                    }
                }
            } header: { Text("Account") }

            Section {
                TextField("Monthly budget (USD)", value: $s.monthlyBudget,
                          format: .currency(code: "USD").precision(.fractionLength(0)))
                    .onSubmit { model.settingsChanged() }
                Text("Set to $0 for no budget. \(s.accountMode == .subscription ? "On a subscription this is a soft target for API-equivalent value." : "")")
                    .font(.caption).foregroundStyle(.secondary)
                Stepper("Cycle starts on day \(s.cycleStartDay)", value: $s.cycleStartDay, in: 1...28)
                Text("Use your billing or renewal day if it isn't the 1st. Current cycle: \(cycleText).")
                    .font(.caption).foregroundStyle(.secondary)
            } header: { Text("Budget") }

            Section("Forecast right now") {
                ForecastLine(forecast: model.forecast)
                LabeledContent("Spent this cycle", value: model.forecast.spent.usd)
                LabeledContent("Projected", value: model.forecast.projected.usd)
                if let safe = model.forecast.safeDailySpend {
                    LabeledContent("Safe daily spend", value: safe.usd)
                }
            }
        }
        .formStyle(.grouped)
    }

    private var cycleText: String {
        let c = model.forecast.cycle
        let last = Calendar.current.date(byAdding: .day, value: -1, to: c.end)!
        return "\(c.start.formatted(.dateTime.month().day())) – \(last.formatted(.dateTime.month().day()))"
    }
}

// MARK: - Data sources

struct DataSourceSettings: View {
    @Environment(AppModel.self) private var model
    @State private var keyInput = ""
    @State private var testing = false

    private func connect() {
        let key = keyInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty, !testing else { return }
        Task {
            testing = true
            _ = await model.saveAPIKey(key)
            keyInput = ""
            testing = false
        }
    }
    @State private var newRoot = ""

    var body: some View {
        @Bindable var s = model.settings
        Form {
            Section {
                Toggle("Show Max/Pro plan limits", isOn: Binding(
                    get: { s.limitsEnabled }, set: { model.setLimitsEnabled($0) }))
                Text("Shows your 5-hour and weekly limit usage, including desktop-app chats. Uses the Claude Code sign-in from your Keychain (macOS asks once to allow access) and the same unofficial endpoint as Claude Code's /usage, so it may stop working if Anthropic changes it. The sign-in is sent only to api.anthropic.com.")
                    .font(.caption).foregroundStyle(.secondary)
                if s.limitsEnabled {
                    Toggle("Refresh sign-in automatically", isOn: $s.autoRefreshSignIn)
                    Text("When the Claude Code sign-in expires (about every 8 hours), run `claude` with a one-word Haiku prompt so it refreshes itself. It runs with no tools, MCP servers or project settings and isn't saved to your history. Off: run `claude` yourself when limits pause.")
                        .font(.caption).foregroundStyle(.secondary)
                    if let err = model.limitsError {
                        Label(err, systemImage: "exclamationmark.triangle").foregroundStyle(.orange).font(.callout)
                    } else if let at = model.status.limitsUpdatedMs.map(Date.init(ms:)) {
                        Label("Updated \(at.formatted(.relative(presentation: .named)))", systemImage: "checkmark.circle")
                            .foregroundStyle(.green).font(.callout)
                    }
                }
            } header: { Text("Subscription limits") }

            Section {
                if model.hasAPIKey {
                    HStack {
                        Label(model.apiStatusMessage, systemImage: model.api.state.symbol)
                            .foregroundStyle(model.apiUsable ? Color.green : Color.orange)
                            .font(.callout)
                        Spacer()
                        if model.isFetchingAPI { ProgressView().controlSize(.small) }
                    }
                    Picker("Use", selection: $s.dataSource) {
                        ForEach(DataSourceMode.allCases) { Text($0.title).tag($0) }
                    }
                    actorPicker
                    HStack {
                        Button("Test connection") { Task { testing = true; await model.testAPI(); testing = false } }
                        Button("Refresh now") { model.refreshAPI(full: true) }
                        Spacer()
                        Button("Remove key", role: .destructive) { model.removeAPIKey() }
                    }
                    .disabled(testing)
                } else {
                    SecureField("API key", text: $keyInput, prompt: Text("sk-ant-admin… or read:analytics key"))
                        .onSubmit { if !keyInput.isEmpty { connect() } }
                    HStack {
                        Button("Connect") { connect() }
                            .keyboardShortcut(.defaultAction)
                        .disabled(keyInput.isEmpty || testing)
                        if testing { ProgressView().controlSize(.small) }
                        Spacer()
                    }
                    if model.api.state != .notConfigured {
                        Label(model.apiStatusMessage, systemImage: model.api.state.symbol)
                            .foregroundStyle(.orange).font(.callout)
                    }
                    if keyInput.trimmingCharacters(in: .whitespaces).hasPrefix("sk-ant-admin") {
                        Label(Self.adminKeyWarning, systemImage: "exclamationmark.shield")
                            .foregroundStyle(.orange).font(.callout)
                    }
                }
                Text("Optional. Pulls exact billed cost from Anthropic's Usage & Cost API. Use a Claude Enterprise key scoped to read:analytics (created by your org's primary owner) if you can. Console orgs need an Admin API key, which can manage the whole organization, so only store one on a computer you trust. If your org doesn't allow it, the test tells you and the app keeps using local logs. The key is stored only in your Keychain. API data lags 4–24 h, so Hybrid fills recent hours from local logs, scaled to match your billed rate.")
                    .font(.caption).foregroundStyle(.secondary)
            } header: { Text("Anthropic Usage & Cost API") }

            Section {
                ForEach(defaultLogRoots(), id: \.self) { path in
                    HStack {
                        Image(systemName: FileManager.default.fileExists(atPath: path) ? "folder.fill" : "folder.badge.questionmark")
                            .foregroundStyle(.secondary)
                        Text(path).font(.callout).lineLimit(1).truncationMode(.middle)
                        Spacer()
                        Text("default").font(.caption).foregroundStyle(.tertiary)
                    }
                }
                ForEach(s.extraRoots, id: \.self) { path in
                    HStack {
                        Image(systemName: "folder.fill").foregroundStyle(Theme.coral)
                        Text(path).font(.callout).lineLimit(1).truncationMode(.middle)
                        Spacer()
                        Button {
                            s.extraRoots.removeAll { $0 == path }
                        } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                    }
                }
                HStack {
                    TextField("Add folder", text: $newRoot, prompt: Text("~/other-claude/projects"))
                    Button("Add") {
                        s.extraRoots.append(newRoot)
                        newRoot = ""
                    }
                    .disabled(newRoot.isEmpty)
                }
                HStack {
                    Text("\(model.recordCount.formatted()) messages cached").font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    Button("Rebuild cache") { model.rebuildCache() }
                }
            } header: { Text("Local transcript folders") } footer: {
                Text("History is cached, so totals survive Claude Code's automatic transcript cleanup.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .task { if model.hasAPIKey && model.apiActors.isEmpty { await model.loadActors() } }
    }

    static let adminKeyWarning = "This is an Admin key: it can manage your whole organization (API keys, members, workspaces), not just read usage. Prefer a key scoped to read:analytics, and only store an Admin key on a computer you trust."

    @ViewBuilder private var actorPicker: some View {
        let s = model.settings
        Picker("Count usage from", selection: Binding<String>(
            get: { s.apiActorId ?? "" },
            set: { id in model.setActor(model.apiActors.first { $0.id == id }) })) {
            Text("Whole organization").tag("")
            ForEach(model.apiActors) { a in
                Text("\(a.display) · \(a.cost.usdCompact) last 30d").tag(a.id)
            }
            if let id = s.apiActorId, !model.apiActors.contains(where: { $0.id == id }) {
                Text(s.apiActorName ?? id).tag(id)
            }
        }
        Text("On a shared work org, pick yourself so the budget tracks only your usage.")
            .font(.caption).foregroundStyle(.secondary)
    }
}

// MARK: - Alerts

struct AlertSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var s = model.settings
        Form {
            Section {
                Toggle("Budget notifications", isOn: $s.alertsEnabled)
                ForEach([50, 80, 90, 100], id: \.self) { t in
                    Toggle("At \(t)% of budget", isOn: Binding(
                        get: { s.alertThresholds.contains(t) },
                        set: { on in
                            if on { s.alertThresholds.append(t) } else { s.alertThresholds.removeAll { $0 == t } }
                        }))
                    .disabled(!s.alertsEnabled)
                }
                Toggle("When the forecast says I'll go over", isOn: $s.forecastAlert).disabled(!s.alertsEnabled)
                Toggle("When a plan limit passes 90%", isOn: $s.limitAlerts).disabled(!s.alertsEnabled)
            } footer: {
                Text("Each alert fires once per billing cycle (or limit window).").font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Button("Send test notification") { model.sendTestAlert() }
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - Pricing

struct PricingSettings: View {
    @Environment(AppModel.self) private var model
    @State private var editing: String?

    var body: some View {
        let table = priceTable()
        let info = priceTableInfo()
        Form {
            Section {
                Grid(alignment: .trailing, horizontalSpacing: 10, verticalSpacing: 6) {
                    GridRow {
                        Text("Model").gridColumnAlignment(.leading)
                        Text("Input"); Text("Output"); Text("5m write"); Text("1h write"); Text("Cache read")
                    }
                    .font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                    ForEach(table, id: \.id) { m in
                        let r = model.settings.priceOverrides[m.id] ?? m.rates
                        GridRow {
                            HStack(spacing: 4) {
                                Text(m.name)
                                if model.settings.priceOverrides[m.id] != nil {
                                    Image(systemName: "pencil.circle.fill").foregroundStyle(Theme.coral).help("Custom price")
                                }
                            }
                            .gridColumnAlignment(.leading)
                            Text(fmt(r.input)); Text(fmt(r.output)); Text(fmt(r.write5m)); Text(fmt(r.write1h)); Text(fmt(r.read))
                        }
                        .font(.callout.monospacedDigit())
                        .contentShape(Rectangle())
                        .onTapGesture(count: 2) { editing = m.id }
                    }
                }
            } header: {
                Text("USD per million tokens")
            } footer: {
                Text("List prices as of \(info.updated). Fast mode is ×\(String(format: "%.0f", info.fastMultiplier)); web search $\(String(format: "%.0f", info.webSearchPerThousand)) per 1,000. Haiku 5.5 uses higher rates above 100K prompt tokens. Double-click a row to set a custom price (e.g. a negotiated rate).")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if !model.settings.priceOverrides.isEmpty {
                Button("Reset all custom prices") {
                    model.settings.priceOverrides = [:]
                }
            }
        }
        .formStyle(.grouped)
        .sheet(item: Binding(get: { editing.map(IdentifiedString.init) }, set: { editing = $0?.id })) { item in
            PriceEditor(modelId: item.id)
        }
    }

    private func fmt(_ v: Double) -> String {
        v < 1 ? String(format: "$%.3g", v) : String(format: "$%.2f", v).replacingOccurrences(of: ".00", with: "")
    }
}

struct IdentifiedString: Identifiable { let id: String }

struct PriceEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let modelId: String
    @State private var rates = Rates(input: 0, output: 0, write5m: 0, write1h: 0, read: 0)

    var body: some View {
        let base = priceTable().first { $0.id == modelId }
        Form {
            Section(base?.name ?? modelId) {
                TextField("Input", value: $rates.input, format: .number)
                TextField("Output", value: $rates.output, format: .number)
                TextField("Cache write (5m)", value: $rates.write5m, format: .number)
                TextField("Cache write (1h)", value: $rates.write1h, format: .number)
                TextField("Cache read", value: $rates.read, format: .number)
            }
        }
        .formStyle(.grouped)
        .frame(width: 320)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .destructiveAction) {
                Button("Use list price") {
                    model.settings.priceOverrides[modelId] = nil
                    dismiss()
                }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    model.settings.priceOverrides[modelId] = rates
                    dismiss()
                }
            }
        }
        .onAppear { rates = model.settings.priceOverrides[modelId] ?? base?.rates ?? rates }
    }
}

extension ApiState {
    var symbol: String {
        switch self {
        case .notConfigured: return "circle.dashed"
        case .connected: return "checkmark.circle.fill"
        case .noDataYet: return "clock.badge.checkmark"
        case .unauthorized, .forbidden: return "lock.circle.fill"
        case .unavailable: return "nosign"
        case .failed: return "exclamationmark.triangle.fill"
        }
    }
}

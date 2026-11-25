import SwiftUI
import UsageCore

struct HeaderView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        // Fall back to the cost header if limits can't be loaded, so the panel is never empty.
        if model.showsLimits && !(model.limits.isEmpty && model.limitsError != nil) {
            LimitsHeader()
        } else {
            costHeader
        }
    }

    private var costHeader: some View {
        let f = model.forecast
        return VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center, spacing: 14) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(cycleLabel(f.cycle))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                        .textCase(.uppercase)
                    Text(f.spent.usd)
                        .font(.system(size: 34, weight: .semibold, design: .rounded))
                        .monospacedDigit()
                        .contentTransition(.numericText())
                        .help(CurrencyFormat.exact(f.spent))
                    HStack(spacing: 10) {
                        Label {
                            Text("\(Text("Today ").foregroundStyle(.secondary))\(Text(f.today.usd).fontWeight(.semibold))")
                        } icon: { Image(systemName: "sun.max.fill").foregroundStyle(Theme.coral) }
                        Text("·").foregroundStyle(.tertiary)
                        Text("\(Text("Avg ").foregroundStyle(.secondary))\(Text(f.dailyAverage.usd).fontWeight(.semibold))\(Text("/day").foregroundStyle(.secondary))")
                    }
                    .font(.callout)
                    .monospacedDigit()
                }
                Spacer(minLength: 0)
                if f.budget > 0 {
                    BudgetRing(spent: f.budgetUsedFraction, projected: f.projectedFraction, status: f.status) {
                        VStack(spacing: 0) {
                            Text(f.budgetUsedFraction.percent)
                                .font(.system(size: 17, weight: .semibold, design: .rounded))
                                .monospacedDigit()
                            Text("of \(f.budget.usdCompact)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(width: 84, height: 84)
                } else if model.isSubscription, model.settings.planPrice > 0 {
                    ValueBadge(multiple: f.spent / model.settings.planPrice, plan: model.settings.planPrice)
                } else {
                    Button("Set budget…") {
                        NSApp.activate()
                        openSettings()
                    }
                    .buttonStyle(.glass)
                }
            }

            ForecastLine(forecast: f)

            if model.settings.limitsEnabled {
                LimitsRow()
            }
        }
    }

    private func cycleLabel(_ c: BillingCycle) -> String {
        let fmt = Date.FormatStyle().month(.abbreviated).day()
        let last = Calendar.current.date(byAdding: .day, value: -1, to: c.end)!
        let noun = model.isSubscription ? "API value" : "Spend"
        return "\(noun) · \(c.start.formatted(fmt)) – \(last.formatted(fmt))"
    }
}

struct ForecastLine: View {
    let forecast: Forecast

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: Theme.statusSymbol(forecast.status))
                .foregroundStyle(Theme.statusColor(forecast.status))
            Text(forecast.headline)
                .font(.callout.weight(.medium))
            Spacer(minLength: 0)
            if let safe = forecast.safeDailySpend, forecast.status != .exceeded {
                Text("\(safe.usd)/day left")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                    .help("Spend at most this much per day for the rest of the cycle to stay within budget")
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(Theme.statusColor(forecast.status).opacity(0.10), in: Capsule())
        .accessibilityElement(children: .combine)
    }
}

struct ValueBadge: View {
    let multiple: Double
    let plan: Double

    var body: some View {
        VStack(spacing: 2) {
            Text(String(format: "%.1f×", multiple))
                .font(.system(size: 26, weight: .semibold, design: .rounded))
                .foregroundStyle(Theme.coral)
                .monospacedDigit()
            Text("your \(plan.usdCompact) plan")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(10)
        .background(Theme.coral.opacity(0.10), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .help("API-equivalent value of this cycle's usage compared with your subscription price")
    }
}

struct LimitsRow: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let err = model.limitsError, model.limits.isEmpty {
            Label(err, systemImage: "info.circle")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else if model.limits.isEmpty {
            HStack(spacing: 6) {
                ProgressView().controlSize(.small)
                Text("Loading plan limits…").font(.caption).foregroundStyle(.secondary)
            }
        } else {
            HStack(spacing: 8) {
                ForEach(model.limits.prefix(3)) { LimitGauge(limit: $0) }
            }
        }
    }
}

// MARK: - Plan-limits header (subscriptions)

/// Leads with the 5-hour and weekly limits; API-equivalent cost moves to a secondary line.
struct LimitsHeader: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let f = model.forecast
        VStack(alignment: .leading, spacing: 12) {
            Text("Plan usage · \(planName)")
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)

            if model.limits.isEmpty {
                LimitsRow()
            } else {
                HStack(spacing: 8) {
                    ForEach(model.limits.prefix(3)) { LimitRing(limit: $0) }
                }
                LimitsPaceLine(limits: model.limits)
            }

            HStack(spacing: 6) {
                Image(systemName: "dollarsign.circle").foregroundStyle(Theme.coral)
                Text("\(Text("API value ").foregroundStyle(.secondary))\(Text(f.spent.usd).fontWeight(.semibold))\(Text(" this cycle · ").foregroundStyle(.secondary))\(Text(f.today.usd).fontWeight(.semibold))\(Text(" today").foregroundStyle(.secondary))")
                Spacer(minLength: 4)
                if model.settings.planPrice > 0 {
                    Text(String(format: "%.1f× plan", f.spent / model.settings.planPrice))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Theme.coral)
                        .padding(.horizontal, 7).padding(.vertical, 3)
                        .background(Theme.coral.opacity(0.10), in: Capsule())
                        .help("API-equivalent value of this cycle's usage compared with your \(model.settings.planPrice.usdCompact) subscription")
                }
            }
            .font(.callout)
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.85)
        }
    }

    private var planName: String {
        switch model.settings.planPrice {
        case 200: return "Max 20x"
        case 100: return "Max 5x"
        case 20: return "Pro"
        default: return "Subscription"
        }
    }
}

struct LimitRing: View {
    let limit: PlanLimit

    var body: some View {
        let u = limit.effectiveUtilization()
        let p = limit.projectedAtReset()
        VStack(spacing: 6) {
            ZStack {
                Circle().stroke(.fill.tertiary, lineWidth: 7)
                if let p, p > u {
                    Circle().trim(from: 0, to: min(p, 1))
                        .stroke(Theme.limitColor(p).opacity(0.28), style: StrokeStyle(lineWidth: 7, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                }
                Circle().trim(from: 0, to: min(max(u, 0.004), 1))
                    .stroke(Theme.limitColor(u), style: StrokeStyle(lineWidth: 7, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Text(u.percent)
                    .font(.system(size: 17, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .contentTransition(.numericText())
            }
            .frame(width: 66, height: 66)
            .padding(.bottom, 2)
            Text(title).font(.caption.weight(.medium)).lineLimit(1)
            Text(subtitle).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 10)
        .padding(.horizontal, 4)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .help(helpText)
        .accessibilityElement(children: .combine)
    }

    private var title: String {
        switch limit.id {
        case "five_hour": return "5-hour session"
        case "seven_day": return "Weekly"
        default: return limit.label.replacingOccurrences(of: "Weekly · ", with: "") + " weekly"
        }
    }

    private var subtitle: String {
        if let hit = limit.projectedHit() {
            return "full ~\(hit.formatted(.dateTime.weekday(.abbreviated).hour()))"
        }
        return limit.resetText()
    }

    private var helpText: String {
        var s = "\(limit.label): \(limit.effectiveUtilization().percent) used. \(limit.resetText())."
        if let p = limit.projectedAtReset() { s += " On pace for \(p.percent) by reset (faint arc)." }
        return s
    }
}

/// One-line verdict: will any limit run out before it resets?
struct LimitsPaceLine: View {
    let limits: [PlanLimit]

    var body: some View {
        let (status, text) = verdict
        HStack(spacing: 6) {
            Image(systemName: Theme.statusSymbol(status)).foregroundStyle(Theme.statusColor(status))
            Text(text).font(.callout.weight(.medium)).lineLimit(1).minimumScaleFactor(0.85)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(Theme.statusColor(status).opacity(0.10), in: Capsule())
        .accessibilityElement(children: .combine)
    }

    private func name(_ l: PlanLimit) -> String {
        switch l.id {
        case "five_hour": return "5-hour limit"
        case "seven_day": return "weekly limit"
        default: return l.label.replacingOccurrences(of: "Weekly · ", with: "") + " weekly limit"
        }
    }

    private var verdict: (BudgetStatus, String) {
        if let full = limits.first(where: { $0.effectiveUtilization() >= 1 }) {
            return (.exceeded, "\(name(full).capitalizedFirst) reached · \(full.resetText())")
        }
        let hits = limits.compactMap { l in l.projectedHit().map { (l, $0) } }
        if let (l, date) = hits.min(by: { $0.1 < $1.1 }) {
            return (.willExceed, "At this pace the \(name(l)) runs out \(date.formatted(.dateTime.weekday(.abbreviated).hour().minute()))")
        }
        if let week = limits.first(where: { $0.id == "seven_day" }), let p = week.projectedAtReset() {
            return (p >= 0.8 ? .tight : .underBudget, "On pace for ~\(p.percent) of your weekly limit by reset")
        }
        return (.underBudget, "Plenty of room in your plan limits")
    }
}

private extension String {
    var capitalizedFirst: String { prefix(1).uppercased() + dropFirst() }
}

import SwiftUI
import NacutCore
import NacutKitCore

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
                    Text(f.cycleLabel)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                        .textCase(.uppercase)
                    Text(f.spent.usd)
                        .font(.system(size: 34, weight: .semibold, design: .rounded))
                        .monospacedDigit()
                        .contentTransition(.numericText())
                        .help(USD.exact(f.spent))
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
}

struct ForecastLine: View {
    let forecast: ForecastView

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
            Text("Plan usage · \(model.snap.planName)")
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)

            if model.limits.isEmpty {
                LimitsRow()
            } else {
                HStack(spacing: 8) {
                    ForEach(model.limits.prefix(3)) { LimitRing(limit: $0) }
                }
                if let v = model.limitsVerdict { LimitsPaceLine(verdict: v) }
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
}

struct LimitRing: View {
    let limit: LimitView

    var body: some View {
        let u = limit.utilization
        VStack(spacing: 6) {
            ZStack {
                Circle().stroke(.fill.tertiary, lineWidth: 7)
                if let p = limit.projectedAtReset, p > u {
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
            Text(limit.title).font(.caption.weight(.medium)).lineLimit(1)
            Text(limit.subtitle).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 10)
        .padding(.horizontal, 4)
        .background(.fill.quinary, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .help(limit.helpText() + (limit.projectedAtReset != nil ? " The faint arc is the pace." : ""))
        .accessibilityElement(children: .combine)
    }
}

/// One-line verdict from the core: will any limit run out before it resets?
struct LimitsPaceLine: View {
    let verdict: Verdict

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: Theme.statusSymbol(verdict.status)).foregroundStyle(Theme.statusColor(verdict.status))
            Text(verdict.text).font(.callout.weight(.medium)).lineLimit(1).minimumScaleFactor(0.85)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(Theme.statusColor(verdict.status).opacity(0.10), in: Capsule())
        .accessibilityElement(children: .combine)
    }
}

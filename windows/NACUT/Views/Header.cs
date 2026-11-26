using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using uniffi.nacut;

namespace Nacut.Views;

/// <summary>Panel header: plan limits on a subscription, cost and budget otherwise (HeaderView.swift).</summary>
static class Header
{
    public static UIElement Build(AppModel m)
    {
        // Fall back to the cost header if limits can't be loaded, so the panel is never empty.
        var limitsFailed = m.Limits.Count == 0 && m.Status.limitsError is not null;
        return m.ShowsLimits && !limitsFailed ? LimitsHeader(m) : CostHeader(m);
    }

    static UIElement CostHeader(AppModel m)
    {
        var f = m.Forecast;
        var left = Ui.V(4,
            Ui.Text(f.cycleLabel.ToUpperInvariant(), 11, FontWeights.SemiBold, Theme.TextSecondary),
            Ui.Figure(f.spent.Usd(), 34).Tip(f.spent.UsdExact()),
            Ui.H(10,
                Ui.H(4, Ui.Icon("", 13, Theme.Coral), Ui.Runs(13, ("Today ", false), (f.today.Usd(), true))),
                Ui.Text("·", 13, color: Theme.TextTertiary),
                Ui.Runs(13, ("Avg ", false), (f.dailyAverage.Usd(), true), ("/day", false))));

        UIElement right;
        if (f.budget > 0)
        {
            var center = Ui.V(0,
                Ui.Figure(f.budgetUsedFraction.Pct(), 17),
                Ui.Secondary($"of {f.budget.UsdCompact()}", 10));
            foreach (var c in ((StackPanel)center).Children) ((FrameworkElement)c).HorizontalAlignment = HorizontalAlignment.Center;
            var color = f.budgetUsedFraction >= 1 ? Theme.Status(BudgetStatus.Exceeded) : Theme.Coral;
            right = Charts.Ring(f.budgetUsedFraction, f.projectedFraction, color, Theme.Status(f.status), 84, 9, center)
                .Tip($"Budget {f.budgetUsedFraction.Pct()} used, projected {f.projectedFraction.Pct()}");
        }
        else if (m.IsSubscription && m.Settings.planPrice > 0)
        {
            right = ValueBadge(f.spent / m.Settings.planPrice, m.Settings.planPrice);
        }
        else
        {
            var b = new Button { Content = "Set budget…" };
            b.Click += (_, _) => App.Current.ShowSettings("budget");
            right = b;
        }

        return Ui.V(12,
            Ui.Spread(left, right, 14),
            ForecastLine(f),
            m.Settings.limitsEnabled ? LimitsRow(m) : null);
    }

    public static UIElement ForecastLine(ForecastView f)
    {
        var color = Theme.Status(f.status);
        UIElement? safe = f.safeDailySpend is { } s && f.status != BudgetStatus.Exceeded
            ? Ui.Secondary($"{s.Usd()}/day left").Tip("Spend at most this much per day for the rest of the cycle to stay within budget")
            : null;
        return Ui.Pill(Ui.Spread(Ui.H(6, Ui.Icon(Theme.StatusGlyph(f.status), 13, color), Ui.Text(f.headline, 13, FontWeights.Medium)), safe), color);
    }

    static UIElement ValueBadge(double multiple, double plan)
    {
        var stack = Ui.V(2,
            Ui.Figure($"{multiple:0.0}×", 26, Theme.Coral),
            Ui.Secondary($"your {plan.UsdCompact()} plan", 10));
        foreach (var c in stack.Children) ((FrameworkElement)c).HorizontalAlignment = HorizontalAlignment.Center;
        return Ui.Box(stack, 12, 10, Theme.CoralSoft, stroke: false)
            .Tip("API-equivalent value of this cycle's usage compared with your subscription price");
    }

    public static UIElement LimitsRow(AppModel m)
    {
        var st = m.Status;
        if (st.limitsError is { } err && m.Limits.Count == 0)
        {
            // A Grid rather than a horizontal stack, so the message can wrap.
            var g = new Grid { ColumnSpacing = 6 };
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            var icon = Ui.Icon("", 12);
            icon.VerticalAlignment = VerticalAlignment.Top;
            icon.Margin = new Thickness(0, 2, 0, 0);
            var text = Ui.Secondary(err, 12, wrap: true);
            Grid.SetColumn(text, 1);
            g.Children.Add(icon);
            g.Children.Add(text);
            return g;
        }
        if (m.Limits.Count == 0)
            return Ui.H(6, new ProgressRing { Width = 14, Height = 14, IsActive = true }, Ui.Secondary("Loading plan limits…"));
        return Ui.Columns(Math.Min(m.Limits.Count, 3), 8, m.Limits.Take(3).Select(LimitGauge));
    }

    static UIElement LimitGauge(LimitView l)
    {
        var stack = Ui.V(5,
            Ui.Spread(Ui.Text(Logic.Formats.LimitLabel(l.id, l.label), 12, FontWeights.Medium), Ui.Text(l.utilization.Pct(), 12, FontWeights.SemiBold)),
            Charts.Bar(l.utilization, Theme.ForLimit(l.utilization), 6, l.projectedAtReset, l.projectedAtReset is { } p ? Theme.ForLimit(p) : null),
            Ui.Secondary(l.subtitle, 11));
        return Ui.Box(stack, 10, 8).Tip(HelpText(l));
    }

    public static string HelpText(LimitView l)
    {
        var s = $"{l.label}: {l.utilization.Pct()} used. {l.resetText}.";
        if (l.projectedAtReset is { } p) s += $" On pace for {p.Pct()} by reset.";
        return s;
    }

    // MARK: Plan-limits header (subscriptions)

    static UIElement LimitsHeader(AppModel m)
    {
        var f = m.Forecast;
        var stack = Ui.V(12, Ui.Text($"PLAN USAGE · {m.Snap.planName.ToUpperInvariant()}", 11, FontWeights.SemiBold, Theme.TextSecondary));
        if (m.Limits.Count == 0)
        {
            stack.Children.Add(LimitsRow(m));
        }
        else
        {
            stack.Children.Add(Ui.Columns(Math.Min(m.Limits.Count, 3), 8, m.Limits.Take(3).Select(LimitRing)));
            if (m.Status.limitsVerdict is { } v)
            {
                var c = Theme.Status(v.status);
                stack.Children.Add(Ui.Pill(Ui.H(6, Ui.Icon(Theme.StatusGlyph(v.status), 13, c), Ui.Text(v.text, 13, FontWeights.Medium)), c));
            }
        }

        UIElement? multiple = null;
        if (m.Settings.planPrice > 0)
        {
            multiple = Ui.Pill(Ui.Text($"{f.spent / m.Settings.planPrice:0.0}× plan", 11, FontWeights.SemiBold, Theme.Coral), Theme.Coral, 7, 3)
                .Tip($"API-equivalent value of this cycle's usage compared with your {m.Settings.planPrice.UsdCompact()} subscription");
        }
        stack.Children.Add(Ui.Spread(
            Ui.H(6, Ui.Icon("", 13, Theme.Coral),
                Ui.Runs(13, ("API value ", false), (f.spent.Usd(), true), (" this cycle · ", false), (f.today.Usd(), true), (" today", false))),
            multiple));
        return stack;
    }

    static UIElement LimitRing(LimitView l)
    {
        var u = l.utilization;
        var ring = Charts.Ring(u, l.projectedAtReset, Theme.ForLimit(u), l.projectedAtReset is { } p ? Theme.ForLimit(p) : null, 66, 7,
            Ui.Figure(u.Pct(), 17));
        ring.Margin = new Thickness(0, 0, 0, 2);
        var stack = Ui.V(4, ring, Ui.Text(l.title, 12, FontWeights.Medium), Ui.Secondary(l.subtitle, 11));
        foreach (var c in stack.Children) ((FrameworkElement)c).HorizontalAlignment = HorizontalAlignment.Center;
        var box = Ui.Box(stack, 12, 10, Theme.SubtleFill, stroke: false);
        box.Padding = new Thickness(4, 10, 4, 10);
        return box.Tip(HelpText(l) + (l.projectedAtReset is not null ? " The faint arc is the pace." : ""));
    }
}

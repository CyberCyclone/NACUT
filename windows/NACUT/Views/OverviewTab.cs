using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using uniffi.nacut;
using Windows.UI;

namespace Nacut.Views;

/// <summary>Stat tiles, cumulative spend, the last 30 days by model, caching and insights (OverviewTab.swift).</summary>
static class OverviewTab
{
    public static UIElement Build(AppModel m, double width)
    {
        var f = m.Forecast;
        var i = m.Insights;
        var lastDay = f.cycleEndMs.Local().AddDays(-1);
        var today = m.Series.TryGetValue(CoreExtensions.Today, out var t) ? t.messages : 0UL;

        var tiles = Ui.Columns(3, 8,
        [
            Ui.StatTile("Today", f.today.Usd(), today > 0 ? $"{today:N0} messages" : "No usage yet", ""),
            Ui.StatTile("Daily average", f.dailyAverage.Usd(), $"7-day {f.trailing7Average.Usd()}", "",
                tip: "Cycle-to-date spend divided by days elapsed. The 7-day figure is the recent run rate."),
            Ui.StatTile("Projected", f.projected.Usd(), $"by {lastDay:MMM d}", "", Theme.Status(f.status),
                "Spend so far plus the blended daily rate × days remaining"),
            Ui.StatTile("Last 7 days", i.last7Days.Usd(), $"30 days {i.last30Days.UsdCompact()}", ""),
            Ui.StatTile("vs last cycle", VsText(i.vsPreviousCycle), $"Last cycle {i.previousCycleTotal.UsdCompact()}",
                (i.vsPreviousCycle ?? 0) >= 0 ? "" : "",
                tip: "This cycle so far compared with the same point in the previous cycle"),
            Ui.StatTile("All time", i.allTime.UsdCompact(),
                m.Snap.firstRecordMs is { } first ? $"since {first.Local():MMM yyyy}" : null, ""),
        ]);

        UIElement? billed = null;
        if (m.Api.billedThroughMs is { } through)
        {
            var ratio = m.Api.calibration.ratio;
            billed = Ui.H(6, Ui.Icon("", 12),
                Ui.Secondary($"Billed data through {through.Local():MMM d, h:mm tt}. Later usage is a live estimate{(ratio != 1 ? $" (×{ratio:0.00} calibrated)" : "")}.", 12, wrap: true));
        }

        return Ui.V(12, tiles, billed, CumulativeCard(f, width), DailyBarsCard(m, width), CacheCard(m), InsightsRow(m));
    }

    static string VsText(double? v) => v is { } x ? (x >= 0 ? "+" : "") + x.Pct() : "—";

    static UIElement CumulativeCard(ForecastView f, double width)
    {
        var start = f.cycleStartMs.Local();
        var end = f.cycleEndMs.Local();
        var points = f.points.Select(p => new Charts.LinePoint(p.startMs.Local().Date, p.cumulative, (int)p.kind)).ToList();

        UIElement Tip(DateTime day)
        {
            var actual = f.points.FirstOrDefault(p => p.kind == PointKind.Actual && p.startMs.Local().Date == day);
            var projected = f.points.FirstOrDefault(p => p.kind == PointKind.Projected && p.startMs.Local().Date == day);
            var pace = f.points.FirstOrDefault(p => p.kind == PointKind.Pace && p.startMs.Local().Date == day);
            return Ui.V(2,
                Ui.Text(day.ToString("ddd MMM d"), 12, FontWeights.SemiBold),
                actual is not null ? Ui.Text($"Spent {actual.cumulative.Usd()}", 12)
                    : projected is not null ? Ui.Text($"Projected {projected.cumulative.Usd()}", 12) : null,
                pace is not null ? Ui.Secondary($"Pace {pace.cumulative.Usd()}") : null);
        }

        var legend = Ui.H(10,
            Charts.LegendLine("Actual", Theme.Coral, false),
            Charts.LegendLine("Projected", Theme.Coral, true, 0.6),
            f.budget > 0 ? Charts.LegendLine("Pace", Theme.TextSecondary, true, 0.5) : null);
        return Ui.Card("Cycle spend", legend, Charts.Cumulative(points, start, end, f.budget, width, 160, Tip));
    }

    static UIElement DailyBarsCard(AppModel m, double width)
    {
        var days = m.LastDays(30);
        var ids = days.SelectMany(d => d.byModel.Keys).Distinct()
            .OrderBy(Theme.SlotOrder).ThenBy(id => id).ToList();
        var hasOther = false;
        var stacks = days.Select(d =>
        {
            var parts = ids.Where(d.byModel.ContainsKey)
                .Select(id => (m.DisplayName(id), d.byModel[id], Theme.ForModel(id))).ToList();
            var rest = d.cost - d.byModel.Values.Sum();
            if (rest > 0.005) { parts.Add(("Other", rest, Theme.Other)); hasOther = true; }
            return new Charts.Stack(d.startMs.Local().Date, parts);
        }).ToList();
        var avg = days.Sum(d => d.cost) / 30;

        if (stacks.All(s => s.Parts.Count == 0))
            return Ui.Card("Last 30 days", null, Ui.Empty("No usage in the last 30 days"));

        UIElement Tip(Charts.Stack s) => Ui.V(2,
            Ui.Text(s.Day.ToString("ddd MMM d"), 12, FontWeights.SemiBold),
            Ui.Text($"Total {s.Parts.Sum(p => p.value).Usd()}", 12, FontWeights.Medium),
            Ui.V(2, s.Parts.OrderByDescending(p => p.value).Select(p => (UIElement)Ui.H(4, Ui.Dot(p.color, 6), Ui.Text($"{p.name} {p.value.Usd()}", 11))).ToArray()));

        var chart = Charts.StackedBars(stacks, width, 170, avg,
            i => (stacks.Count - 1 - i) % 7 == 0 ? stacks[i].Day.ToString("MMM d") : null, Tip);
        var legendItems = ids.Select(id => (m.DisplayName(id), Theme.ForModel(id))).ToList();
        if (hasOther) legendItems.Add(("Other", Theme.Other));
        return Ui.Card("Last 30 days", Ui.Secondary($"avg {avg.Usd()}/day"), chart, Charts.Legend(legendItems));
    }

    static UIElement CacheCard(AppModel m)
    {
        var savings = m.CacheSavings(StatRange.Cycle);
        var classes = m.TokenClasses(StatRange.Cycle);
        var total = Math.Max(classes.Sum(c => (double)c.tokens), 1);
        var readShare = (classes.FirstOrDefault(c => c.name == "Cache read")?.tokens ?? 0) / total;
        var right = Ui.V(2, Ui.Figure(readShare.Pct(), 18), Ui.Secondary("of tokens were cache hits"));
        right.HorizontalAlignment = HorizontalAlignment.Right;
        foreach (var c in right.Children) ((FrameworkElement)c).HorizontalAlignment = HorizontalAlignment.Right;
        return Ui.Card("Prompt caching", null,
            Ui.Spread(Ui.V(2, Ui.Figure(savings.Usd(), 22, Theme.Status(BudgetStatus.UnderBudget)), Ui.Secondary("saved this cycle by cache reads")), right))
            .Tip("What cache-read tokens would have cost at the normal input price, minus what they actually cost");
    }

    static UIElement InsightsRow(AppModel m)
    {
        var i = m.Insights;
        var hour = m.Snap.busiestHourLast30;
        var messages = m.ModelStats(StatRange.Cycle).Aggregate(0UL, (a, s) => a + s.messages);
        return Ui.Columns(2, 8,
        [
            Ui.StatTile("Peak day", i.peakDay is null ? "—" : i.peakCost.Usd(),
                i.peakDay?.ToDateOnly().ToString("ddd MMM d, yyyy"), ""),
            Ui.StatTile("Active streak", $"{i.activeStreak} day{(i.activeStreak == 1 ? "" : "s")}",
                $"{i.activeDaysThisCycle} active days this cycle", ""),
            Ui.StatTile("Busiest hour", hour is { } h ? Charts.HourLabel(h) : "—", "last 30 days", ""),
            Ui.StatTile("Messages", messages.ToString("N0"), "this cycle", ""),
        ]);
    }
}

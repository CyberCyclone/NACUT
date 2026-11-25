using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using uniffi.nacut;

namespace Nacut.Views;

/// <summary>Cost by model (donut + rows), token classes and billed-vs-local reconciliation (ModelsTab.swift).</summary>
static class ModelsTab
{
    public static UIElement Build(AppModel m, double width, Action rebuild)
    {
        var range = Prefs.Range("modelsRange");
        var stats = m.ModelStats(range);
        var total = stats.Sum(s => s.cost);
        var stack = Ui.V(12, RangePicker(range, r => { Prefs.Set("modelsRange", r.ToString()); rebuild(); }));

        if (stats.Count == 0)
        {
            stack.Children.Add(Ui.Card(null, null, Ui.Empty("No model usage in this range")));
        }
        else
        {
            var donut = Charts.Donut(stats.Select(s => (s.name, s.cost, Theme.ForModel(s.id))).ToList(), 130, total.UsdCompact());
            var legend = Ui.V(6, stats.Take(6).Select(s => (UIElement)Ui.Spread(
                Ui.H(6, Ui.Swatch(Theme.ForModel(s.id)), Ui.Text(s.name, 13)),
                Ui.Secondary(s.share.Pct(), 13))).ToArray());
            legend.VerticalAlignment = VerticalAlignment.Center;
            var row = new Grid { ColumnSpacing = 16 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.Children.Add(donut);
            Grid.SetColumn(legend, 1);
            row.Children.Add(legend);
            stack.Children.Add(Ui.Card("Cost by model", null, row));

            foreach (var s in stats) stack.Children.Add(ModelRow(s));
            stack.Children.Add(TokenClassCard(m, range));
            if (stats.Any(s => s.estimated))
            {
                stack.Children.Add(Ui.H(6, Ui.Icon("", 12),
                    Ui.Secondary("Some models aren't in the bundled price list, so they're priced like the newest model in their family. Add exact prices in Settings → Pricing.", 12, wrap: true)));
            }
        }

        if (m.Snap.reconciliation.Count > 0) stack.Children.Add(ReconciliationCard(m, width));
        return stack;
    }

    public static Segmented RangePicker(StatRange current, Action<StatRange> changed)
    {
        var ranges = new[] { StatRange.Cycle, StatRange.Last30, StatRange.AllTime };
        var seg = new Segmented { HorizontalAlignment = HorizontalAlignment.Stretch };
        foreach (var r in ranges) seg.Items.Add(new SegmentedItem { Content = r.Title() });
        seg.SelectedIndex = Array.IndexOf(ranges, current);
        seg.SelectionChanged += (_, _) =>
        {
            if (seg.SelectedIndex >= 0 && ranges[seg.SelectedIndex] != current) changed(ranges[seg.SelectedIndex]);
        };
        return seg;
    }

    static UIElement ModelRow(ModelView s)
    {
        var title = Ui.H(6, Ui.Dot(Theme.ForModel(s.id), 9), Ui.Text(s.name, 14, FontWeights.SemiBold));
        if (s.estimated) title.Children.Add(Ui.Icon("", 12).Tip("Estimated price"));
        UIElement Metric(string t, string v) => Ui.V(1, Ui.Secondary(t, 11), Ui.Text(v, 12, FontWeights.Medium));
        return Ui.Box(Ui.V(6,
            Ui.Spread(title, Ui.Text(s.cost.Usd(), 14, FontWeights.SemiBold).Tip(s.cost.UsdExact())),
            Charts.Bar(s.share, Theme.ForModel(s.id), 5),
            Ui.Columns(4, 10, [Metric("Messages", s.messages.ToString("N0")), Metric("Tokens", s.tokens.Tokens()), Metric("Per msg", s.costPerMessage.UsdExact()), Metric("Fast", s.fastShare.Pct())]),
            Ui.Columns(4, 10, [Metric("Input", s.input.Tokens()), Metric("Output", s.output.Tokens()), Metric("Cache write", s.cacheWrite.Tokens()), Metric("Cache read", s.cacheRead.Tokens())])),
            14, 12);
    }

    static UIElement TokenClassCard(AppModel m, StatRange range)
    {
        var classes = m.TokenClasses(range);
        var total = Math.Max(classes.Sum(c => c.value), 0.0001);
        var top = classes.OrderByDescending(c => c.value).FirstOrDefault();
        var bars = Charts.HBars(classes.Select(c => (c.name, c.value, $"{c.value.UsdCompact()} · {c.tokens.Tokens()} tok")).ToList(), Theme.Coral);
        return Ui.Card("Where the money goes", null, bars,
            top is { value: > 0 } ? Ui.Secondary($"{top.name} is {(top.value / total).Pct()} of cost in this range.") : null);
    }

    static UIElement ReconciliationCard(AppModel m, double width)
    {
        var days = m.Snap.reconciliation;
        var api = days.Sum(d => d.api);
        var logs = days.Sum(d => d.logs);
        var chart = Charts.GroupedBars(days.Select(d => (d.date.ToDateOnly().ToDateTime(TimeOnly.MinValue), d.api, d.logs)).ToList(),
            Theme.Categorical(0), Theme.Coral, width, 130);
        var text = logs > 0
            ? $"Over {days.Count} billed days the API shows {api.Usd()} vs {logs.Usd()} from local logs ({(api / logs).Pct()}). Live estimates are scaled by ×{m.Api.calibration.ratio:0.00}."
            : "No overlapping days yet.";
        return Ui.Card("Billed vs local estimate", null, chart,
            Charts.Legend([("Billed (API)", Theme.Categorical(0)), ("Local logs", Theme.Coral)]),
            Ui.Secondary(text, 12, wrap: true));
    }
}

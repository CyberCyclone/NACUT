using System.Globalization;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using uniffi.nacut;

namespace Nacut.Views;

/// <summary>Month heatmap with swipe/arrow paging, a month summary and per-day detail (CalendarTab.swift).</summary>
static class CalendarTab
{
    // Survives the panel rebuilding on every data change.
    static DateOnly? visibleMonth;
    static DateOnly? selectedDay;

    public static UIElement Build(AppModel m, double width, Action rebuild)
    {
        var series = m.Series;
        var thisMonth = new DateOnly(DateTime.Now.Year, DateTime.Now.Month, 1);
        var first = series.Keys.Count > 0 ? series.Keys.Min() : thisMonth;
        var months = new List<DateOnly>();
        for (var mo = new DateOnly(first.Year, first.Month, 1); mo <= thisMonth; mo = mo.AddMonths(1)) months.Add(mo);
        if (months.Count == 0) months.Add(thisMonth);
        var current = visibleMonth is { } v && months.Contains(v) ? v : months[^1];

        // Scale shared by all months so colors are comparable: the 95th percentile active day.
        var costs = series.Values.Select(d => d.cost).Where(c => c > 0).OrderBy(c => c).ToList();
        var scaleMax = costs.Count == 0 ? 1 : Math.Max(costs[Math.Min((int)(costs.Count * 0.95), costs.Count - 1)], 0.01);

        var title = Ui.Text(current.ToString("MMMM yyyy"), 18, FontWeights.SemiBold);
        var todayButton = new Button { Content = "Today", Padding = new Thickness(10, 3, 10, 4), IsEnabled = current != months[^1] };

        var flip = new FlipView { Height = 300, Background = null };
        foreach (var mo in months) flip.Items.Add(MonthGrid(m, mo, scaleMax, series, rebuild));
        flip.SelectedIndex = months.IndexOf(current);
        var summaryHost = new ContentControl { HorizontalContentAlignment = HorizontalAlignment.Stretch, Content = MonthSummary(series, current) };
        flip.SelectionChanged += (_, _) =>
        {
            if (flip.SelectedIndex < 0) return;
            visibleMonth = months[flip.SelectedIndex];
            title.Text = visibleMonth.Value.ToString("MMMM yyyy");
            todayButton.IsEnabled = flip.SelectedIndex != months.Count - 1;
            summaryHost.Content = MonthSummary(series, visibleMonth.Value);
        };
        todayButton.Click += (_, _) => flip.SelectedIndex = months.Count - 1;

        var stack = Ui.V(12, Ui.Spread(title, todayButton), flip, summaryHost);
        if (selectedDay is { } day)
            stack.Children.Add(DayDetail(m, day, width, () => { selectedDay = null; rebuild(); }));
        else
            stack.Children.Add(Ui.Secondary("Swipe or use ← → to change month. Click a day for details.", 12, wrap: true));
        return stack;
    }

    static UIElement MonthGrid(AppModel m, DateOnly month, double scaleMax, Dictionary<DateOnly, DayEntry> series, Action rebuild)
    {
        var culture = CultureInfo.CurrentCulture;
        var firstDay = (int)culture.DateTimeFormat.FirstDayOfWeek;
        var g = new Grid { ColumnSpacing = 4, RowSpacing = 4, VerticalAlignment = VerticalAlignment.Top };
        for (var c = 0; c < 7; c++) g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        for (var c = 0; c < 7; c++)
        {
            var name = culture.DateTimeFormat.ShortestDayNames[(firstDay + c) % 7];
            var label = Ui.Text(name, 11, FontWeights.Medium, Theme.TextSecondary);
            label.HorizontalAlignment = HorizontalAlignment.Center;
            Grid.SetColumn(label, c);
            g.Children.Add(label);
        }
        var lead = ((int)month.DayOfWeek - firstDay + 7) % 7;
        var count = DateTime.DaysInMonth(month.Year, month.Month);
        for (var r = 0; r < 6; r++) g.RowDefinitions.Add(new RowDefinition { Height = new GridLength(40) });
        var today = CoreExtensions.Today;
        for (var d = 0; d < count; d++)
        {
            var day = month.AddDays(d);
            var cell = DayCell(day, series.GetValueOrDefault(day), scaleMax, day == selectedDay, day == today, day > today);
            cell.Tapped += (_, _) =>
            {
                selectedDay = selectedDay == day ? null : day;
                visibleMonth = month;
                rebuild();
            };
            var slot = lead + d;
            Grid.SetRow(cell, 1 + slot / 7);
            Grid.SetColumn(cell, slot % 7);
            g.Children.Add(cell);
        }
        return g;
    }

    static Border DayCell(DateOnly day, DayEntry? entry, double scaleMax, bool selected, bool isToday, bool future)
    {
        var value = entry?.cost ?? 0;
        var fraction = Math.Min(value / scaleMax, 1);
        var ink = future ? Theme.TextTertiary : Theme.InkOnRamp(fraction);
        var top = Ui.H(2, Ui.Text(day.Day.ToString(), 11, isToday ? FontWeights.Bold : FontWeights.Normal, ink));
        top.VerticalAlignment = VerticalAlignment.Top;
        top.Opacity = future ? 1 : 0.8;
        if (entry?.source == CostSource.Estimated)
            top.Children.Add(Ui.Dot(Theme.TextSecondary, 3).Tip("Live estimate (not yet billed)"));
        var content = new Grid();
        content.Children.Add(top);
        if (value > 0)
        {
            var amount = Ui.Text(value.UsdCompact(), 11, FontWeights.SemiBold, Theme.InkOnRamp(fraction), font: Ui.DisplayFont);
            amount.VerticalAlignment = VerticalAlignment.Bottom;
            content.Children.Add(amount);
        }
        var border = new Border
        {
            Child = content,
            Padding = new Thickness(5, 3, 5, 3),
            CornerRadius = new CornerRadius(7),
            Background = Theme.Brush(value > 0 ? Theme.Ramp(fraction) : Theme.SubtleFill),
            BorderThickness = new Thickness(selected || isToday ? 1.5 : 1),
            BorderBrush = selected ? Theme.Brush(Theme.TextPrimary) : isToday ? Theme.Brush(Theme.Coral) : null,
        };
        border.PointerEntered += (_, _) => { if (!selected && !isToday) border.BorderBrush = Theme.Brush(Theme.TextSecondary, 0.5); };
        border.PointerExited += (_, _) => { if (!selected && !isToday) border.BorderBrush = null; };
        var tip = $"{day:dddd, MMMM d}: {value.UsdExact()}" + (entry is not null ? $" · {entry.messages:N0} messages" : "");
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(border, tip);
        return border.Tip(tip);
    }

    static UIElement MonthSummary(Dictionary<DateOnly, DayEntry> series, DateOnly month)
    {
        var days = series.Where(kv => kv.Key.Year == month.Year && kv.Key.Month == month.Month && kv.Value.cost > 0).ToList();
        var total = days.Sum(kv => kv.Value.cost);
        var busiest = days.OrderByDescending(kv => kv.Value.cost).FirstOrDefault();
        UIElement Item(string title, string value) => Ui.Box(Ui.V(2, Ui.Secondary(title, 11), Ui.Text(value, 13, FontWeights.SemiBold)), 10, 8, Theme.SubtleFill, stroke: false);
        return Ui.Columns(4, 8,
        [
            Item("Month total", total.Usd()),
            Item("Avg / active day", days.Count == 0 ? "—" : (total / days.Count).Usd()),
            Item("Active days", days.Count.ToString()),
            Item("Busiest", days.Count == 0 ? "—" : $"{busiest.Key.Day} · {busiest.Value.cost.UsdCompact()}"),
        ]);
    }

    static UIElement DayDetail(AppModel m, DateOnly day, double width, Action close)
    {
        var models = m.DayModels(day);
        var hourly = m.Hourly(day);
        var sessions = m.DaySessions(day, 5);
        var total = m.Series.TryGetValue(day, out var e) ? e.cost : 0;
        var trailing = Ui.H(4, Ui.Text(total.Usd(), 14, FontWeights.SemiBold), Ui.IconButton("", "Close", close));
        var title = day.ToString("dddd, MMMM d");

        if (models.Count == 0)
            return Ui.Card(title, trailing, Ui.Empty(total > 0 ? "Billed via API · no local detail for this day" : "No usage this day"));

        // Hourly parts are keyed by display name, which two model ids can share: the first (costliest) one's color wins.
        var colorOf = models.GroupBy(x => x.name).ToDictionary(g => g.Key, g => Theme.ForModel(g.First().id));
        var stacks = Enumerable.Range(0, 24).Select(h => new Charts.Stack(day.ToDateTime(TimeOnly.MinValue).AddHours(h),
            hourly.Where(x => x.hour == h).Select(x => (x.model, x.cost, colorOf.GetValueOrDefault(x.model, Theme.Other))).ToList())).ToList();
        UIElement Tip(Charts.Stack s) => Ui.V(2,
            Ui.Text(s.Day.ToString("h tt"), 12, FontWeights.SemiBold),
            Ui.V(2, s.Parts.Select(p => (UIElement)Ui.H(4, Ui.Dot(p.color, 6), Ui.Text($"{p.name} {p.value.Usd()}", 11))).ToArray()));
        var chart = Charts.StackedBars(stacks, width, 110, null, i => i % 6 == 0 ? Charts.HourLabel(i) : null, Tip, barWidth: 9);

        var stack = Ui.V(8, chart);
        foreach (var x in models)
        {
            stack.Children.Add(Ui.Spread(
                Ui.H(6, Ui.Dot(Theme.ForModel(x.id)), Ui.Text(x.name, 13)),
                Ui.H(10, Ui.Secondary($"{x.messages:N0} msgs"), new TextBlock { Text = x.cost.Usd(), Width = 70, TextAlignment = TextAlignment.Right, FontWeight = FontWeights.Medium })));
        }
        if (sessions.Count > 0)
        {
            stack.Children.Add(Ui.Divider());
            stack.Children.Add(Ui.Text("Sessions", 12, FontWeights.SemiBold, Theme.TextSecondary));
            foreach (var s in sessions) stack.Children.Add(ProjectsTab.SessionRow(s));
        }
        return Ui.Card(title, trailing, stack);
    }
}

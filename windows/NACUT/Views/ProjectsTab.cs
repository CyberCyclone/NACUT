using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using uniffi.nacut;

namespace Nacut.Views;

/// <summary>Projects, top sessions, where Claude is used, agent split and the weekday × hour heatmap (ProjectsTab.swift).</summary>
static class ProjectsTab
{
    static bool showAllProjects;

    public static UIElement Build(AppModel m, double width, Action rebuild)
    {
        var range = Prefs.Range("projectsRange");
        var projects = m.ProjectStats(range);
        var sessions = m.Sessions(range, 8);
        var entry = m.EntrypointStats(range);
        var split = m.AgentSplit(range);

        UIElement? toggle = projects.Count > 6
            ? Ui.Link(showAllProjects ? "Show less" : $"Show all {projects.Count}", () => { showAllProjects = !showAllProjects; rebuild(); })
            : null;
        var projectRows = Ui.V(8);
        if (projects.Count == 0) projectRows.Children.Add(Ui.Empty("No project usage in this range"));
        foreach (var p in showAllProjects ? projects : projects.Take(6))
        {
            projectRows.Children.Add(Ui.V(4,
                Ui.Spread(Ui.H(6, Ui.Icon("", 12, Theme.Coral), Ui.Text(p.name, 13, FontWeights.Medium)), Ui.Text(p.cost.Usd(), 13, FontWeights.SemiBold)),
                Charts.Bar(p.share, Theme.Coral, 5),
                Ui.Secondary($"{p.sessions} session{(p.sessions == 1 ? "" : "s")} · {p.messages:N0} msgs · last active {Relative(p.lastActiveMs.Local())}", 11)).Tip(p.path));
        }

        var sessionRows = Ui.V(4);
        if (sessions.Count == 0) sessionRows.Children.Add(Ui.Empty("No sessions in this range"));
        foreach (var s in sessions) sessionRows.Children.Add(SessionRow(s));

        var shares = Ui.Columns(2, 12,
        [
            Ui.Card("Where you use Claude", null, Charts.SegmentedShare(entry.Select(e => (e.name, e.value)).ToList())),
            Ui.Card("Agents", null, Charts.SegmentedShare([("Main agent", split.main), ("Subagents", split.subagents)])),
        ]);

        return Ui.V(12,
            ModelsTab.RangePicker(range, r => { Prefs.Set("projectsRange", r.ToString()); rebuild(); }),
            Ui.Card("Projects", toggle, projectRows),
            Ui.Card("Top sessions", null, sessionRows),
            shares,
            HeatmapCard(m, range));
    }

    public static UIElement SessionRow(SessionView s)
    {
        var chips = new WrapPanel { HorizontalSpacing = 4 };
        foreach (var model in s.models)
        {
            chips.Children.Add(new Border
            {
                Child = Ui.Text(model, 11, FontWeights.Medium),
                Padding = new Thickness(5, 0, 5, 1),
                CornerRadius = new CornerRadius(8),
                Background = Theme.Brush(Theme.TrackFill),
            });
        }
        var left = Ui.V(2,
            Ui.Text(s.title, 13, FontWeights.Medium),
            Ui.Secondary($"{s.project} · {s.startMs.Local():MMM d, h:mm tt} · {Duration(s.durationSecs)} · {s.entrypoint}", 11),
            chips);
        var right = Ui.V(2, Ui.Text(s.cost.Usd(), 13, FontWeights.SemiBold), Ui.Secondary($"{s.messages:N0} msgs", 11));
        foreach (var c in right.Children) ((FrameworkElement)c).HorizontalAlignment = HorizontalAlignment.Right;
        right.VerticalAlignment = VerticalAlignment.Top;
        var row = Ui.Spread(left, right);
        row.Padding = new Thickness(0, 3, 0, 3);
        return row;
    }

    static string Duration(double secs)
    {
        var s = (int)secs;
        if (s < 60) return "<1m";
        int h = s / 3600, m = s % 3600 / 60;
        if (h >= 48) return $"{h / 24}d {h % 24}h";
        return h > 0 ? $"{h}h {m}m" : $"{m}m";
    }

    public static string Relative(DateTime t)
    {
        var d = DateTime.Now - t;
        if (d.TotalMinutes < 1) return "just now";
        if (d.TotalHours < 1) return $"{(int)d.TotalMinutes} min ago";
        if (d.TotalDays < 1) return $"{(int)d.TotalHours} hr ago";
        if (d.TotalDays < 2) return "yesterday";
        if (d.TotalDays < 30) return $"{(int)d.TotalDays} days ago";
        return t.ToString("MMM d");
    }

    static UIElement HeatmapCard(AppModel m, StatRange range)
    {
        var grid = m.Heatmap(range);
        var days = new[] { "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun" };
        var heat = Charts.Heatmap(grid, (r, c) => $"{days[r]} {Charts.HourLabel(c)}: {(r < grid.Count && c < grid[r].Count ? grid[r][c] : 0).Usd()}");
        return Ui.Card("When you use Claude", Charts.RampLegend(), heat);
    }
}

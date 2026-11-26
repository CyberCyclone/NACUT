using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;
using Windows.UI;

namespace Nacut;

/// <summary>
/// Charts drawn with XAML shapes, following the same mark specs as the Mac app's Swift Charts:
/// thin marks, rounded bar ends, 2px lines, dashed projections, recessive axes, native tooltips.
/// Sizes are passed in, since the panel has a fixed width.
/// </summary>
static class Charts
{
    const double AxisFont = 11;

    // MARK: Rings and bars

    /// <summary>Progress ring: solid arc for the value, faint arc to the projection.</summary>
    public static Grid Ring(double fraction, double? projected, Color color, Color? projectedColor, double size, double thickness, UIElement? center)
    {
        var g = new Grid { Width = size, Height = size };
        g.Children.Add(new Ellipse { Stroke = Theme.Brush(Theme.TrackFill), StrokeThickness = thickness, Margin = new Thickness(0) });
        if (projected is { } p && p > fraction)
            g.Children.Add(Arc(size, thickness, Math.Min(p, 1), projectedColor ?? color, 0.25));
        g.Children.Add(Arc(size, thickness, Math.Clamp(Math.Max(fraction, 0.004), 0, 1), color, 1));
        if (center is FrameworkElement c)
        {
            c.HorizontalAlignment = HorizontalAlignment.Center;
            c.VerticalAlignment = VerticalAlignment.Center;
            g.Children.Add(c);
        }
        return g;
    }

    static Microsoft.UI.Xaml.Shapes.Path Arc(double size, double thickness, double fraction, Color color, double opacity)
    {
        var r = (size - thickness) / 2;
        var c = size / 2;
        var sweep = Math.Min(fraction, 0.9999) * 360;
        Point At(double deg) => new(c + r * Math.Cos((deg - 90) * Math.PI / 180), c + r * Math.Sin((deg - 90) * Math.PI / 180));
        var fig = new PathFigure { StartPoint = At(0), IsClosed = false };
        fig.Segments.Add(new ArcSegment
        {
            Point = At(sweep),
            Size = new Size(r, r),
            IsLargeArc = sweep > 180,
            SweepDirection = SweepDirection.Clockwise,
        });
        var geo = new PathGeometry();
        geo.Figures.Add(fig);
        return new Microsoft.UI.Xaml.Shapes.Path
        {
            Data = geo,
            Stroke = Theme.Brush(color),
            StrokeThickness = thickness,
            StrokeStartLineCap = PenLineCap.Round,
            StrokeEndLineCap = PenLineCap.Round,
            Opacity = opacity,
        };
    }

    /// <summary>Capsule bar: track, optional faint projection, solid value.</summary>
    public static Grid Bar(double fraction, Color color, double height = 6, double? projected = null, Color? projectedColor = null)
    {
        var g = new Grid { Height = height };
        g.Children.Add(new Border { Background = Theme.Brush(Theme.TrackFill), CornerRadius = new CornerRadius(height / 2) });
        if (projected is { } p && p > fraction) g.Children.Add(Fill(p, projectedColor ?? color, 0.28, height));
        g.Children.Add(Fill(Math.Max(fraction, 0.01), color, 1, height));
        return g;
    }

    static Grid Fill(double fraction, Color color, double opacity, double height)
    {
        var f = Math.Clamp(fraction, 0, 1);
        var g = new Grid();
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(f, GridUnitType.Star) });
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1 - f, GridUnitType.Star) });
        g.Children.Add(new Border { Background = Theme.Brush(color), Opacity = opacity, CornerRadius = new CornerRadius(height / 2) });
        return g;
    }

    /// <summary>Part-to-whole as one segmented bar with a legend (≤ 4 parts).</summary>
    public static StackPanel SegmentedShare(IReadOnlyList<(string name, double value)> items)
    {
        var total = Math.Max(items.Sum(i => i.value), 0.0001);
        var shown = items.Take(4).ToList();
        var bar = new Grid { Height = 10, ColumnSpacing = 2 };
        for (var i = 0; i < shown.Count; i++)
        {
            if (shown[i].value <= 0) continue;
            bar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(Math.Max(shown[i].value / total, 0.01), GridUnitType.Star) });
            var seg = new Border { Background = Theme.Brush(Theme.Categorical(i)), CornerRadius = new CornerRadius(3) };
            Grid.SetColumn(seg, bar.ColumnDefinitions.Count - 1);
            bar.Children.Add(seg.Tip($"{shown[i].name}: {shown[i].value.Usd()}"));
        }
        var stack = Ui.V(8, bar);
        for (var i = 0; i < shown.Count; i++)
        {
            stack.Children.Add(Ui.Spread(
                Ui.H(6, Ui.Swatch(Theme.Categorical(i), 8), Ui.Text(shown[i].name, 12)),
                Ui.Secondary((shown[i].value / total).Pct(), 12)).Tip(shown[i].value.Usd()));
        }
        return stack;
    }

    // MARK: Axes

    static TextBlock AxisLabel(string s) => new()
    {
        Text = s,
        FontSize = AxisFont,
        Foreground = Theme.Brush(Theme.TextSecondary),
    };

    static Line HLine(double x1, double x2, double y, Color c, double thickness = 1, double opacity = 1, bool dashed = false) => new()
    {
        X1 = x1, X2 = x2, Y1 = y, Y2 = y,
        Stroke = Theme.Brush(c),
        StrokeThickness = thickness,
        Opacity = opacity,
        StrokeDashArray = dashed ? [3, 3] : null,
    };

    static void Place(Canvas canvas, UIElement e, double x, double y)
    {
        Canvas.SetLeft(e, x);
        Canvas.SetTop(e, y);
        canvas.Children.Add(e);
    }

    /// <summary>Grid lines and $ labels on the leading edge; returns the y scale.</summary>
    static Func<double, double> YAxis(Canvas canvas, double max, double left, double top, double plotW, double plotH)
    {
        var ticks = uniffi.nacut.NacutMethods.ChartTicks(max, 3);
        var yMax = ticks[^1];
        double Y(double v) => top + plotH - v / yMax * plotH;
        foreach (var t in ticks)
        {
            canvas.Children.Add(HLine(left, left + plotW, Y(t), Theme.Divider));
            var label = AxisLabel(t.UsdCompact());
            label.Width = left - 6;
            label.TextAlignment = TextAlignment.Right;
            Place(canvas, label, 0, Y(t) - 8);
        }
        return Y;
    }

    public static Border TooltipBox(UIElement content) => new()
    {
        Child = content,
        Padding = new Thickness(8, 5, 8, 5),
        CornerRadius = new CornerRadius(6),
        Background = Theme.Brush(Theme.IsDark ? Color.FromArgb(0xF0, 0x2C, 0x2C, 0x2C) : Color.FromArgb(0xF5, 0xFF, 0xFF, 0xFF)),
        BorderBrush = Theme.Brush(Theme.CardStroke),
        BorderThickness = new Thickness(1),
        IsHitTestVisible = false,
    };

    // MARK: Cumulative spend (line + area + projection + budget pace)

    public record LinePoint(DateTime Day, double Value, int Series);

    /// <summary>
    /// Series 0 = actual (solid, filled), 1 = projected (dashed), 2 = budget pace (dashed, grey).
    /// Hovering shows a crosshair with the day's values.
    /// </summary>
    public static Canvas Cumulative(IReadOnlyList<LinePoint> points, DateTime start, DateTime end, double budget, double width, double height,
        Func<DateTime, UIElement> tooltip)
    {
        var canvas = new Canvas { Width = width, Height = height, Background = new SolidColorBrush(Colors.Transparent) };
        const double left = 40, bottom = 18, top = 14;
        var plotW = width - left - 4;
        var plotH = height - bottom - top;
        var max = Math.Max(points.Count == 0 ? 0 : points.Max(p => p.Value), budget) * 1.05;
        var Y = YAxis(canvas, Math.Max(max, 0.1), left, top, plotW, plotH);
        var span = (end - start).TotalDays;
        double X(DateTime d) => left + (d - start).TotalDays / span * plotW;

        // Weekly date labels, skipping any that would run off the trailing edge.
        for (var d = start; d < end; d = d.AddDays(7))
        {
            if (X(d) + 36 > width) break;
            Place(canvas, AxisLabel(d.ToString("MMM d")), X(d) - 2, top + plotH + 2);
        }

        var coral = Theme.Coral;
        var actual = points.Where(p => p.Series == 0).OrderBy(p => p.Day).ToList();
        if (actual.Count > 0)
        {
            var area = new Polygon
            {
                Fill = new LinearGradientBrush
                {
                    StartPoint = new Point(0, 0),
                    EndPoint = new Point(0, 1),
                    GradientStops =
                    {
                        new GradientStop { Color = Color.FromArgb(56, coral.R, coral.G, coral.B), Offset = 0 },
                        new GradientStop { Color = Color.FromArgb(5, coral.R, coral.G, coral.B), Offset = 1 },
                    },
                },
            };
            foreach (var p in actual) area.Points.Add(new Point(X(p.Day), Y(p.Value)));
            area.Points.Add(new Point(X(actual[^1].Day), Y(0)));
            area.Points.Add(new Point(X(actual[0].Day), Y(0)));
            canvas.Children.Add(area);
        }

        foreach (var series in new[] { 2, 1, 0 })
        {
            var pts = points.Where(p => p.Series == series).OrderBy(p => p.Day).ToList();
            if (pts.Count < 2) continue;
            var line = new Polyline
            {
                Stroke = Theme.Brush(series == 2 ? Theme.TextSecondary : coral),
                Opacity = series == 0 ? 1 : series == 1 ? 0.6 : 0.5,
                StrokeThickness = series == 0 ? 2.5 : 2,
                StrokeLineJoin = PenLineJoin.Round,
                StrokeStartLineCap = PenLineCap.Round,
                StrokeEndLineCap = PenLineCap.Round,
                StrokeDashArray = series == 0 ? null : [2, 2],
            };
            foreach (var p in pts) line.Points.Add(new Point(X(p.Day), Y(p.Value)));
            canvas.Children.Add(line);
        }

        if (budget > 0)
        {
            canvas.Children.Add(HLine(left, left + plotW, Y(budget), Theme.TextSecondary, 1, 0.6));
            Place(canvas, AxisLabel($"Budget {budget.UsdCompact()}"), left + 2, Y(budget) - 15);
        }

        AddCrosshair(canvas, left, top, plotW, plotH, start, end, X, tooltip);
        return canvas;
    }

    static void AddCrosshair(Canvas canvas, double left, double top, double plotW, double plotH, DateTime start, DateTime end,
        Func<DateTime, double> X, Func<DateTime, UIElement> tooltip)
    {
        var rule = new Line { Y1 = top, Y2 = top + plotH, Stroke = Theme.Brush(Theme.TextSecondary), Opacity = 0.4, StrokeThickness = 1, Visibility = Visibility.Collapsed };
        canvas.Children.Add(rule);
        var tip = TooltipBox(new Grid());
        tip.Visibility = Visibility.Collapsed;
        var overlay = new Rectangle { Width = plotW, Height = plotH, Fill = new SolidColorBrush(Colors.Transparent) };
        Place(canvas, overlay, left, top);
        canvas.Children.Add(tip);
        DateTime? shown = null;
        overlay.PointerMoved += (_, e) =>
        {
            var x = e.GetCurrentPoint(canvas).Position.X;
            var day = start.AddDays(Math.Round((x - left) / plotW * (end - start).TotalDays)).Date;
            if (day < start) day = start.Date;
            if (day >= end) day = end.AddDays(-1).Date;
            var dx = X(day);
            rule.X1 = rule.X2 = dx;
            rule.Visibility = Visibility.Visible;
            if (shown != day)
            {
                tip.Child = tooltip(day);
                shown = day;
            }
            tip.Visibility = Visibility.Visible;
            tip.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            var w = tip.DesiredSize.Width;
            // A tooltip wider than the chart would make max < min, which Math.Clamp rejects.
            Canvas.SetLeft(tip, Math.Clamp(dx - w / 2, 0, Math.Max(0, canvas.Width - w)));
            Canvas.SetTop(tip, 0);
        };
        overlay.PointerExited += (_, _) =>
        {
            rule.Visibility = Visibility.Collapsed;
            tip.Visibility = Visibility.Collapsed;
            shown = null;
        };
    }

    // MARK: Stacked bars (daily cost by model)

    public record Stack(DateTime Day, IReadOnlyList<(string name, double value, Color color)> Parts);

    /// <summary>
    /// One stacked bar per slot, with a dashed average rule and a native tooltip per bar.
    /// `labelEvery` controls how often the x labels appear; `xLabel` formats them.
    /// </summary>
    public static Canvas StackedBars(IReadOnlyList<Stack> stacks, double width, double height, double? average,
        Func<int, string?> xLabel, Func<Stack, UIElement> tooltip, double barWidth = 0)
    {
        var canvas = new Canvas { Width = width, Height = height };
        const double left = 40, bottom = 18, top = 6;
        var plotW = width - left - 4;
        var plotH = height - bottom - top;
        var max = stacks.Count == 0 ? 1 : Math.Max(stacks.Max(s => s.Parts.Sum(p => p.value)), 0.01);
        var Y = YAxis(canvas, Math.Max(max, 0.1), left, top, plotW, plotH);
        var slot = plotW / Math.Max(stacks.Count, 1);
        var bw = barWidth > 0 ? Math.Min(barWidth, slot - 2) : Math.Max(slot - 2, 1);

        for (var i = 0; i < stacks.Count; i++)
        {
            var s = stacks[i];
            var x = left + i * slot + (slot - bw) / 2;
            var y = Y(0);
            var parts = s.Parts.Where(p => p.value > 0).ToList();
            for (var j = 0; j < parts.Count; j++)
            {
                var h = Y(0) - Y(parts[j].value);
                if (h < 0.5) continue;
                var isTop = j == parts.Count - 1;
                var seg = new Border
                {
                    Width = bw,
                    Height = Math.Max(h - (isTop ? 0 : 1), 0.5),
                    Background = Theme.Brush(parts[j].color),
                    CornerRadius = isTop ? new CornerRadius(Math.Min(2, bw / 2), Math.Min(2, bw / 2), 0, 0) : new CornerRadius(0),
                };
                y -= h;
                Place(canvas, seg, x, y + (isTop ? 0 : 1));
            }
            var label = xLabel(i);
            if (label is not null) Place(canvas, AxisLabel(label), x - 2, top + plotH + 2);

            // Full-height hit target so thin bars are easy to hover.
            var hit = new Rectangle { Width = slot, Height = plotH, Fill = new SolidColorBrush(Colors.Transparent) };
            if (parts.Count > 0) hit.Tip(tooltip(s));
            Place(canvas, hit, left + i * slot, top);
        }

        if (average is { } avg && avg > 0)
            canvas.Children.Add(HLine(left, left + plotW, Y(avg), Theme.TextSecondary, 1, 0.5, dashed: true));
        return canvas;
    }

    /// <summary>Two bars per day (billed vs local), for the reconciliation card.</summary>
    public static Canvas GroupedBars(IReadOnlyList<(DateTime day, double a, double b)> rows, Color ca, Color cb, double width, double height)
    {
        var canvas = new Canvas { Width = width, Height = height };
        const double left = 40, bottom = 18, top = 6;
        var plotW = width - left - 4;
        var plotH = height - bottom - top;
        var max = rows.Count == 0 ? 1 : Math.Max(rows.Max(r => Math.Max(r.a, r.b)), 0.01);
        var Y = YAxis(canvas, Math.Max(max, 0.1), left, top, plotW, plotH);
        var slot = plotW / Math.Max(rows.Count, 1);
        var bw = Math.Max((slot - 3) / 2, 1);
        for (var i = 0; i < rows.Count; i++)
        {
            var (day, a, b) = rows[i];
            var x = left + i * slot + 1;
            foreach (var (v, c, dx) in new[] { (a, ca, 0.0), (b, cb, bw + 1) })
            {
                var h = Y(0) - Y(v);
                if (h < 0.5) continue;
                Place(canvas, new Border { Width = bw, Height = h, Background = Theme.Brush(c), CornerRadius = new CornerRadius(2, 2, 0, 0) }, x + dx, Y(v));
            }
            var hit = new Rectangle { Width = slot, Height = plotH, Fill = new SolidColorBrush(Colors.Transparent) };
            hit.Tip($"{day:ddd MMM d}\nBilled {a.Usd()} · Local {b.Usd()}");
            Place(canvas, hit, left + i * slot, top);
            if (i % 7 == 0) Place(canvas, AxisLabel(day.ToString("MMM d")), x - 2, top + plotH + 2);
        }
        return canvas;
    }

    // MARK: Donut

    /// <summary>Donut with a 1.5° gap between sectors. Hovering a sector dims the rest and updates the center.</summary>
    public static Grid Donut(IReadOnlyList<(string name, double value, Color color)> items, double size, string totalLabel)
    {
        var grid = new Grid { Width = size, Height = size };
        var canvas = new Canvas { Width = size, Height = size };
        grid.Children.Add(canvas);
        var title = new TextBlock { Text = "Total", FontSize = 11, Foreground = Theme.Brush(Theme.TextSecondary), HorizontalAlignment = HorizontalAlignment.Center };
        var value = new TextBlock { Text = totalLabel, FontSize = 16, FontWeight = FontWeights.SemiBold, FontFamily = Ui.DisplayFont, HorizontalAlignment = HorizontalAlignment.Center };
        grid.Children.Add(new StackPanel { HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center, Children = { title, value }, IsHitTestVisible = false });

        var total = Math.Max(items.Sum(i => i.value), 0.0001);
        var outer = size / 2;
        var inner = outer * 0.64;
        var c = new Point(outer, outer);
        var angle = 0.0;
        var paths = new List<Microsoft.UI.Xaml.Shapes.Path>();
        foreach (var item in items)
        {
            var sweep = item.value / total * 360;
            if (sweep <= 0) continue;
            var gap = items.Count > 1 ? Math.Min(1.5, sweep / 3) : 0;
            var a0 = angle + gap / 2;
            var a1 = angle + sweep - gap / 2;
            angle += sweep;
            var path = new Microsoft.UI.Xaml.Shapes.Path { Fill = Theme.Brush(item.color), Data = Sector(c, inner, outer, a0, Math.Min(a1, a0 + 359.99)) };
            paths.Add(path);
            var name = item.name;
            var cost = item.value;
            path.PointerEntered += (_, _) =>
            {
                foreach (var p in paths) p.Opacity = p == path ? 1 : 0.35;
                title.Text = name;
                value.Text = cost.UsdCompact();
            };
            path.PointerExited += (_, _) =>
            {
                foreach (var p in paths) p.Opacity = 1;
                title.Text = "Total";
                value.Text = totalLabel;
            };
            canvas.Children.Add(path);
        }
        return grid;
    }

    static Geometry Sector(Point c, double r0, double r1, double a0, double a1)
    {
        Point At(double r, double deg) => new(c.X + r * Math.Cos((deg - 90) * Math.PI / 180), c.Y + r * Math.Sin((deg - 90) * Math.PI / 180));
        var large = a1 - a0 > 180;
        var fig = new PathFigure { StartPoint = At(r1, a0), IsClosed = true };
        fig.Segments.Add(new ArcSegment { Point = At(r1, a1), Size = new Size(r1, r1), IsLargeArc = large, SweepDirection = SweepDirection.Clockwise });
        fig.Segments.Add(new LineSegment { Point = At(r0, a1) });
        fig.Segments.Add(new ArcSegment { Point = At(r0, a0), Size = new Size(r0, r0), IsLargeArc = large, SweepDirection = SweepDirection.Counterclockwise });
        var geo = new PathGeometry();
        geo.Figures.Add(fig);
        return geo;
    }

    // MARK: Horizontal bars (token classes)

    public static Grid HBars(IReadOnlyList<(string name, double value, string annotation)> rows, Color color)
    {
        var g = new Grid { ColumnSpacing = 8, RowSpacing = 6 };
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var max = Math.Max(rows.Count == 0 ? 0 : rows.Max(r => r.value), 0.0001) * 1.6;
        for (var i = 0; i < rows.Count; i++)
        {
            g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var label = Ui.Secondary(rows[i].name, 12);
            Grid.SetRow(label, i);
            g.Children.Add(label);

            var f = Math.Clamp(rows[i].value / max, 0, 1);
            var bar = new Grid { ColumnSpacing = 6 };
            bar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(Math.Max(f, 0.005), GridUnitType.Star) });
            bar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1 - f, GridUnitType.Star) });
            bar.Children.Add(new Border { Height = 14, Background = Theme.Brush(color), CornerRadius = new CornerRadius(0, 3, 3, 0) });
            var ann = Ui.Secondary(rows[i].annotation, 11);
            Grid.SetColumn(ann, 1);
            bar.Children.Add(ann);
            Grid.SetRow(bar, i);
            Grid.SetColumn(bar, 1);
            g.Children.Add(bar);
        }
        return g;
    }

    // MARK: Weekday × hour heatmap

    public static Grid Heatmap(List<List<double>> grid, Func<int, int, string> tip, Action<int, int>? hover = null)
    {
        var days = new[] { "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun" };
        var max = Math.Max(grid.SelectMany(r => r).DefaultIfEmpty(0).Max(), 0.0001);
        var g = new Grid { ColumnSpacing = 3, RowSpacing = 3 };
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(28) });
        for (var c = 0; c < 24; c++) g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (var r = 0; r < 7; r++)
        {
            g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var label = Ui.Secondary(days[r], 11);
            Grid.SetRow(label, r);
            g.Children.Add(label);
            for (var c = 0; c < 24; c++)
            {
                var v = r < grid.Count && c < grid[r].Count ? grid[r][c] : 0;
                var cell = new Border
                {
                    Height = 12,
                    CornerRadius = new CornerRadius(2.5),
                    Background = Theme.Brush(v > 0 ? Theme.Ramp(v / max) : Theme.SubtleFill),
                }.Tip(tip(r, c));
                Grid.SetRow(cell, r);
                Grid.SetColumn(cell, c + 1);
                g.Children.Add(cell);
            }
        }
        g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        foreach (var h in new[] { 0, 6, 12, 18 })
        {
            var label = Ui.Secondary(HourLabel(h), 11);
            Grid.SetRow(label, 7);
            Grid.SetColumn(label, h + 1);
            Grid.SetColumnSpan(label, 6);
            g.Children.Add(label);
        }
        return g;
    }

    public static string HourLabel(int h) => Logic.Formats.HourLabel(h, System.Globalization.CultureInfo.CurrentCulture);

    /// <summary>The ramp legend: "Less ▢▢▢▢▢ More".</summary>
    public static StackPanel RampLegend()
    {
        var p = Ui.H(2, Ui.Secondary("Less", 11));
        for (var i = 0; i < Theme.RampCount; i++) p.Children.Add(Ui.Swatch(Theme.RampStep(i), 9));
        p.Children.Add(Ui.Secondary("More", 11));
        return p;
    }

    /// <summary>Legend chips for a set of series.</summary>
    public static WrapPanel Legend(IEnumerable<(string name, Color color)> items, bool dashedLast = false)
    {
        var w = new WrapPanel { HorizontalSpacing = 10 };
        foreach (var (name, color) in items) w.Children.Add(Ui.H(4, Ui.Swatch(color), Ui.Secondary(name, 11)));
        return w;
    }

    public static StackPanel LegendLine(string name, Color color, bool dashed, double opacity = 1)
    {
        var line = new Line { X1 = 0, X2 = 12, Y1 = 3, Y2 = 3, Stroke = Theme.Brush(color), StrokeThickness = 2, Opacity = opacity, StrokeDashArray = dashed ? [2, 1.5] : null };
        return Ui.H(4, new Canvas { Width = 12, Height = 6, Children = { line } }, Ui.Secondary(name, 11));
    }
}

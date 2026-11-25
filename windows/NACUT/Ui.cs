using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Foundation;
using Windows.UI;
using Windows.UI.Text;

namespace Nacut;

/// <summary>
/// Small builders for the panel's views, mirroring the Mac app's SwiftUI components
/// (Card, StatTile, pills). Views are rebuilt from the latest snapshot rather than data-bound.
/// </summary>
static class Ui
{
    public static readonly FontFamily IconFont = new("Segoe Fluent Icons,Segoe MDL2 Assets");
    public static readonly FontFamily DisplayFont = new("Segoe UI Variable Display,Segoe UI");
    public static readonly FontFamily TextFont = new("Segoe UI Variable Text,Segoe UI");

    public static TextBlock Text(string s, double size = 14, FontWeight? weight = null, Color? color = null,
        bool wrap = false, FontFamily? font = null)
    {
        var t = new TextBlock
        {
            Text = s,
            FontSize = size,
            FontWeight = weight ?? FontWeights.Normal,
            TextWrapping = wrap ? TextWrapping.Wrap : TextWrapping.NoWrap,
            TextTrimming = wrap ? TextTrimming.None : TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (font is not null) t.FontFamily = font;
        if (color is { } c) t.Foreground = Theme.Brush(c);
        return t;
    }

    public static TextBlock Secondary(string s, double size = 12, bool wrap = false) =>
        Text(s, size, color: Theme.TextSecondary, wrap: wrap);

    /// <summary>A large rounded figure (the SwiftUI `.rounded` design's Windows equivalent).</summary>
    public static TextBlock Figure(string s, double size, Color? color = null) =>
        Text(s, size, FontWeights.SemiBold, color, font: DisplayFont);

    /// <summary>One line built from runs, e.g. secondary "Today " then semibold "$4.21".</summary>
    public static TextBlock Runs(double size, params (string text, bool strong)[] parts)
    {
        var t = new TextBlock { FontSize = size, VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis };
        foreach (var (text, strong) in parts)
        {
            var run = new Microsoft.UI.Xaml.Documents.Run { Text = text };
            if (strong) run.FontWeight = FontWeights.SemiBold;
            else run.Foreground = Theme.Brush(Theme.TextSecondary);
            t.Inlines.Add(run);
        }
        return t;
    }

    public static FontIcon Icon(string glyph, double size = 12, Color? color = null) => new()
    {
        Glyph = glyph,
        FontFamily = IconFont,
        FontSize = size,
        Foreground = Theme.Brush(color ?? Theme.TextSecondary),
        VerticalAlignment = VerticalAlignment.Center,
    };

    public static StackPanel V(double spacing, params UIElement?[] children)
    {
        var p = new StackPanel { Spacing = spacing };
        foreach (var c in children) if (c is not null) p.Children.Add(c);
        return p;
    }

    public static StackPanel H(double spacing, params UIElement?[] children)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal, Spacing = spacing };
        foreach (var c in children)
        {
            if (c is null) continue;
            if (c is FrameworkElement f && f.ReadLocalValue(FrameworkElement.VerticalAlignmentProperty) == DependencyProperty.UnsetValue)
                f.VerticalAlignment = VerticalAlignment.Center;
            p.Children.Add(c);
        }
        return p;
    }

    /// <summary>`left` stretches; `right` hugs the trailing edge (SwiftUI's HStack { a; Spacer(); b }).</summary>
    public static Grid Spread(UIElement left, UIElement? right, double spacing = 8)
    {
        var g = new Grid { ColumnSpacing = spacing };
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        g.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        g.Children.Add(left);
        if (right is not null)
        {
            Grid.SetColumn((FrameworkElement)right, 1);
            g.Children.Add(right);
        }
        return g;
    }

    /// <summary>Equal-width columns, wrapping into rows (LazyVGrid with flexible columns).</summary>
    public static Grid Columns(int count, double spacing, IEnumerable<UIElement> items)
    {
        var g = new Grid { ColumnSpacing = spacing, RowSpacing = spacing };
        for (var c = 0; c < count; c++) g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var i = 0;
        foreach (var item in items)
        {
            if (i % count == 0) g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            Grid.SetRow((FrameworkElement)item, i / count);
            Grid.SetColumn((FrameworkElement)item, i % count);
            g.Children.Add(item);
            i++;
        }
        return g;
    }

    public static Border Box(UIElement child, double radius = 12, double padding = 10, Color? fill = null, bool stroke = true) => new()
    {
        Child = child,
        CornerRadius = new CornerRadius(radius),
        Padding = new Thickness(padding),
        Background = Theme.Brush(fill ?? Theme.CardFill),
        BorderBrush = stroke ? Theme.Brush(Theme.CardStroke) : null,
        BorderThickness = new Thickness(stroke ? 1 : 0),
    };

    /// <summary>Rounded card with an optional title row, like the Mac panel's `Card`.</summary>
    public static Border Card(string? title, UIElement? trailing, params UIElement?[] content)
    {
        var stack = V(10);
        if (title is not null || trailing is not null)
            stack.Children.Add(Spread(Text(title ?? "", 13, FontWeights.SemiBold), trailing));
        foreach (var c in content) if (c is not null) stack.Children.Add(c);
        return Box(stack, 14, 12);
    }

    /// <summary>Capsule tinted with a status color.</summary>
    public static Border Pill(UIElement child, Color tint, double hPad = 10, double vPad = 7) => new()
    {
        Child = child,
        // WinUI doesn't clamp oversized radii (it draws an ellipse), so match the one-line height.
        CornerRadius = new CornerRadius(vPad + 9),
        Padding = new Thickness(hPad, vPad, hPad, vPad),
        Background = Theme.Brush(tint, 0.12),
    };

    public static Border StatTile(string title, string value, string? detail, string glyph, Color? tint = null, string? tip = null)
    {
        var stack = V(4,
            H(5, Icon(glyph, 11, tint ?? Theme.Coral), Secondary(title, 12)),
            Figure(value, 18),
            detail is null ? null : Secondary(detail, 11));
        return Box(stack).Tip(tip);
    }

    public static FrameworkElement Empty(string text) => new Grid
    {
        MinHeight = 60,
        Children = { new TextBlock { Text = text, FontSize = 13, Foreground = Theme.Brush(Theme.TextSecondary), HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center } },
    };

    public static Border Divider() => new() { Height = 1, Background = Theme.Brush(Theme.Divider), Margin = new Thickness(0, 2, 0, 2) };

    public static T Tip<T>(this T element, string? tip) where T : DependencyObject
    {
        if (!string.IsNullOrEmpty(tip)) ToolTipService.SetToolTip(element, tip);
        return element;
    }

    public static T Tip<T>(this T element, UIElement content) where T : DependencyObject
    {
        ToolTipService.SetToolTip(element, new ToolTip { Content = content });
        return element;
    }

    public static Button IconButton(string glyph, string tip, Action onClick)
    {
        var b = new Button
        {
            Content = new FontIcon { Glyph = glyph, FontFamily = IconFont, FontSize = 14 },
            Padding = new Thickness(8, 6, 8, 6),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
        };
        b.Click += (_, _) => onClick();
        ToolTipService.SetToolTip(b, tip);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(b, tip);
        return b;
    }

    public static HyperlinkButton Link(string text, Action onClick)
    {
        var b = new HyperlinkButton { Content = text, Padding = new Thickness(4, 2, 4, 2), FontSize = 12 };
        b.Click += (_, _) => onClick();
        return b;
    }

    /// <summary>A color swatch for legends.</summary>
    public static Border Swatch(Color c, double size = 10, double radius = 2) => new()
    {
        Width = size,
        Height = size,
        CornerRadius = new CornerRadius(radius),
        Background = Theme.Brush(c),
        VerticalAlignment = VerticalAlignment.Center,
    };

    public static Microsoft.UI.Xaml.Shapes.Ellipse Dot(Color c, double size = 8) => new()
    {
        Width = size,
        Height = size,
        Fill = Theme.Brush(c),
        VerticalAlignment = VerticalAlignment.Center,
    };
}

/// <summary>Lays children out left to right, wrapping to new lines (legends, model chips).</summary>
sealed class WrapPanel : Panel
{
    public double HorizontalSpacing { get; set; } = 8;
    public double VerticalSpacing { get; set; } = 4;

    protected override Size MeasureOverride(Size available)
    {
        double x = 0, y = 0, line = 0, width = 0;
        foreach (var child in Children)
        {
            child.Measure(new Size(available.Width, double.PositiveInfinity));
            var s = child.DesiredSize;
            if (x > 0 && x + s.Width > available.Width)
            {
                y += line + VerticalSpacing;
                x = 0;
                line = 0;
            }
            x += s.Width + HorizontalSpacing;
            line = Math.Max(line, s.Height);
            width = Math.Max(width, x - HorizontalSpacing);
        }
        return new Size(double.IsInfinity(available.Width) ? width : Math.Min(width, available.Width), y + line);
    }

    protected override Size ArrangeOverride(Size final)
    {
        double x = 0, y = 0, line = 0;
        foreach (var child in Children)
        {
            var s = child.DesiredSize;
            if (x > 0 && x + s.Width > final.Width)
            {
                y += line + VerticalSpacing;
                x = 0;
                line = 0;
            }
            child.Arrange(new Rect(x, y, s.Width, s.Height));
            x += s.Width + HorizontalSpacing;
            line = Math.Max(line, s.Height);
        }
        return final;
    }
}

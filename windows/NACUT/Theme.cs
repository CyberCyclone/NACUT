using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using uniffi.nacut;
using Windows.UI;
using Windows.UI.ViewManagement;

namespace Nacut;

/// <summary>
/// The same palette as the Mac app (apple/App/Views/Theme.swift): coral accent, a validated categorical
/// palette in fixed order, a sequential coral ramp and status colors, each with light and dark steps
/// (the values live in NACUT.Logic's <see cref="Logic.Palette"/>; this picks the step for the system theme).
/// Fluent text and fill tones are defined here too, so views can be rebuilt when the system theme changes.
/// </summary>
static class Theme
{
    static readonly UISettings ui = new();

    /// <summary>True when Windows apps use dark mode.</summary>
    public static bool IsDark
    {
        get
        {
            var bg = ui.GetColorValue(UIColorType.Background);
            return bg.R + bg.G + bg.B < 384;
        }
    }

    public static ElementTheme ElementTheme => IsDark ? ElementTheme.Dark : ElementTheme.Light;

    /// <summary>Raised (on a background thread) when the system theme or accent changes.</summary>
    public static event Action? Changed;
    static Theme() => ui.ColorValuesChanged += (_, _) => Changed?.Invoke();

    static Color Hex(uint rgb, byte a = 255)
    {
        var (alpha, r, g, b) = Logic.Palette.Argb(rgb, a);
        return Color.FromArgb(alpha, r, g, b);
    }

    static Color Pick(Logic.ColorPair pair) => Hex(pair.For(IsDark));
    public static SolidColorBrush Brush(Color c) => new(c);
    public static SolidColorBrush Brush(Color c, double opacity) => new(c) { Opacity = opacity };

    // MARK: Fluent tones (WinUI's TextFillColor*/CardBackground* values)

    public static Color TextPrimary => IsDark ? Hex(0xFFFFFF) : Hex(0x000000, 0xE4);
    public static Color TextSecondary => IsDark ? Hex(0xFFFFFF, 0xC5) : Hex(0x000000, 0x9E);
    public static Color TextTertiary => IsDark ? Hex(0xFFFFFF, 0x87) : Hex(0x000000, 0x72);
    public static Color CardFill => IsDark ? Hex(0xFFFFFF, 0x0D) : Hex(0xFFFFFF, 0xB3);
    public static Color CardStroke => IsDark ? Hex(0x000000, 0x19) : Hex(0x000000, 0x0F);
    public static Color SubtleFill => IsDark ? Hex(0xFFFFFF, 0x0F) : Hex(0x000000, 0x09);
    public static Color TrackFill => IsDark ? Hex(0xFFFFFF, 0x1A) : Hex(0x000000, 0x12);
    public static Color Divider => IsDark ? Hex(0xFFFFFF, 0x15) : Hex(0x000000, 0x0F);
    /// <summary>Opaque panel background for screenshots (no acrylic in RenderTargetBitmap).</summary>
    public static Color SolidBackground => IsDark ? Hex(0x202020) : Hex(0xF3F3F3);

    // MARK: Brand and data colors

    public static Color Coral => Pick(Logic.Palette.Coral);
    public static Color CoralSoft => Pick(Logic.Palette.CoralSoft);

    public static Color Categorical(int slot) => Pick(Logic.Palette.Categorical[slot]);
    public static Color Other => Pick(Logic.Palette.Other);

    /// <summary>A model's color from the core's fixed model → slot registry, so it matches every view and app.</summary>
    public static Color ForModel(string id) => Pick(Logic.Palette.ForSlot(NacutMethods.ModelSlot(id)));
    public static int SlotOrder(string id) => (int)NacutMethods.ModelSlotOrder(id);

    public static int RampCount => Logic.Palette.Ramp.Count;
    public static Color RampStep(int i) => Pick(Logic.Palette.Ramp[i]);

    /// <summary>Sequential coral ramp for magnitude (calendar, heatmap), light → dark.</summary>
    public static Color Ramp(double fraction) => Logic.Palette.RampIndex(fraction) is { } i ? RampStep(i) : Colors.Transparent;

    /// <summary>Ink that stays legible on a ramp step.</summary>
    public static Color InkOnRamp(double fraction) => Logic.Palette.LightInkOnRamp(fraction) ? Colors.White : TextPrimary;

    public static Color Status(BudgetStatus s) => Pick(Logic.Palette.Status(s.Kind()));

    /// <summary>Segoe Fluent Icons glyph for a status.</summary>
    public static string StatusGlyph(BudgetStatus s) => Logic.Palette.StatusGlyph(s.Kind());

    static Logic.StatusKind Kind(this BudgetStatus s) => s switch
    {
        BudgetStatus.NoBudget => Logic.StatusKind.NoBudget,
        BudgetStatus.UnderBudget => Logic.StatusKind.UnderBudget,
        BudgetStatus.Tight => Logic.StatusKind.Tight,
        BudgetStatus.WillExceed => Logic.StatusKind.WillExceed,
        _ => Logic.StatusKind.Exceeded,
    };

    public static Color ForLevel(Level l) => l switch
    {
        Level.Normal => Coral,
        Level.Warning => Status(BudgetStatus.Tight),
        _ => Status(BudgetStatus.Exceeded),
    };

    public static Color ForLimit(double u) => Logic.Palette.ForLimit(u) switch
    {
        Logic.LimitLevel.Critical => Status(BudgetStatus.Exceeded),
        Logic.LimitLevel.Warning => Status(BudgetStatus.Tight),
        _ => Coral,
    };
}

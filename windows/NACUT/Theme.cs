using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using uniffi.nacut;
using Windows.UI;
using Windows.UI.ViewManagement;

namespace Nacut;

/// <summary>
/// The same palette as the Mac app (apple/App/Views/Theme.swift): coral accent, a validated categorical
/// palette in fixed order, a sequential coral ramp and status colors, each with light and dark steps.
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

    static Color Hex(uint rgb, byte a = 255) => Color.FromArgb(a, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
    static Color Pick(uint light, uint dark) => Hex(IsDark ? dark : light);
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

    public static Color Coral => Pick(0xD97757, 0xE58262);
    public static Color CoralSoft => Pick(0xF5C9B8, 0x5C3426);

    static readonly (uint light, uint dark)[] categorical =
    [
        (0x2A78D6, 0x3987E5), // blue
        (0xEB6834, 0xD95926), // orange
        (0x1BAF7A, 0x199E70), // aqua
        (0xEDA100, 0xC98500), // yellow
        (0xE87BA4, 0xD55181), // magenta
        (0x008300, 0x0CA30C), // green
        (0x6250D6, 0x9085E9), // violet
        (0xE34948, 0xE66767), // red
    ];

    public static Color Categorical(int slot) => Pick(categorical[slot].light, categorical[slot].dark);
    public static Color Other => Pick(0x9A9890, 0x6E6D66);

    /// <summary>Fixed model → slot registry so a model keeps its color across every view and filter.</summary>
    static readonly Dictionary<string, int> modelSlots = new()
    {
        ["claude-opus-5-5"] = 0,
        ["claude-fable-5-2"] = 1,
        ["claude-sonnet-5-5"] = 2,
        ["claude-haiku-5-5"] = 3,
        ["claude-fable-5-1"] = 4,
        ["claude-opus-5"] = 5,
        ["claude-sonnet-5"] = 6,
        ["claude-opus-4-8"] = 7,
    };

    public static Color ForModel(string id) => modelSlots.TryGetValue(id, out var s) ? Categorical(s) : Other;
    public static int SlotOrder(string id) => modelSlots.TryGetValue(id, out var s) ? s : 99;

    static readonly (uint light, uint dark)[] ramp =
    [
        (0xFBE9E2, 0x3A2620),
        (0xF5C9B8, 0x5C3426),
        (0xEDA58B, 0x8A4730),
        (0xE0805F, 0xBD5F3F),
        (0xC25E3D, 0xE58262),
    ];

    public static int RampCount => ramp.Length;
    public static Color RampStep(int i) => Pick(ramp[i].light, ramp[i].dark);

    /// <summary>Sequential coral ramp for magnitude (calendar, heatmap), light → dark.</summary>
    public static Color Ramp(double fraction)
    {
        if (fraction <= 0) return Colors.Transparent;
        var i = Math.Min((int)Math.Ceiling(fraction * ramp.Length) - 1, ramp.Length - 1);
        return RampStep(Math.Max(i, 0));
    }

    /// <summary>Ink that stays legible on a ramp step.</summary>
    public static Color InkOnRamp(double fraction) => fraction > 0.6 ? Colors.White : TextPrimary;

    public static Color Status(BudgetStatus s) => s switch
    {
        BudgetStatus.NoBudget or BudgetStatus.UnderBudget => Pick(0x0B8A4A, 0x2FBF71),
        BudgetStatus.Tight => Pick(0xB86E00, 0xF0A23B),
        BudgetStatus.WillExceed => Pick(0xC2410C, 0xF97316),
        _ => Pick(0xC0262D, 0xF05252),
    };

    /// <summary>Segoe Fluent Icons glyph for a status.</summary>
    public static string StatusGlyph(BudgetStatus s) => s switch
    {
        BudgetStatus.NoBudget or BudgetStatus.UnderBudget => "", // Completed
        BudgetStatus.Tight => "",                                 // Info
        BudgetStatus.WillExceed => "",                            // Warning
        _ => "",                                                  // ErrorBadge
    };

    public static Color ForLevel(Level l) => l switch
    {
        Level.Normal => Coral,
        Level.Warning => Status(BudgetStatus.Tight),
        _ => Status(BudgetStatus.Exceeded),
    };

    public static Color ForLimit(double u) =>
        u >= 0.9 ? Status(BudgetStatus.Exceeded) : u >= 0.7 ? Status(BudgetStatus.Tight) : Coral;
}

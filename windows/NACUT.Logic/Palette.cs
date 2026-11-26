namespace Nacut.Logic;

/// <summary>A color as 0xRRGGBB for light mode and for dark mode.</summary>
public readonly record struct ColorPair(uint Light, uint Dark)
{
    public uint For(bool dark) => dark ? Dark : Light;
}

/// <summary>Plan-limit severity: ≥ 90% critical, ≥ 70% warning (the core's `Level::for_utilization`).</summary>
public enum LimitLevel { Normal, Warning, Critical }

/// <summary>The core's budget statuses (`BudgetStatus`), which the app maps to by name.</summary>
public enum StatusKind { NoBudget, UnderBudget, Tight, WillExceed, Exceeded }

/// <summary>
/// The palette as numbers, the same as the Mac and Linux apps (apple/App/Views/Theme.swift, linux/src/theme.rs):
/// coral accent, a categorical palette in fixed order, a sequential coral ramp and status colors, each with light
/// and dark steps. The app's Theme turns these into WinUI colors for the current system theme.
/// </summary>
public static class Palette
{
    /// <summary>ARGB bytes of 0xRRGGBB with alpha `a`.</summary>
    public static (byte A, byte R, byte G, byte B) Argb(uint rgb, byte a = 255) => (a, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);

    public static readonly ColorPair Coral = new(0xD97757, 0xE58262);
    public static readonly ColorPair CoralSoft = new(0xF5C9B8, 0x5C3426);
    /// <summary>Models outside the slot registry, and "Other" parts.</summary>
    public static readonly ColorPair Other = new(0x9A9890, 0x6E6D66);

    /// <summary>Categorical slots 0–7; the core's model registry (`model_slot`) picks a model's slot.</summary>
    public static readonly IReadOnlyList<ColorPair> Categorical =
    [
        new(0x2A78D6, 0x3987E5), // blue
        new(0xEB6834, 0xD95926), // orange
        new(0x1BAF7A, 0x199E70), // aqua
        new(0xEDA100, 0xC98500), // yellow
        new(0xE87BA4, 0xD55181), // magenta
        new(0x008300, 0x0CA30C), // green
        new(0x6250D6, 0x9085E9), // violet
        new(0xE34948, 0xE66767), // red
    ];

    /// <summary>A model's color: its categorical slot, or <see cref="Other"/> for no slot or one this palette lacks.</summary>
    public static ColorPair ForSlot(uint? slot) => slot is { } s && s < Categorical.Count ? Categorical[(int)s] : Other;

    /// <summary>Sequential coral ramp for magnitude (calendar, heatmap), light → dark.</summary>
    public static readonly IReadOnlyList<ColorPair> Ramp =
    [
        new(0xFBE9E2, 0x3A2620),
        new(0xF5C9B8, 0x5C3426),
        new(0xEDA58B, 0x8A4730),
        new(0xE0805F, 0xBD5F3F),
        new(0xC25E3D, 0xE58262),
    ];

    /// <summary>
    /// Ramp step for a fraction of the scale: equal fifths, anything past the top in the darkest step. Null (no
    /// fill) for zero, negative or NaN.
    /// </summary>
    public static int? RampIndex(double fraction)
    {
        if (double.IsNaN(fraction) || fraction <= 0) return null;
        if (fraction >= 1) return Ramp.Count - 1;
        return Math.Clamp((int)Math.Ceiling(fraction * Ramp.Count) - 1, 0, Ramp.Count - 1);
    }

    /// <summary>Whether text on a ramp step needs light ink to stay legible.</summary>
    public static bool LightInkOnRamp(double fraction) => fraction > 0.6;

    public static LimitLevel ForLimit(double utilization) =>
        utilization >= 0.9 ? LimitLevel.Critical : utilization >= 0.7 ? LimitLevel.Warning : LimitLevel.Normal;

    public static ColorPair Status(StatusKind s) => s switch
    {
        StatusKind.NoBudget or StatusKind.UnderBudget => new(0x0B8A4A, 0x2FBF71),
        StatusKind.Tight => new(0xB86E00, 0xF0A23B),
        StatusKind.WillExceed => new(0xC2410C, 0xF97316),
        _ => new(0xC0262D, 0xF05252),
    };

    /// <summary>Segoe Fluent Icons glyph for a status.</summary>
    public static string StatusGlyph(StatusKind s) => s switch
    {
        StatusKind.NoBudget or StatusKind.UnderBudget => "", // Completed
        StatusKind.Tight => "",                               // Info
        StatusKind.WillExceed => "",                          // Warning
        _ => "",                                              // ErrorBadge
    };
}

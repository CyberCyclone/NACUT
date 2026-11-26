using System.Globalization;

namespace Nacut.Logic;

/// <summary>Short UI strings. Callers pass the clock and culture, so results never depend on the machine.</summary>
public static class Formats
{
    /// <summary>Session length: `&lt;1m`, `42m`, `3h 5m`, `2d 4h` (days from 48 hours).</summary>
    public static string Duration(double secs)
    {
        if (!double.IsFinite(secs) || secs < 60) return "<1m";
        var s = (int)Math.Min(secs, int.MaxValue);
        int h = s / 3600, m = s % 3600 / 60;
        if (h >= 48) return $"{h / 24}d {h % 24}h";
        return h > 0 ? $"{h}h {m}m" : $"{m}m";
    }

    /// <summary>`just now`, `5 min ago`, `3 hr ago`, `yesterday`, `12 days ago`, then the date (`MMM d` in `culture`).</summary>
    public static string Relative(DateTime t, DateTime now, CultureInfo culture)
    {
        var d = now - t;
        if (d.TotalMinutes < 1) return "just now";
        if (d.TotalHours < 1) return $"{(int)d.TotalMinutes} min ago";
        if (d.TotalDays < 1) return $"{(int)d.TotalHours} hr ago";
        if (d.TotalDays < 2) return "yesterday";
        if (d.TotalDays < 30) return $"{(int)d.TotalDays} days ago";
        return t.ToString("MMM d", culture);
    }

    /// <summary>The panel footer's freshness: `Updated just now`, `Updated 5m ago`, or `Loading…` before the first scan.</summary>
    public static string Updated(DateTime? updated, DateTime now)
    {
        if (updated is not { } t) return "Loading…";
        var age = now - t;
        return age.TotalSeconds < 60 ? "Updated just now" : $"Updated {(int)age.TotalMinutes}m ago";
    }

    /// <summary>Axis hour label, lower case: `12 am`, `6 pm` (en-US).</summary>
    public static string HourLabel(int hour, CultureInfo culture) =>
        new DateTime(2000, 1, 1).AddHours(hour).ToString("h tt", culture).ToLowerInvariant();

    /// <summary>Price per million tokens: up to 3 decimals below $1, else up to 2 (`$0.08`, `$3.75`, `$15`).</summary>
    public static string Price(double v, CultureInfo culture) =>
        "$" + v.ToString(v < 1 ? "0.###" : "0.##", culture);

    /// <summary>A cleared number box reads NaN: keep the previous value then.</summary>
    public static double KeepFinite(double value, double previous) => double.IsFinite(value) ? value : previous;

    /// <summary>Cuts `s` to at most `max` characters, ending in `…` when cut (tray tooltips allow 127).</summary>
    public static string Truncate(string s, int max)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(max, 1);
        return s.Length <= max ? s : s[..(max - 1)] + "…";
    }

    /// <summary>Console Admin keys (`sk-ant-admin…`) get a warning before they're stored.</summary>
    public static bool IsAdminKey(string key) => key.Trim().StartsWith("sk-ant-admin", StringComparison.Ordinal);

    /// <summary>Limit names: the core's `seven_day` window reads "Weekly"; others keep their label.</summary>
    public static string LimitLabel(string id, string label) => id == "seven_day" ? "Weekly" : label;
}

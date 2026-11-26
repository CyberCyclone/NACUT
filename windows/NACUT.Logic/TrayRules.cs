using System.Globalization;

namespace Nacut.Logic;

/// <summary>What the tray icon draws: a percentage over a gauge, or "$" when there's no gauge.</summary>
/// <param name="Text">`42`, `7`, `!` (100% or more) or `$`.</param>
/// <param name="FontScale">Font size as a fraction of the icon size.</param>
/// <param name="HasGauge">Whether a gauge bar is drawn under the text.</param>
public readonly record struct TrayLabel(string Text, float FontScale, bool HasGauge);

public static class TrayRules
{
    /// <summary>The label for the core's gauge fraction (worst plan limit or budget used), or "$" without one.</summary>
    public static TrayLabel Label(double? gaugeFraction)
    {
        if (gaugeFraction is not { } f) return new("$", 0.8f, false);
        // NaN reads as 0 and anything below 0 as 0%, so the icon never shows "-5" or "NaN".
        var pct = double.IsNaN(f) ? 0 : (int)Math.Round(Math.Clamp(f, 0, 1) * 100);
        var text = pct >= 100 ? "!" : pct.ToString(CultureInfo.InvariantCulture);
        return new(text, text.Length >= 2 ? 0.62f : 0.72f, true);
    }

    /// <summary>Height of the gauge bar for an icon `size` pixels square.</summary>
    public static float BarHeight(int size) => Math.Max(2f, size / 8f);

    /// <summary>Width of the gauge's filled part: never less than its height, so even 0% shows a dot.</summary>
    public static float GaugeWidth(double fraction, int size)
    {
        var f = double.IsNaN(fraction) ? 0 : Math.Clamp(fraction, 0, 1);
        return Math.Max(BarHeight(size), (size - 1) * (float)f);
    }

    /// <summary>
    /// Whether Windows keeps the icon in the ^ overflow. Explorer records each tray icon under
    /// HKCU\Control Panel\NotifyIconSettings with its exe path (packaged apps' paths start with a known-folder
    /// GUID, so only the folder and file name are compared) and sets IsPromoted = 1 (a DWORD) once the user pins
    /// it to the taskbar. The first entry for this exe decides; no entry means Windows hasn't placed it yet.
    /// </summary>
    public static bool IsInOverflow(string? exePath, IEnumerable<(string? ExecutablePath, object? IsPromoted)> entries)
    {
        if (string.IsNullOrEmpty(exePath)) return false;
        var tail = PathTail(exePath);
        foreach (var (path, promoted) in entries)
        {
            if (path is not null && EndsWithSegments(path, tail))
                return promoted is not int p || p == 0;
        }
        return false;
    }

    /// <summary>
    /// Whether `path` ends with `tail` starting at a folder boundary, so `Tools\NOTNACUT\NACUT.exe` doesn't count as
    /// `NACUT\NACUT.exe`.
    /// </summary>
    static bool EndsWithSegments(string path, string tail)
    {
        if (!path.EndsWith(tail, StringComparison.OrdinalIgnoreCase)) return false;
        var before = path.Length - tail.Length;
        return before == 0 || path[before - 1] is '\\' or '/';
    }

    /// <summary>
    /// `folder\file.exe` of a Windows path, like Path.Combine(GetFileName(GetDirectoryName(p)), GetFileName(p)) on
    /// Windows, but the same on every OS (both separators count) so tests can run anywhere.
    /// </summary>
    static string PathTail(string path)
    {
        var parts = path.Split('\\', '/');
        var file = parts[^1];
        // A drive root ("C:") or an empty segment isn't a folder name.
        var folder = parts.Length >= 2 ? parts[^2] : "";
        if (folder.Length == 0 || (parts.Length == 2 && folder.EndsWith(':'))) return file;
        return folder + "\\" + file;
    }
}

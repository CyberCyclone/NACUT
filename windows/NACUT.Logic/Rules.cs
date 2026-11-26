namespace Nacut.Logic;

/// <summary>Command-line switches (`--render-to DIR`, `--background`, …).</summary>
public static class CommandLine
{
    /// <summary>The value after `name`, or null when it's missing or last.</summary>
    public static string? Arg(string[] args, string name)
    {
        var i = Array.IndexOf(args, name);
        return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
    }

    public static bool Flag(string[] args, string name) => args.Contains(name);
}

/// <summary>The stats ranges, mirroring the core's `StatRange` (the app maps between the two by name).</summary>
public enum RangeChoice { Cycle, Last30, AllTime }

/// <summary>Small settings and preference rules.</summary>
public static class Rules
{
    /// <summary>A range remembered in ui.json by name; anything else (missing, edited, unknown) is the cycle.</summary>
    public static RangeChoice ParseRange(string? stored) =>
        Enum.TryParse<RangeChoice>(stored, out var r) && Enum.IsDefined(r) ? r : RangeChoice.Cycle;

    /// <summary>Showing plan limits in the tray turns limit fetching on; showing cost leaves it as it was.</summary>
    public static bool LimitsEnabledAfterDisplayChange(bool limitsEnabled, bool displayIsLimits) =>
        limitsEnabled || displayIsLimits;
}

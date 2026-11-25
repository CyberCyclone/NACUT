using System.Text.Json;
using Microsoft.UI.Dispatching;
using uniffi.nacut;

namespace Nacut;

/// <summary>
/// The app's view of the shared Rust core (<see cref="NacutEngine"/>): republishes its snapshots on the
/// UI thread and forwards settings and actions. Scanning, plan limits, the API, pricing, forecasts and
/// settings storage (%APPDATA%\NACUT\settings.json) all live in the core.
/// </summary>
sealed class AppModel
{
    public NacutEngine Engine { get; }
    readonly DispatcherQueue dispatcher;
    readonly Bridge bridge;
    bool reloadScheduled;

    public Snapshot Snap { get; private set; }
    /// <summary>Raised on the UI thread after every snapshot change.</summary>
    public event Action? Changed;
    /// <summary>Raised on the UI thread when the core decides an alert should show.</summary>
    public event Action<Alert>? AlertRaised;

    public AppModel(DispatcherQueue dispatcher, bool renderMode)
    {
        this.dispatcher = dispatcher;
        Engine = NacutEngine.Standard();
        Snap = Engine.Snapshot();
        bridge = new Bridge(this);
        Engine.SetObserver(bridge);
        if (renderMode)
        {
            // Screenshots: read the logs once; no background work, network or credential access.
            Engine.ScanNow();
            Snap = Engine.Snapshot();
            return;
        }
        Engine.Start();
    }

    void ScheduleReload()
    {
        if (reloadScheduled) return;
        reloadScheduled = true;
        dispatcher.TryEnqueue(async () =>
        {
            await Task.Delay(50);
            reloadScheduled = false;
            Reload();
        });
    }

    public void Reload()
    {
        Snap = Engine.Snapshot();
        Changed?.Invoke();
    }

    // MARK: State

    public Settings Settings => Snap.settings;
    public StatusView Status => Snap.status;
    public ForecastView Forecast => Snap.forecast;
    public InsightsView Insights => Snap.insights;
    public ApiView Api => Snap.api;
    public bool IsSubscription => Settings.accountMode == AccountMode.Subscription;
    public List<LimitView> Limits => Snap.status.limits;
    public bool ShowsLimits => Snap.status.showsLimits;

    /// <summary>Daily series keyed by local date.</summary>
    public Dictionary<DateOnly, DayEntry> Series =>
        Snap.series.GroupBy(d => d.date.ToDateOnly()).ToDictionary(g => g.Key, g => g.First());

    // MARK: Queries

    public List<ModelView> ModelStats(StatRange r) => Engine.ModelStats(r);
    public List<ProjectView> ProjectStats(StatRange r) => Engine.ProjectStats(r);
    public List<SessionView> Sessions(StatRange r, int limit) => Engine.Sessions(r, (uint)limit);
    public List<NamedValue> EntrypointStats(StatRange r) => Engine.EntrypointStats(r);
    public AgentSplit AgentSplit(StatRange r) => Engine.AgentSplit(r);
    public List<NamedValue> TokenClasses(StatRange r) => Engine.TokenClasses(r);
    public double CacheSavings(StatRange r) => Engine.CacheSavings(r);
    public List<List<double>> Heatmap(StatRange r) => Engine.WeekdayHourHeatmap(r);
    public List<HourCost> Hourly(DateOnly d) => Engine.Hourly(d.ToKey());
    public List<ModelView> DayModels(DateOnly d) => Engine.DayModels(d.ToKey());
    public List<SessionView> DaySessions(DateOnly d, int limit) => Engine.DaySessions(d.ToKey(), (uint)limit);
    public List<DayEntry> LastDays(int count) => Engine.LastDays((uint)count);
    public string DisplayName(string model) => Engine.DisplayName(model);

    // MARK: Actions

    public void Update(Func<Settings, Settings> change)
    {
        Engine.SetSettings(change(Engine.Settings()));
        Reload();
    }

    public void SetUsageDisplay(UsageDisplay display)
    {
        Update(s => s with { usageDisplay = display, limitsEnabled = s.limitsEnabled || display == UsageDisplay.Limits });
    }

    public void RefreshAll()
    {
        Engine.Refresh();
        if (Settings.limitsEnabled) Engine.RefreshLimits();
        if (Settings.dataSource != DataSourceMode.Logs) Engine.RefreshApi(false);
    }

    public async Task<ApiResult> SaveApiKeyAsync(string key)
    {
        var result = await Task.Run(() => Engine.SaveApiKey(key));
        Reload();
        return result;
    }

    public async Task TestApiAsync()
    {
        await Task.Run(() => Engine.TestApi());
        Reload();
    }

    public async Task LoadActorsAsync()
    {
        await Task.Run(() => Engine.LoadActors());
        Reload();
    }

    /// <summary>Receives core callbacks on background threads and hops to the UI thread.</summary>
    sealed class Bridge(AppModel model) : NacutObserver
    {
        public void OnChange() => model.ScheduleReload();
        public void OnAlert(Alert alert) => model.dispatcher.TryEnqueue(() => model.AlertRaised?.Invoke(alert));
    }
}

static class CoreExtensions
{
    public static DateOnly ToDateOnly(this DayKey k) => new(k.year, k.month, k.day);
    public static DayKey ToKey(this DateOnly d) => new(d.Year, (byte)d.Month, (byte)d.Day);
    public static DateTime Local(this long ms) => DateTimeOffset.FromUnixTimeMilliseconds(ms).LocalDateTime;
    public static DateOnly Today => DateOnly.FromDateTime(DateTime.Now);

    public static string Usd(this double v) => NacutMethods.FormatUsd(v, false);
    public static string UsdCompact(this double v) => v switch
    {
        0 => "$0",
        < 10 => NacutMethods.FormatUsd(v, false),
        < 1000 => "$" + Math.Round(v).ToString("0"),
        _ => "$" + (v / 1000).ToString("0.0") + "k",
    };
    public static string UsdExact(this double v) => NacutMethods.FormatUsdExact(v);
    public static string Pct(this double v) => NacutMethods.FormatPercent(v);
    public static string Tokens(this ulong n) => NacutMethods.FormatTokens(n);

    public static string Title(this AccountMode m) => m == AccountMode.Subscription ? "Max / Pro subscription" : "API pay-as-you-go";
    public static string Title(this UsageDisplay d) => d == UsageDisplay.Limits ? "Plan limits" : "API value";
    public static string Title(this SecondFigure f) => f switch
    {
        SecondFigure.Cycle => "This cycle",
        SecondFigure.AllTime => "All time",
        _ => "Hidden",
    };
    public static string Title(this DataSourceMode m) => m switch
    {
        DataSourceMode.Logs => "Local logs",
        DataSourceMode.Api => "Anthropic API only",
        _ => "Hybrid (API + live logs)",
    };
    public static string Title(this StatRange r) => r switch
    {
        StatRange.Cycle => "This cycle",
        StatRange.Last30 => "30 days",
        _ => "All time",
    };
}

/// <summary>Small per-user UI preferences (last tab, ranges) in %LOCALAPPDATA%\NACUT\ui.json.</summary>
static class Prefs
{
    static readonly string Path = System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NACUT", "ui.json");
    static Dictionary<string, string>? values;

    static Dictionary<string, string> Values
    {
        get
        {
            if (values is not null) return values;
            try { values = JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(Path)); }
            catch { }
            return values ??= [];
        }
    }

    public static string Get(string key, string fallback) => Values.TryGetValue(key, out var v) ? v : fallback;

    public static void Set(string key, string value)
    {
        Values[key] = value;
        try
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
            File.WriteAllText(Path, JsonSerializer.Serialize(Values));
        }
        catch (Exception e) { Log.Write($"Prefs save failed: {e.Message}"); }
    }

    public static StatRange Range(string key) =>
        Enum.TryParse<StatRange>(Get(key, nameof(StatRange.Cycle)), out var r) ? r : StatRange.Cycle;
}

static class Log
{
    static readonly string Path = System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NACUT", "nacut.log");

    public static void Write(string message)
    {
        try
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
            File.AppendAllText(Path, $"{DateTime.Now:yyyy-MM-dd HH:mm:ss} {message}{Environment.NewLine}");
        }
        catch { }
    }
}

using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.Windows.Widgets.Providers;
using uniffi.nacut;
using WinRT;

namespace Nacut;

/// <summary>
/// Windows 11 Widgets board provider (MSIX builds only): small, medium and large Adaptive Cards
/// showing plan limits or cost, like the Mac widget. The board activates it through COM
/// (see Package.appxmanifest); the running tray app registers the class, so cards update live.
/// </summary>
[ComVisible(true)]
[Guid(ClassId)]
public sealed class WidgetProvider : IWidgetProvider
{
    public const string ClassId = "0680C996-81BF-4C02-B604-E3E659D8A8E3";

    static readonly Dictionary<string, string> active = []; // widget id → size
    static readonly object gate = new();
    static bool subscribed;

    public WidgetProvider()
    {
        lock (gate)
        {
            if (subscribed) return;
            subscribed = true;
        }
        // Pick up widgets that were pinned before this process started.
        foreach (var info in WidgetManager.GetDefault().GetWidgetInfos())
        {
            lock (gate) active[info.WidgetContext.Id] = info.WidgetContext.Size.ToString().ToLowerInvariant();
        }
        App.Current.Dispatcher.TryEnqueue(() => App.Current.Model.Changed += UpdateAll);
        UpdateAll();
    }

    public void CreateWidget(WidgetContext context)
    {
        lock (gate) active[context.Id] = context.Size.ToString().ToLowerInvariant();
        Update(context.Id);
    }

    public void DeleteWidget(string widgetId, string customState)
    {
        lock (gate) active.Remove(widgetId);
    }

    public void OnActionInvoked(WidgetActionInvokedArgs args)
    {
        if (args.Verb == "open") App.Current.Dispatcher.TryEnqueue(App.Current.ShowPanel);
        else if (args.Verb == "refresh") App.Current.Dispatcher.TryEnqueue(App.Current.Model.RefreshAll);
    }

    public void OnWidgetContextChanged(WidgetContextChangedArgs args)
    {
        lock (gate) active[args.WidgetContext.Id] = args.WidgetContext.Size.ToString().ToLowerInvariant();
        Update(args.WidgetContext.Id);
    }

    public void Activate(WidgetContext context) => Update(context.Id);
    public void Deactivate(string widgetId) { }

    static void UpdateAll()
    {
        string[] ids;
        lock (gate) ids = [.. active.Keys];
        foreach (var id in ids) Update(id);
    }

    static void Update(string id)
    {
        string size;
        lock (gate) if (!active.TryGetValue(id, out size!)) return;
        App.Current.Dispatcher.TryEnqueue(() =>
        {
            try
            {
                var (template, data) = Card(App.Current.Model, size);
                WidgetManager.GetDefault().UpdateWidget(new WidgetUpdateRequestOptions(id) { Template = template, Data = data });
            }
            catch (Exception e)
            {
                Log.Write($"Widget update failed: {e.Message}");
            }
        });
    }

    // MARK: Cards

    /// <summary>The Adaptive Card template and its data for one widget size.</summary>
    static (string template, string data) Card(AppModel m, string size)
    {
        var st = m.Status;
        var f = m.Forecast;
        var limits = st.limits.Take(size == "small" ? 2 : 3).Select(l => new JsonObject
        {
            ["label"] = l.id == "seven_day" ? "Weekly" : l.label,
            ["percent"] = l.utilization.Pct(),
            // Column weights for the bar (Adaptive Cards has no progress element on the Widgets board).
            ["fill"] = Math.Max((int)Math.Round(Math.Clamp(l.utilization, 0, 1) * 100), 1),
            ["rest"] = Math.Max(100 - (int)Math.Round(Math.Clamp(l.utilization, 0, 1) * 100), 1),
            ["detail"] = l.subtitle,
            ["color"] = l.utilization >= 0.9 ? "attention" : l.utilization >= 0.7 ? "warning" : "accent", // container styles
        }).ToList();
        var showLimits = st.showsLimits && limits.Count > 0;
        var days = m.LastDays(14);
        var data = new JsonObject
        {
            ["showLimits"] = showLimits,
            ["plan"] = m.Snap.planName,
            ["limits"] = new JsonArray([.. limits]),
            ["verdict"] = st.limitsVerdict?.text ?? "",
            ["cycleLabel"] = f.cycleLabel,
            ["spent"] = f.spent.Usd(),
            ["today"] = f.today.Usd(),
            ["projected"] = f.projected.Usd(),
            ["headline"] = f.headline,
            ["apiValue"] = $"API value {f.spent.Usd()} this cycle · {f.today.Usd()} today",
            ["week"] = new JsonArray([.. days.TakeLast(7).Select(d => (JsonNode)new JsonObject
            {
                ["day"] = d.startMs.Local().ToString("ddd"),
                ["cost"] = d.cost.UsdCompact(),
            })]),
            ["updated"] = st.lastUpdatedMs is { } ms ? $"Updated {ms.Local():h:mm tt}" : "Loading…",
        };
        return (Template(size), data.ToJsonString());
    }

    static string Template(string size)
    {
        var limitRow = new JsonObject
        {
            ["type"] = "Container",
            ["$data"] = "${limits}",
            ["spacing"] = "Small",
            ["items"] = new JsonArray(
                new JsonObject
                {
                    ["type"] = "ColumnSet",
                    ["columns"] = new JsonArray(
                        Column("stretch", Text("${label}", weight: "Bolder", size: "Small")),
                        Column("auto", Text("${percent}", weight: "Bolder", size: "Small"))),
                },
                new JsonObject
                {
                    ["type"] = "ColumnSet",
                    ["spacing"] = "Small",
                    ["columns"] = new JsonArray(
                        new JsonObject { ["type"] = "Column", ["width"] = "${fill}", ["style"] = "${color}", ["minHeight"] = "6px", ["items"] = new JsonArray() },
                        new JsonObject { ["type"] = "Column", ["width"] = "${rest}", ["style"] = "emphasis", ["minHeight"] = "6px", ["spacing"] = "None", ["items"] = new JsonArray() }),
                },
                Text("${detail}", size: "Small", subtle: true, spacing: "None")),
        };

        var limitsBody = new JsonArray(
            Text("Plan usage · ${plan}", size: "Small", subtle: true, weight: "Bolder"),
            limitRow);
        if (size != "small")
        {
            limitsBody.Add(Text("${verdict}", size: "Small", wrap: true));
            limitsBody.Add(Text("${apiValue}", size: "Small", subtle: true, wrap: true));
        }

        var costBody = new JsonArray(
            Text("${cycleLabel}", size: "Small", subtle: true, weight: "Bolder"),
            Text("${spent}", size: "ExtraLarge", weight: "Bolder", spacing: "None"),
            Text("Today ${today}", size: "Small"));
        if (size != "small")
        {
            costBody.Add(Text("${headline}", size: "Small", wrap: true));
            costBody.Add(new JsonObject
            {
                ["type"] = "ColumnSet",
                ["spacing"] = "Medium",
                ["columns"] = new JsonArray(new JsonObject
                {
                    ["type"] = "Column",
                    ["$data"] = "${week}",
                    ["width"] = "stretch",
                    ["items"] = new JsonArray(
                        Text("${day}", size: "Small", subtle: true, align: "Center"),
                        Text("${cost}", size: "Small", weight: "Bolder", align: "Center", spacing: "None")),
                }),
            });
        }

        var body = new JsonArray(
            new JsonObject { ["type"] = "Container", ["$when"] = "${showLimits}", ["items"] = limitsBody },
            new JsonObject { ["type"] = "Container", ["$when"] = "${!showLimits}", ["items"] = costBody });
        if (size == "large") body.Add(Text("${updated}", size: "Small", subtle: true, spacing: "Medium"));

        var card = new JsonObject
        {
            ["type"] = "AdaptiveCard",
            ["$schema"] = "http://adaptivecards.io/schemas/adaptive-card.json",
            ["version"] = "1.6",
            ["body"] = body,
            ["actions"] = size == "small" ? new JsonArray() : new JsonArray(
                new JsonObject { ["type"] = "Action.Execute", ["title"] = "Open NACUT", ["verb"] = "open" },
                new JsonObject { ["type"] = "Action.Execute", ["title"] = "Refresh", ["verb"] = "refresh" }),
        };
        if (size == "small") card["selectAction"] = new JsonObject { ["type"] = "Action.Execute", ["verb"] = "open" };
        return card.ToJsonString();
    }

    static JsonObject Text(string text, string? size = null, string? weight = null, bool subtle = false, bool wrap = false,
        string? spacing = null, string? align = null)
    {
        var o = new JsonObject { ["type"] = "TextBlock", ["text"] = text };
        if (size is not null) o["size"] = size;
        if (weight is not null) o["weight"] = weight;
        if (subtle) o["isSubtle"] = true;
        if (wrap) o["wrap"] = true;
        if (spacing is not null) o["spacing"] = spacing;
        if (align is not null) o["horizontalAlignment"] = align;
        return o;
    }

    static JsonObject Column(string width, JsonObject item) =>
        new() { ["type"] = "Column", ["width"] = width, ["items"] = new JsonArray(item) };

    // MARK: COM registration

    [ComImport, Guid("00000001-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IClassFactory
    {
        [PreserveSig] int CreateInstance(IntPtr outer, ref Guid riid, out IntPtr result);
        [PreserveSig] int LockServer(bool lockServer);
    }

    sealed class Factory : IClassFactory
    {
        public int CreateInstance(IntPtr outer, ref Guid riid, out IntPtr result)
        {
            result = IntPtr.Zero;
            if (outer != IntPtr.Zero) return unchecked((int)0x80040110); // CLASS_E_NOAGGREGATION
            if (riid != typeof(WidgetProvider).GUID && riid != new Guid("00000000-0000-0000-C000-000000000046"))
                return unchecked((int)0x80004002); // E_NOINTERFACE
            result = MarshalInspectable<IWidgetProvider>.FromManaged(new WidgetProvider());
            return 0;
        }

        public int LockServer(bool lockServer) => 0;
    }

    static uint cookie;

    /// <summary>Lets the Widgets board reach this process (packaged builds only).</summary>
    public static void Register()
    {
        var clsid = typeof(WidgetProvider).GUID;
        var hr = CoRegisterClassObject(ref clsid, new Factory(), 0x4 /* CLSCTX_LOCAL_SERVER */, 0x1 /* REGCLS_MULTIPLEUSE */, out cookie);
        if (hr != 0) Log.Write($"Widget provider registration failed: 0x{hr:X8}");
    }

    public static void Unregister()
    {
        if (cookie != 0) CoRevokeClassObject(cookie);
    }

    [DllImport("ole32.dll")]
    static extern int CoRegisterClassObject(ref Guid clsid, [MarshalAs(UnmanagedType.IUnknown)] object factory, uint context, uint flags, out uint cookie);

    [DllImport("ole32.dll")]
    static extern int CoRevokeClassObject(uint cookie);
}

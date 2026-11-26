using System.Runtime.InteropServices;
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

    /// <summary>The Adaptive Card template and its data for one widget size (laid out by <see cref="Logic.WidgetCard"/>).</summary>
    static (string template, string data) Card(AppModel m, string size)
    {
        var st = m.Status;
        var f = m.Forecast;
        var input = new Logic.WidgetInput(
            ShowsLimits: st.showsLimits,
            Plan: m.Snap.planName,
            Limits: [.. st.limits.Select(l => new Logic.WidgetLimit(l.id, l.label, l.utilization, l.utilization.Pct(), l.subtitle))],
            Verdict: st.limitsVerdict?.text ?? "",
            CycleLabel: f.cycleLabel,
            Spent: f.spent.Usd(),
            Today: f.today.Usd(),
            Projected: f.projected.Usd(),
            Headline: f.headline,
            Days: [.. m.LastDays(14).Select(d => new Logic.WidgetDay(d.startMs.Local().ToString("ddd"), d.cost.UsdCompact()))],
            Updated: st.lastUpdatedMs is { } ms ? $"Updated {ms.Local():h:mm tt}" : "Loading…");
        return (Logic.WidgetCard.Template(size), Logic.WidgetCard.Data(input, size));
    }

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

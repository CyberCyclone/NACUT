using System.Runtime.InteropServices;
using CommunityToolkit.WinUI.Controls;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using uniffi.nacut;
using Windows.Graphics;

namespace Nacut.Views;

/// <summary>
/// The tray flyout: a borderless, always-on-top acrylic window placed by the taskbar corner nearest the
/// pointer. It hides when it loses focus, like the Mac's menu bar panel (PanelView.swift).
/// </summary>
sealed class PanelWindow : Window
{
    public const double PanelWidth = 452;
    const double MaxHeight = 780;
    const double Pad = 16;
    /// <summary>Width available to a chart inside a card (panel − padding − card padding/border).</summary>
    public const double ChartWidth = PanelWidth - 2 * Pad - 26;

    static readonly string[] TabNames = ["Overview", "Calendar", "Models", "Projects"];

    readonly AppModel m;
    readonly Grid root;
    readonly ContentControl headerHost = new() { HorizontalContentAlignment = HorizontalAlignment.Stretch };
    readonly ContentControl tabHost = new() { HorizontalContentAlignment = HorizontalAlignment.Stretch };
    readonly ScrollViewer scroll;
    readonly Segmented tabs = new() { HorizontalAlignment = HorizontalAlignment.Stretch };
    readonly TextBlock statusText = new() { FontSize = 12, VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis };
    readonly Ellipse statusDot = new() { Width = 6, Height = 6, VerticalAlignment = VerticalAlignment.Center };
    readonly Grid loading;
    readonly InfoBar trayTip;
    readonly DispatcherTimer clock = new() { Interval = TimeSpan.FromSeconds(10) };
    readonly IntPtr hwnd;
    bool visible;
    DateTime lastHidden;
    int selectedTab;
    bool tabsReady;
    /// <summary>Set once the panel has had focus since it was shown; only then does losing focus hide it.
    /// (Windows can refuse focus, e.g. when launched by Task Scheduler, and the panel would vanish at once.)</summary>
    bool hadFocus;

    public int TabIndex => selectedTab;
    public FrameworkElement Root => root;
    public FrameworkElement TabContent => (FrameworkElement)tabHost.Content;

    public PanelWindow(AppModel model)
    {
        m = model;
        Title = "NACUT";
        hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);

        foreach (var name in TabNames) tabs.Items.Add(new SegmentedItem { Content = name });
        selectedTab = Math.Max(Array.IndexOf(TabNames, Prefs.Get("panelTab", "Overview")), 0);
        // Segmented re-applies its first selection while it loads, so the panel owns the tab and only
        // listens to the control once it has loaded.
        tabs.Loaded += (_, _) =>
        {
            tabs.SelectedIndex = selectedTab;
            DispatcherQueue.TryEnqueue(() => tabsReady = true);
        };
        tabs.SelectionChanged += (_, _) =>
        {
            if (!tabsReady)
            {
                if (tabs.SelectedIndex != selectedTab) DispatcherQueue.TryEnqueue(() => tabs.SelectedIndex = selectedTab);
                return;
            }
            if (tabs.SelectedIndex < 0 || tabs.SelectedIndex == selectedTab) return;
            selectedTab = tabs.SelectedIndex;
            Prefs.Set("panelTab", TabNames[selectedTab]);
            RebuildTab(resetScroll: true);
        };

        scroll = new ScrollViewer
        {
            Content = tabHost,
            Padding = new Thickness(Pad, 0, Pad, 14),
            HorizontalScrollMode = ScrollMode.Disabled,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
        };

        var footer = new Grid { Padding = new Thickness(12, 6, 8, 6), ColumnSpacing = 2 };
        footer.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (var i = 0; i < 3; i++) footer.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        footer.Children.Add(new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, Children = { statusDot, statusText } });
        AddFooterButton(footer, 1, Ui.IconButton("", "Refresh now", m.RefreshAll));
        AddFooterButton(footer, 2, Ui.IconButton("", "Settings", () => App.Current.ShowSettings()));
        AddFooterButton(footer, 3, Ui.IconButton("", "Quit NACUT", () => App.Current.Quit()));

        loading = new Grid
        {
            Background = Theme.Brush(Theme.SolidBackground, 0.92),
            Children =
            {
                new StackPanel
                {
                    Spacing = 10, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center,
                    Children = { new ProgressRing { IsActive = true, Width = 28, Height = 28 }, Ui.Secondary("Reading your Claude Code history…", 13) },
                },
            },
        };

        // Windows 11 puts new tray icons in the ^ overflow and apps can't promote themselves, so say where it went.
        trayTip = new InfoBar
        {
            Severity = InfoBarSeverity.Informational,
            Title = "NACUT is under ^ in the taskbar",
            Message = "Drag its icon next to the clock to keep your usage in view.",
            IsOpen = false,
            Margin = new Thickness(Pad, 0, Pad, 8),
        };
        trayTip.CloseButtonClick += (_, _) => Prefs.Set("trayTipDismissed", "1");

        root = new Grid { RequestedTheme = Theme.ElementTheme };
        // header, tabs, scrolling tab content, tray tip, divider, footer
        foreach (var star in new[] { false, false, true, false, false, false })
            root.RowDefinitions.Add(new RowDefinition { Height = star ? new GridLength(1, GridUnitType.Star) : GridLength.Auto });
        headerHost.Margin = new Thickness(Pad, Pad, Pad, 12);
        tabs.Margin = new Thickness(Pad, 0, Pad, 10);
        AddRow(headerHost, 0);
        AddRow(tabs, 1);
        AddRow(scroll, 2);
        var divider = new Rectangle { Height = 1, Fill = Theme.Brush(Theme.Divider) };
        AddRow(trayTip, 3);
        AddRow(divider, 4);
        AddRow(footer, 5);
        Grid.SetRowSpan(loading, 6);
        root.Children.Add(loading);
        root.KeyDown += (_, e) =>
        {
            if (e.Key == Windows.System.VirtualKey.Escape) HidePanel();
        };
        Content = root;

        SystemBackdrop = new DesktopAcrylicBackdrop();
        var presenter = OverlappedPresenter.Create();
        presenter.IsResizable = false;
        presenter.IsMaximizable = false;
        presenter.IsMinimizable = false;
        presenter.IsAlwaysOnTop = true;
        presenter.SetBorderAndTitleBar(true, false);
        AppWindow.SetPresenter(presenter);
        AppWindow.IsShownInSwitchers = false;
        AppWindow.SetIcon(System.IO.Path.Combine(AppContext.BaseDirectory, "Assets", "nacut.ico"));
        AppWindow.Closing += (_, e) => { e.Cancel = true; HidePanel(); };

        Activated += (_, e) =>
        {
            // Activate() reports "activated" even when Windows refuses focus, so check for real.
            if (e.WindowActivationState != WindowActivationState.Deactivated) hadFocus |= GetForegroundWindow() == hwnd;
            else if (visible && hadFocus && DebugRenderer.OutputDir is null) HidePanel();
        };
        m.Changed += () => { if (visible) Refresh(); };
        Theme.Changed += () => DispatcherQueue.TryEnqueue(() =>
        {
            root.RequestedTheme = Theme.ElementTheme;
            if (visible) Refresh();
        });
        clock.Tick += (_, _) => UpdateFooter();
    }

    void AddRow(FrameworkElement e, int row)
    {
        Grid.SetRow(e, row);
        root.Children.Add(e);
    }

    static void AddFooterButton(Grid footer, int column, Button b)
    {
        Grid.SetColumn(b, column);
        footer.Children.Add(b);
    }

    // MARK: Showing and hiding

    public void Toggle()
    {
        if (visible) HidePanel();
        // The click that took focus away already hid the panel; don't reopen it.
        else if ((DateTime.Now - lastHidden).TotalMilliseconds > 350) ShowNearTray();
    }

    public void ShowNearTray()
    {
        Refresh();
        trayTip.IsOpen = Prefs.Get("trayTipDismissed", "") != "1" && TrayIcon.IsInOverflow();
        PlaceNearTray();
        visible = true;
        hadFocus = false;
        AppWindow.Show(true);
        Activate();
        SetForegroundWindow(hwnd);
        clock.Start();
    }

    public void HidePanel()
    {
        if (!visible) return;
        visible = false;
        lastHidden = DateTime.Now;
        clock.Stop();
        AppWindow.Hide();
    }

    /// <summary>Opens the panel at a fixed spot for screenshots, with an opaque background.</summary>
    public void ShowForRender(int tab)
    {
        root.Background = Theme.Brush(Theme.SolidBackground);
        selectedTab = tab;
        tabs.SelectedIndex = tab;
        visible = true;
        Refresh();
        GetCursorPos(out var cursor);
        var scale = MonitorScale(cursor);
        AppWindow.MoveAndResize(new RectInt32(40, 40, (int)(PanelWidth * scale), (int)(MaxHeight * scale)));
        AppWindow.Show(true);
        Activate();
    }

    void PlaceNearTray()
    {
        GetCursorPos(out var cursor);
        var display = DisplayArea.GetFromPoint(new PointInt32(cursor.X, cursor.Y), DisplayAreaFallback.Nearest);
        var work = display.WorkArea;
        var outer = display.OuterBounds;
        var r = Logic.PanelPlacement.Place(
            new Logic.PixelRect(work.X, work.Y, work.Width, work.Height),
            new Logic.PixelRect(outer.X, outer.Y, outer.Width, outer.Height),
            new Logic.PixelPoint(cursor.X, cursor.Y),
            MonitorScale(cursor), PanelWidth, MaxHeight);
        AppWindow.MoveAndResize(new RectInt32(r.X, r.Y, r.Width, r.Height));
    }

    // MARK: Content

    public void SelectTab(int index) { selectedTab = index; tabs.SelectedIndex = index; RebuildTab(resetScroll: true); }

    public void Refresh()
    {
        try
        {
            headerHost.Content = Header.Build(m);
            RebuildTab(resetScroll: false);
        }
        catch (Exception e)
        {
            Log.Write($"Panel build failed: {e}");
        }
        loading.Visibility = !m.Status.hasLoaded && m.Snap.recordCount == 0 ? Visibility.Visible : Visibility.Collapsed;
        UpdateFooter();
    }

    void RebuildTab(bool resetScroll)
    {
        var offset = scroll.VerticalOffset;
        var w = ChartWidth;
        tabHost.Content = selectedTab switch
        {
            1 => CalendarTab.Build(m, w, () => RebuildTab(false)),
            2 => ModelsTab.Build(m, w, () => RebuildTab(false)),
            3 => ProjectsTab.Build(m, w, () => RebuildTab(false)),
            _ => OverviewTab.Build(m, w),
        };
        if (resetScroll) scroll.ChangeView(null, 0, null, true);
        else DispatcherQueue.TryEnqueue(() => scroll.ChangeView(null, offset, null, true));
    }

    void UpdateFooter()
    {
        var st = m.Status;
        var busy = st.isScanning || m.Api.isFetching;
        statusDot.Fill = Theme.Brush(busy ? Theme.Coral : Theme.Status(BudgetStatus.UnderBudget));
        statusText.Foreground = Theme.Brush(Theme.TextSecondary);
        var updated = Logic.Formats.Updated(st.lastUpdatedMs?.Local(), DateTime.Now);
        var source = m.Settings.dataSource switch
        {
            DataSourceMode.Logs => "local logs",
            DataSourceMode.Api => "Anthropic API",
            _ => "API + logs",
        };
        statusText.Text = $"{updated} · {source}";
    }

    // MARK: Win32

    [StructLayout(LayoutKind.Sequential)]
    struct POINT { public int X, Y; }

    [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT p);
    /// <summary>Scale of the monitor under a point (a hidden window's own DPI isn't reliable).</summary>
    static double MonitorScale(POINT p)
    {
        var monitor = MonitorFromPoint(p, 2 /* MONITOR_DEFAULTTONEAREST */);
        return GetDpiForMonitor(monitor, 0 /* MDT_EFFECTIVE_DPI */, out var dpi, out _) == 0 ? dpi / 96.0 : 1;
    }

    [DllImport("user32.dll")] static extern IntPtr MonitorFromPoint(POINT p, uint flags);
    [DllImport("shcore.dll")] static extern int GetDpiForMonitor(IntPtr monitor, int type, out uint dpiX, out uint dpiY);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
}

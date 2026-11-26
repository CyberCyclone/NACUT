using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.nacut;
using Windows.Graphics;

namespace Nacut.Views;

/// <summary>
/// Settings with the same five sections as the Mac (SettingsView.swift), laid out with Windows 11
/// settings cards. Pages are built on navigation; status lines refresh when the core's data changes.
/// </summary>
sealed class SettingsWindow : Window
{
    static readonly (string tag, string title, string glyph)[] Sections =
    [
        ("general", "General", "\uE713"),
        ("budget", "Budget", "\uE8C7"),
        ("sources", "Data Sources", "\uE968"),
        ("alerts", "Alerts", "\uEA8F"),
        ("pricing", "Pricing", "\uE8EC"),
    ];

    readonly AppModel m;
    readonly NavigationView nav;
    readonly ScrollViewer page = new() { HorizontalScrollMode = ScrollMode.Disabled };
    readonly List<Action> live = [];
    readonly IntPtr hwnd;
    string currentTag = "general";
    bool closed;
    /// <summary>The API's actor list is fetched at most once per key while this window is open.</summary>
    bool actorsRequested;
    /// <summary>An Admin key was connected in this window (the key itself is never kept).</summary>
    bool adminKeySaved;

    public SettingsWindow(AppModel model)
    {
        m = model;
        Title = "NACUT Settings";
        hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        SystemBackdrop = new MicaBackdrop();
        ExtendsContentIntoTitleBar = true;

        var titleBar = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 10,
            Height = 40,
            Padding = new Thickness(14, 0, 0, 0),
            Children =
            {
                new Image { Width = 16, Height = 16, Source = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage(new Uri(System.IO.Path.Combine(AppContext.BaseDirectory, "Assets", "nacut-256.png"))) },
                new TextBlock { Text = "NACUT Settings", FontSize = 12, VerticalAlignment = VerticalAlignment.Center },
            },
        };
        SetTitleBar(titleBar);

        nav = new NavigationView
        {
            PaneDisplayMode = NavigationViewPaneDisplayMode.Left,
            IsSettingsVisible = false,
            IsBackButtonVisible = NavigationViewBackButtonVisible.Collapsed,
            IsPaneToggleButtonVisible = false,
            OpenPaneLength = 220,
            Content = page,
        };
        foreach (var (tag, title, glyph) in Sections)
            nav.MenuItems.Add(new NavigationViewItem { Content = title, Tag = tag, Icon = new FontIcon { Glyph = glyph, FontFamily = Ui.IconFont } });
        nav.SelectionChanged += (_, e) =>
        {
            if (e.SelectedItem is NavigationViewItem { Tag: string tag }) Show(tag);
        };

        var root = new Grid { RequestedTheme = Theme.ElementTheme };
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.Children.Add(titleBar);
        Grid.SetRow(nav, 1);
        root.Children.Add(nav);
        Content = root;

        var scale = GetDpiForSystem() / 96.0; // the window has no DPI of its own until shown
        AppWindow.Resize(new SizeInt32((int)(920 * scale), (int)(700 * scale)));
        AppWindow.SetIcon(System.IO.Path.Combine(AppContext.BaseDirectory, "Assets", "nacut.ico"));

        // Theme.Changed is static, so the handler is removed on close or closed windows would leak and keep rebuilding.
        Action themeChanged = () => DispatcherQueue.TryEnqueue(() =>
        {
            if (closed) return;
            root.RequestedTheme = Theme.ElementTheme;
            Show(currentTag);
        });
        m.Changed += OnChanged;
        Theme.Changed += themeChanged;
        Closed += (_, _) =>
        {
            closed = true;
            m.Changed -= OnChanged;
            Theme.Changed -= themeChanged;
        };
    }

    void OnChanged()
    {
        foreach (var update in live) update();
    }

    public void ShowSection(string? tag)
    {
        var target = tag ?? currentTag;
        nav.SelectedItem = nav.MenuItems.OfType<NavigationViewItem>().FirstOrDefault(i => (string)i.Tag == target);
        Show(target);
        Activate();
    }

    void Show(string tag)
    {
        currentTag = tag;
        live.Clear();
        var content = tag switch
        {
            "budget" => Budget(),
            "sources" => DataSources(),
            "alerts" => Alerts(),
            "pricing" => Pricing(),
            _ => General(),
        };
        content.MaxWidth = 820;
        content.Padding = new Thickness(36, 8, 36, 36);
        content.HorizontalAlignment = HorizontalAlignment.Stretch;
        page.Content = content;
    }

    /// <summary>Rebuilds the current page (after a change that alters its structure).</summary>
    void Rebuild() => Show(currentTag);

    // MARK: Building blocks

    static StackPanel Page(string title, params UIElement?[] children)
    {
        var p = new StackPanel { Spacing = 4 };
        p.Children.Add(new TextBlock { Text = title, FontSize = 28, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, 12, 0, 16) });
        foreach (var c in children) if (c is not null) p.Children.Add(c);
        return p;
    }

    static TextBlock Section(string title) => new()
    {
        Text = title,
        FontWeight = FontWeights.SemiBold,
        FontSize = 14,
        Margin = new Thickness(1, 22, 0, 6),
    };

    static SettingsCard Card(string header, string? description, object? content, string? glyph = null) => new()
    {
        Header = header,
        Description = description,
        Content = content,
        HeaderIcon = glyph is null ? null : new FontIcon { Glyph = glyph, FontFamily = Ui.IconFont },
    };

    static TextBlock Note(string text) => new()
    {
        Text = text,
        TextWrapping = TextWrapping.Wrap,
        FontSize = 12,
        Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        Margin = new Thickness(2, 6, 2, 0),
    };

    /// <summary>A text block whose content is recomputed whenever the core's data changes.</summary>
    TextBlock Live(Func<string> text, double size = 13)
    {
        var t = new TextBlock { Text = text(), FontSize = size, TextWrapping = TextWrapping.Wrap, VerticalAlignment = VerticalAlignment.Center };
        live.Add(() => t.Text = text());
        return t;
    }

    static ToggleSwitch Toggle(bool on, Action<bool> changed)
    {
        var t = new ToggleSwitch { IsOn = on, OnContent = "On", OffContent = "Off" };
        t.Toggled += (_, _) => changed(t.IsOn);
        return t;
    }

    static ComboBox Combo<T>(IReadOnlyList<T> values, T selected, Func<T, string> title, Action<T> changed, double width = 220)
    {
        var c = new ComboBox { MinWidth = width };
        foreach (var v in values) c.Items.Add(title(v));
        c.SelectedIndex = Math.Max(values.ToList().IndexOf(selected), 0);
        c.SelectionChanged += (_, _) =>
        {
            if (c.SelectedIndex >= 0 && !Equals(values[c.SelectedIndex], selected)) changed(values[c.SelectedIndex]);
        };
        return c;
    }

    static Button Btn(string text, Action click, bool accent = false)
    {
        var b = new Button { Content = text };
        if (accent) b.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
        b.Click += (_, _) => click();
        return b;
    }

    // MARK: General

    StackPanel General()
    {
        var s = m.Settings;
        var p = Page("General");

        if (m.IsSubscription)
        {
            p.Children.Add(Card("Show usage as",
                s.usageDisplay == UsageDisplay.Limits
                    ? "The tray icon, panel header and tooltip show your 5-hour and weekly plan limits. What the same usage would cost at API prices is still in the panel."
                    : "The tray icon, panel header and tooltip show what your usage would cost at API prices. On a subscription you don't pay this; it shows how much value you get from your plan.",
                Combo([UsageDisplay.Limits, UsageDisplay.Cost], s.usageDisplay, d => d.Title(), d => { m.SetUsageDisplay(d); Rebuild(); }),
                "\uE9D9"));
            if (s.usageDisplay == UsageDisplay.Limits && m.Status.limitsError is { } err && m.Limits.Count == 0)
            {
                p.Children.Add(new InfoBar
                {
                    IsOpen = true, IsClosable = false, Severity = InfoBarSeverity.Warning,
                    Message = $"Plan limits unavailable ({err}), so the tray shows cost for now.",
                });
            }
        }

        p.Children.Add(Section("Tray"));
        var second = Combo([SecondFigure.Cycle, SecondFigure.AllTime, SecondFigure.None], s.secondFigure, f => f.Title(),
            f => m.Update(x => x with { secondFigure = f }));
        second.IsEnabled = !m.ShowsLimits;
        p.Children.Add(Card("Second figure", "Shown after today's cost in the tooltip and tray menu", second));
        p.Children.Add(Card(m.ShowsLimits ? "Compact (5-hour limit only)" : "Compact (today only)", null,
            Toggle(s.compactMenuBar, on => m.Update(x => x with { compactMenuBar = on }))));
        var preview = Live(() => m.Status.menuBarText);
        preview.FontFamily = new FontFamily("Cascadia Mono,Consolas");
        p.Children.Add(Card("Preview", null, new Border
        {
            Child = preview,
            Padding = new Thickness(10, 4, 10, 5),
            CornerRadius = new CornerRadius(5),
            Background = (Brush)Application.Current.Resources["ControlFillColorSecondaryBrush"],
        }));

        p.Children.Add(Section("Startup"));
        // Enabling can fail or be refused (policy, Task Manager), so the switch shows what Windows actually did.
        ToggleSwitch login = null!;
        var syncing = false;
        login = Toggle(Startup.Enabled, on =>
        {
            if (syncing) return;
            Startup.Enabled = on;
            var actual = Startup.Enabled;
            if (actual == on) return;
            syncing = true;
            login.IsOn = actual;
            syncing = false;
        });
        p.Children.Add(Card("Open at login", "Start NACUT in the tray when you sign in to Windows", login, "\uE7E8"));

        p.Children.Add(Section("About"));
        p.Children.Add(Card("Messages tracked", null, Live(() => m.Snap.recordCount.ToString("N0"))));
        p.Children.Add(Card("History since", null, Live(() => m.Snap.firstRecordMs is { } ms ? ms.Local().ToString("MMM d, yyyy") : "—")));
        p.Children.Add(Card("Core", null, new TextBlock { Text = $"nacut-core {NacutMethods.CoreVersion()}" }));
        p.Children.Add(Note("Costs are computed from Claude Code transcripts (desktop app Code tab, VS Code, and CLI) at Anthropic list prices. Desktop-app chats aren't in those logs; turn on plan limits or an API connection in Data Sources to cover them."));
        p.Children.Add(Note("NACUT is an independent project and is not affiliated with, endorsed by, or sponsored by Anthropic. MIT License © 2026 Casey Gibson."));
        return p;
    }

    // MARK: Budget

    StackPanel Budget()
    {
        var s = m.Settings;
        var p = Page("Budget");
        p.Children.Add(Section("Account"));

        var modes = new RadioButtons();
        foreach (var mode in new[] { AccountMode.Subscription, AccountMode.Api }) modes.Items.Add(mode.Title());
        modes.SelectedIndex = s.accountMode == AccountMode.Subscription ? 0 : 1;
        modes.SelectionChanged += (_, _) =>
        {
            var mode = modes.SelectedIndex == 0 ? AccountMode.Subscription : AccountMode.Api;
            if (mode == m.Settings.accountMode) return;
            m.Update(x => x with { accountMode = mode });
            Rebuild();
        };
        p.Children.Add(Card("This PC's account",
            s.accountMode == AccountMode.Subscription
                ? "Costs show what your usage would cost at API prices, so you can see the value you get from your plan."
                : "Costs are what you pay per token. The budget forecast tells you if you'll go over.",
            modes, "\uE77B"));

        if (s.accountMode == AccountMode.Subscription)
        {
            var plans = new List<(double price, string title)> { (20, "Pro · $20/mo"), (100, "Max 5x · $100/mo"), (200, "Max 20x · $200/mo") };
            if (!plans.Any(x => x.price == s.planPrice)) plans.Add((s.planPrice, $"Custom · {s.planPrice.Usd()}"));
            p.Children.Add(Card("Plan", null, Combo(plans, plans.First(x => x.price == s.planPrice), x => x.title,
                x => m.Update(y => y with { planPrice = x.price }))));
        }

        p.Children.Add(Section("Budget"));
        var budget = new NumberBox
        {
            Value = s.monthlyBudget, Minimum = 0, SmallChange = 10, LargeChange = 100, MinWidth = 160,
            SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact, PlaceholderText = "0",
        };
        budget.ValueChanged += (_, e) =>
        {
            if (!double.IsNaN(e.NewValue) && e.NewValue != m.Settings.monthlyBudget) m.Update(x => x with { monthlyBudget = Math.Max(e.NewValue, 0) });
        };
        p.Children.Add(Card("Monthly budget (USD)",
            "Set to $0 for no budget." + (s.accountMode == AccountMode.Subscription ? " On a subscription this is a soft target for API-equivalent value." : ""),
            budget, "\uE825"));

        var day = new NumberBox
        {
            Value = s.cycleStartDay, Minimum = 1, Maximum = 28, SmallChange = 1, MinWidth = 120,
            SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Inline,
        };
        day.ValueChanged += (_, e) =>
        {
            if (!double.IsNaN(e.NewValue) && (byte)e.NewValue != m.Settings.cycleStartDay) m.Update(x => x with { cycleStartDay = (byte)Math.Clamp(e.NewValue, 1, 28) });
        };
        var cycleCard = Card("Cycle starts on day", null, day, "\uE787");
        cycleCard.Description = Live(() =>
        {
            var f = m.Forecast;
            return $"Use your billing or renewal day if it isn't the 1st. Current cycle: {f.cycleStartMs.Local():MMM d} – {f.cycleEndMs.Local().AddDays(-1):MMM d}.";
        }, 12);
        p.Children.Add(cycleCard);

        p.Children.Add(Section("Forecast right now"));
        var forecastHost = new ContentControl { HorizontalContentAlignment = HorizontalAlignment.Stretch, Content = Header.ForecastLine(m.Forecast) };
        live.Add(() => forecastHost.Content = Header.ForecastLine(m.Forecast));
        p.Children.Add(forecastHost);
        p.Children.Add(Card("Spent this cycle", null, Live(() => m.Forecast.spent.Usd())));
        p.Children.Add(Card("Projected", null, Live(() => m.Forecast.projected.Usd())));
        p.Children.Add(Card("Safe daily spend", "Spend at most this much per day for the rest of the cycle to stay within budget",
            Live(() => m.Forecast.safeDailySpend is { } v ? v.Usd() : "—")));
        return p;
    }

    // MARK: Data sources

    StackPanel DataSources()
    {
        var s = m.Settings;
        var p = Page("Data Sources");

        p.Children.Add(Section("Subscription limits"));
        p.Children.Add(Card("Show Max/Pro plan limits",
            "Shows your 5-hour and weekly limit usage, including desktop-app chats. Uses the Claude Code sign-in (~/.claude/.credentials.json) and the same unofficial endpoint as Claude Code's /usage, so it may stop working if Anthropic changes it.",
            Toggle(s.limitsEnabled, on => { m.Update(x => x with { limitsEnabled = on }); Rebuild(); }), "\uE9D9"));
        if (s.limitsEnabled)
        {
            var status = Live(() => m.Status.limitsError is { } e ? $"⚠ {e}"
                : m.Status.limitsUpdatedMs is { } at ? $"✓ Updated {ProjectsTab.Relative(at.Local())}" : "Loading…", 12);
            status.Margin = new Thickness(4, 2, 0, 0);
            p.Children.Add(status);
            p.Children.Add(Card("Refresh sign-in automatically",
                "When the Claude Code sign-in expires (about every 8 hours), run `claude` with a one-word Haiku prompt so it refreshes itself. Off: run `claude` yourself when limits pause.",
                Toggle(s.autoRefreshSignIn, on => m.Update(x => x with { autoRefreshSignIn = on }))));
        }

        p.Children.Add(Section("Anthropic Usage & Cost API"));
        if (m.Api.hasKey)
        {
            var apiStatus = Live(() => (m.Api.usable ? "✓ " : "⚠ ") + m.Api.message);
            p.Children.Add(Card("Connection", null, apiStatus, "\uE968"));
            p.Children.Add(Card("Use", null, Combo([DataSourceMode.Logs, DataSourceMode.Hybrid, DataSourceMode.Api], s.dataSource, d => d.Title(),
                d => m.Update(x => x with { dataSource = d }), 260)));

            // The people arrive after the page is built (LoadActorsOnceAsync), so the picker is rebuilt when they change.
            SettingsCard ActorPicker()
            {
                var actors = m.Api.actors;
                var current = m.Settings;
                var options = new List<(string? id, string title)> { (null, "Whole organization") };
                options.AddRange(actors.Select(a => ((string?)a.id, $"{a.display} · {a.cost.UsdCompact()} last 30d")));
                if (current.apiActorId is { } id && actors.All(a => a.id != id)) options.Add((id, current.apiActorName ?? id));
                return Card("Count usage from", "On a shared work org, pick yourself so the budget tracks only your usage.",
                    Combo(options, options.First(o => o.id == current.apiActorId), o => o.title,
                        o => { m.Engine.SetActor(actors.FirstOrDefault(a => a.id == o.id)); m.Reload(); }, 260));
            }
            var actorHost = new ContentControl { HorizontalContentAlignment = HorizontalAlignment.Stretch, Content = ActorPicker() };
            var shownActors = m.Api.actors.Select(a => a.id).ToList();
            live.Add(() =>
            {
                if (m.Api.actors.Select(a => a.id).SequenceEqual(shownActors)) return;
                shownActors = m.Api.actors.Select(a => a.id).ToList();
                actorHost.Content = ActorPicker();
            });
            p.Children.Add(actorHost);

            var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            buttons.Children.Add(Btn("Test connection", async () => await m.TestApiAsync()));
            buttons.Children.Add(Btn("Refresh now", () => m.Engine.RefreshApi(true)));
            buttons.Children.Add(Btn("Remove key", () =>
            {
                m.Engine.RemoveApiKey();
                actorsRequested = false;
                adminKeySaved = false;
                m.Reload();
                Rebuild();
            }));
            p.Children.Add(Card("Actions", null, buttons));
            if (adminKeySaved) p.Children.Add(AdminKeyWarning(true));
            if (m.Api.actors.Count == 0) _ = LoadActorsOnceAsync();
        }
        else
        {
            var key = new PasswordBox { PlaceholderText = "read:analytics or sk-ant-admin… key", MinWidth = 300 };
            var adminWarning = AdminKeyWarning(false);
            key.PasswordChanged += (_, _) => adminWarning.IsOpen = IsAdminKey(key.Password);
            var progress = new ProgressRing { IsActive = false, Width = 18, Height = 18 };
            var result = new TextBlock { TextWrapping = TextWrapping.Wrap, FontSize = 12, Margin = new Thickness(4, 2, 0, 0) };
            if (m.Api.state != ApiState.NotConfigured) result.Text = $"⚠ {m.Api.message}";
            var connect = Btn("Connect", () => { }, accent: true);
            async void Connect()
            {
                var k = key.Password.Trim();
                if (k.Length == 0 || !connect.IsEnabled) return;
                connect.IsEnabled = false;
                progress.IsActive = true;
                var admin = IsAdminKey(k);
                var r = await m.SaveApiKeyAsync(k);
                progress.IsActive = false;
                connect.IsEnabled = true;
                key.Password = "";
                if (r.usable)
                {
                    adminKeySaved = admin;
                    actorsRequested = false;
                    Rebuild();
                }
                else result.Text = $"⚠ {r.message}";
            }
            connect.Click += (_, _) => Connect();
            key.KeyDown += (_, e) => { if (e.Key == Windows.System.VirtualKey.Enter) Connect(); };
            p.Children.Add(Card("API key",
                "Use a Claude Enterprise key scoped to read:analytics if you can. A Console Admin key (sk-ant-admin…) also works, but it can manage your whole organization, so only store one on a computer you trust.",
                new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Children = { key, connect, progress } }, "\uE8D7"));
            p.Children.Add(adminWarning);
            p.Children.Add(result);
        }
        p.Children.Add(Note("Optional. Pulls exact billed cost from Anthropic's Usage & Cost API. Console orgs: an Admin API key. Claude Enterprise: a key with read:analytics created by your org's primary owner. If your org doesn't allow it, the test tells you and NACUT keeps using local logs. The key is stored only in Windows Credential Manager. API data lags 4–24 h, so Hybrid fills recent hours from local logs, scaled to match your billed rate."));

        p.Children.Add(Section("Local transcript folders"));
        foreach (var path in NacutMethods.DefaultLogRoots())
        {
            p.Children.Add(Card(path, Directory.Exists(path) ? "Default" : "Default · not found", null, Directory.Exists(path) ? "\uE8B7" : "\uE9CE"));
        }
        p.Children.Add(Card("Include Claude Code in WSL", "Also read ~/.claude/projects inside running WSL distributions",
            Toggle(s.includeWsl, on => m.Update(x => x with { includeWsl = on })), "\uE756"));
        foreach (var path in s.extraRoots)
        {
            var remove = Btn("Remove", () => { m.Update(x => x with { extraRoots = x.extraRoots.Where(r => r != path).ToList() }); Rebuild(); });
            p.Children.Add(Card(path, "Added folder", remove, "\uE8B7"));
        }
        var input = new TextBox { PlaceholderText = @"C:\other-claude\projects", MinWidth = 280 };
        var add = Btn("Add", () =>
        {
            var v = input.Text.Trim();
            if (v.Length == 0) return;
            m.Update(x => x with { extraRoots = [.. x.extraRoots, v] });
            Rebuild();
        });
        var browse = Btn("Browse…", async () =>
        {
            var picker = new Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(picker, hwnd);
            if (await picker.PickSingleFolderAsync() is { } folder) input.Text = folder.Path;
        });
        p.Children.Add(Card("Add folder", null, new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Children = { input, browse, add } }));
        var cacheCard = Card("Cache", "History is cached, so totals survive Claude Code's automatic transcript cleanup.",
            Btn("Rebuild cache", () => m.Engine.RebuildCache()), "\uE81C");
        cacheCard.Header = Live(() => $"{m.Snap.recordCount:N0} messages cached", 14);
        p.Children.Add(cacheCard);
        return p;
    }

    static bool IsAdminKey(string key) => key.Trim().StartsWith("sk-ant-admin", StringComparison.Ordinal);

    static InfoBar AdminKeyWarning(bool saved) => new()
    {
        IsOpen = saved, IsClosable = false, Severity = InfoBarSeverity.Warning,
        Title = saved ? "Connected with an Admin key" : "This is an Admin key",
        Message = "A Console Admin key can manage your whole organization, so only store one on a computer you trust. A Claude Enterprise key scoped to read:analytics is safer if you can get one.",
    };

    /// <summary>
    /// Fetches the org's people for "Count usage from"; the reload that follows refreshes the picker (a live
    /// update, not a page rebuild). Runs at most once per key while the window is open, so rebuilding the
    /// page or an org with no people can't start another fetch.
    /// </summary>
    async Task LoadActorsOnceAsync()
    {
        if (actorsRequested) return;
        actorsRequested = true;
        try
        {
            await m.LoadActorsAsync();
        }
        catch (Exception e)
        {
            Log.Write($"Loading API actors failed: {e.Message}");
        }
    }

    // MARK: Alerts

    StackPanel Alerts()
    {
        var s = m.Settings;
        var p = Page("Alerts");
        var toggles = new List<ToggleSwitch>();
        ToggleSwitch Dependent(bool on, Action<bool> changed)
        {
            var t = Toggle(on, changed);
            t.IsEnabled = s.alertsEnabled;
            toggles.Add(t);
            return t;
        }

        p.Children.Add(Card("Budget notifications", "Each alert fires once per billing cycle (or limit window).",
            Toggle(s.alertsEnabled, on =>
            {
                m.Update(x => x with { alertsEnabled = on });
                foreach (var t in toggles) t.IsEnabled = on;
            }), "\uEA8F"));
        foreach (var threshold in new uint[] { 50, 80, 90, 100 })
        {
            p.Children.Add(Card($"At {threshold}% of budget", null, Dependent(s.alertThresholds.Contains(threshold), on => m.Update(x => x with
            {
                alertThresholds = on ? [.. x.alertThresholds.Where(t => t != threshold), threshold] : x.alertThresholds.Where(t => t != threshold).ToList(),
            }))));
        }
        p.Children.Add(Card("When the forecast says I'll go over", null, Dependent(s.forecastAlert, on => m.Update(x => x with { forecastAlert = on }))));
        p.Children.Add(Card("When a plan limit passes 90%", null, Dependent(s.limitAlerts, on => m.Update(x => x with { limitAlerts = on }))));
        p.Children.Add(Section("Test"));
        p.Children.Add(Card("Send a test notification", null, Btn("Send", () => m.Engine.SendTestAlert())));
        return p;
    }

    // MARK: Pricing

    StackPanel Pricing()
    {
        var table = NacutMethods.PriceTable();
        var info = NacutMethods.PriceTableInfo();
        var overrides = m.Settings.priceOverrides;
        var p = Page("Pricing");
        p.Children.Add(Section("USD per million tokens"));

        var grid = new Grid { ColumnSpacing = 16, RowSpacing = 2 };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (var i = 0; i < 5; i++) grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        string[] heads = ["Model", "Input", "Output", "5m write", "1h write", "Cache read"];
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        for (var c = 0; c < heads.Length; c++)
        {
            var h = new TextBlock { Text = heads[c], FontSize = 12, FontWeight = FontWeights.SemiBold, Opacity = 0.7, TextAlignment = c == 0 ? TextAlignment.Left : TextAlignment.Right, Margin = new Thickness(8, 0, 8, 6) };
            Grid.SetColumn(h, c);
            grid.Children.Add(h);
        }
        var row = 1;
        foreach (var model in table)
        {
            var r = overrides.TryGetValue(model.id, out var o) ? o : model.rates;
            grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var hit = new Border
            {
                CornerRadius = new CornerRadius(4),
                Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            }.Tip("Double-click to set a custom price");
            Grid.SetRow(hit, row);
            Grid.SetColumnSpan(hit, 6);
            var id = model.id;
            hit.DoubleTapped += async (_, _) => await EditPrice(id);
            hit.PointerEntered += (_, _) => hit.Background = (Brush)Application.Current.Resources["SubtleFillColorSecondaryBrush"];
            hit.PointerExited += (_, _) => hit.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
            grid.Children.Add(hit);

            var name = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, Margin = new Thickness(8, 4, 8, 4), IsHitTestVisible = false };
            name.Children.Add(new TextBlock { Text = model.name });
            if (overrides.ContainsKey(model.id)) name.Children.Add(Ui.Icon("\uE70F", 12, Theme.Coral));
            Grid.SetRow(name, row);
            grid.Children.Add(name);
            double[] values = [r.input, r.output, r.write5m, r.write1h, r.read];
            for (var c = 0; c < 5; c++)
            {
                var t = new TextBlock { Text = Fmt(values[c]), TextAlignment = TextAlignment.Right, Margin = new Thickness(8, 4, 8, 4), IsHitTestVisible = false };
                Grid.SetRow(t, row);
                Grid.SetColumn(t, c + 1);
                grid.Children.Add(t);
            }
            row++;
        }
        p.Children.Add(Ui.Box(grid, 8, 8, null));
        p.Children.Add(Note($"List prices as of {info.updated}. Fast mode is ×{info.fastMultiplier:0}; web search ${info.webSearchPerThousand:0} per 1,000. Haiku 5.5 uses higher rates above 100K prompt tokens. Double-click a row to set a custom price (e.g. a negotiated rate)."));
        if (overrides.Count > 0)
        {
            var reset = Btn("Reset all custom prices", () => { m.Update(x => x with { priceOverrides = [] }); Rebuild(); });
            reset.Margin = new Thickness(0, 12, 0, 0);
            p.Children.Add(reset);
        }
        return p;
    }

    static string Fmt(double v) => v < 1 ? "$" + v.ToString("0.###") : "$" + v.ToString("0.##");

    async Task EditPrice(string id)
    {
        var model = NacutMethods.PriceTable().First(x => x.id == id);
        var r = m.Settings.priceOverrides.TryGetValue(id, out var o) ? o : model.rates;
        NumberBox Field(string header, double value) => new() { Header = header, Value = value, Minimum = 0, SmallChange = 0.1, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Hidden };
        var input = Field("Input", r.input);
        var output = Field("Output", r.output);
        var w5 = Field("Cache write (5m)", r.write5m);
        var w1 = Field("Cache write (1h)", r.write1h);
        var read = Field("Cache read", r.read);
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = model.name,
            Content = new StackPanel { Spacing = 8, Children = { input, output, w5, w1, read } },
            PrimaryButtonText = "Save",
            SecondaryButtonText = "Use list price",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            RequestedTheme = Theme.ElementTheme,
        };
        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
        {
            // A cleared NumberBox reads NaN: keep that field's previous rate.
            static double Keep(double value, double previous) => double.IsFinite(value) ? value : previous;
            var rates = new Rates(Keep(input.Value, r.input), Keep(output.Value, r.output), Keep(w5.Value, r.write5m),
                Keep(w1.Value, r.write1h), Keep(read.Value, r.read));
            m.Update(x => x with { priceOverrides = new Dictionary<string, Rates>(x.priceOverrides) { [id] = rates } });
        }
        else if (result == ContentDialogResult.Secondary)
        {
            m.Update(x => x with { priceOverrides = x.priceOverrides.Where(kv => kv.Key != id).ToDictionary() });
        }
        if (result != ContentDialogResult.None) Rebuild();
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern uint GetDpiForSystem();
}

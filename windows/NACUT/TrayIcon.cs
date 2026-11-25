using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.Runtime.InteropServices;
using System.Windows.Input;
using H.NotifyIcon;
using H.NotifyIcon.Core;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Win32;
using uniffi.nacut;

namespace Nacut;

/// <summary>
/// The notification-area icon. Windows can't show text there, so the icon draws the figure the core
/// picks (worst plan limit, or budget used) as a number over a small gauge bar, or "$" in cost mode.
/// The tooltip carries the full `5h 3% · Week 15%` / `$13.25 · $525` text.
/// </summary>
sealed class TrayIcon : IDisposable
{
    readonly AppModel m;
    readonly TaskbarIcon icon;
    readonly MenuFlyoutItem summary;
    string lastKey = "";
    Icon? current;

    /// <summary>
    /// Whether Windows keeps this icon in the ^ overflow. Explorer records each icon under
    /// HKCU\Control Panel\NotifyIconSettings with its exe path (WindowsApps paths start with a known-folder
    /// GUID) and sets IsPromoted = 1 once the user pins it to the taskbar.
    /// </summary>
    public static bool IsInOverflow()
    {
        try
        {
            var exe = Environment.ProcessPath;
            if (exe is null) return false;
            var tail = Path.Combine(Path.GetFileName(Path.GetDirectoryName(exe)) ?? "", Path.GetFileName(exe));
            using var settings = Registry.CurrentUser.OpenSubKey(@"Control Panel\NotifyIconSettings");
            if (settings is null) return false;
            foreach (var name in settings.GetSubKeyNames())
            {
                using var key = settings.OpenSubKey(name);
                if (key?.GetValue("ExecutablePath") is string path && path.EndsWith(tail, StringComparison.OrdinalIgnoreCase))
                    return key.GetValue("IsPromoted") is not int promoted || promoted == 0;
            }
        }
        catch (Exception e) { Log.Write($"Tray promotion check failed: {e.Message}"); }
        return false;
    }

    public TrayIcon(AppModel model)
    {
        m = model;
        summary = new MenuFlyoutItem { IsEnabled = false };
        var menu = new MenuFlyout();
        menu.Items.Add(summary);
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(new MenuFlyoutItem { Text = "Open NACUT", Command = new Command(() => App.Current.ShowPanel()) });
        menu.Items.Add(new MenuFlyoutItem { Text = "Refresh now", Command = new Command(m.RefreshAll) });
        menu.Items.Add(new MenuFlyoutItem { Text = "Settings…", Command = new Command(() => App.Current.ShowSettings()) });
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(new MenuFlyoutItem { Text = "Quit NACUT", Command = new Command(() => App.Current.Quit()) });

        icon = new TaskbarIcon
        {
            ToolTipText = "NACUT",
            NoLeftClickDelay = true,
            LeftClickCommand = new Command(() => App.Current.TogglePanel()),
            ContextMenuMode = ContextMenuMode.SecondWindow,
            ContextFlyout = menu,
        };
        Update();
        icon.ForceCreate(enablesEfficiencyMode: false);
        m.Changed += Update;
        Theme.Changed += () => App.Current.Dispatcher.TryEnqueue(() => { lastKey = ""; Update(); });
    }

    void Update()
    {
        var st = m.Status;
        var text = st.hasLoaded ? st.menuBarText : "NACUT · loading…";
        summary.Text = text;
        icon.ToolTipText = Truncate(st.hasLoaded ? $"NACUT\n{st.tooltip}" : "NACUT · reading your Claude Code history…", 127);

        var light = TaskbarIsLight();
        var key = $"{st.gaugeFraction:0.00}|{st.level}|{st.showsLimits}|{light}|{SmallIconSize()}";
        if (key == lastKey) return;
        lastKey = key;
        var next = Render(st, light);
        icon.Icon = next;
        current?.Dispose();
        current = next;
    }

    static string Truncate(string s, int max) => s.Length <= max ? s : s[..(max - 1)] + "…";

    /// <summary>Saves the icon as a PNG, enlarged, for screenshots.</summary>
    public static void SavePreview(StatusView st, bool lightTaskbar, string path)
    {
        using var icon = Render(st, lightTaskbar);
        using var small = icon.ToBitmap();
        using var big = new Bitmap(small.Width * 4, small.Height * 4, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(big))
        {
            g.Clear(lightTaskbar ? Color.FromArgb(238, 238, 238) : Color.FromArgb(32, 32, 32));
            g.InterpolationMode = InterpolationMode.NearestNeighbor;
            g.PixelOffsetMode = PixelOffsetMode.Half;
            g.DrawImage(small, 0, 0, big.Width, big.Height);
        }
        big.Save(path, ImageFormat.Png);
    }

    /// <summary>Draws the tray icon at the system's small-icon size for the current DPI.</summary>
    static Icon Render(StatusView st, bool lightTaskbar)
    {
        var size = SmallIconSize();
        using var bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            g.Clear(Color.Transparent);
            var ink = lightTaskbar ? Color.FromArgb(28, 28, 28) : Color.White;
            var accent = st.level switch
            {
                Level.Critical => Color.FromArgb(0xF0, 0x52, 0x52),
                Level.Warning => Color.FromArgb(0xF0, 0xA2, 0x3B),
                _ => Color.FromArgb(0xE5, 0x82, 0x62),
            };
            var barH = Math.Max(2f, size / 8f);
            var textArea = new RectangleF(0, -size * 0.04f, size, size - barH - 1);
            using var format = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center, FormatFlags = StringFormatFlags.NoWrap };

            if (st.gaugeFraction is { } f)
            {
                var pct = (int)Math.Round(f * 100);
                var label = pct >= 100 ? "!" : pct.ToString();
                var fontSize = label.Length >= 2 ? size * 0.62f : size * 0.72f;
                using var font = new Font("Segoe UI", fontSize, FontStyle.Bold, GraphicsUnit.Pixel);
                using var textBrush = new SolidBrush(st.level == Level.Normal ? ink : accent);
                g.DrawString(label, font, textBrush, textArea, format);

                using var track = new SolidBrush(Color.FromArgb(lightTaskbar ? 60 : 80, ink));
                using var fill = new SolidBrush(accent);
                var y = size - barH;
                FillRounded(g, track, new RectangleF(0.5f, y, size - 1, barH));
                var w = Math.Max(barH, (size - 1) * (float)Math.Clamp(f, 0, 1));
                FillRounded(g, fill, new RectangleF(0.5f, y, w, barH));
            }
            else
            {
                using var font = new Font("Segoe UI", size * 0.8f, FontStyle.Bold, GraphicsUnit.Pixel);
                using var brush = new SolidBrush(ink);
                g.DrawString("$", font, brush, new RectangleF(0, 0, size, size), format);
            }
        }
        var handle = bmp.GetHicon();
        try { return (Icon)Icon.FromHandle(handle).Clone(); }
        finally { DestroyIcon(handle); }
    }

    static void FillRounded(Graphics g, Brush brush, RectangleF r)
    {
        var d = Math.Min(r.Height, r.Width);
        using var path = new GraphicsPath();
        path.AddArc(r.X, r.Y, d, d, 90, 180);
        path.AddArc(r.Right - d, r.Y, d, d, 270, 180);
        path.CloseFigure();
        g.FillPath(brush, path);
    }

    static bool TaskbarIsLight()
    {
        using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        return key?.GetValue("SystemUsesLightTheme") is int v && v == 1;
    }

    static int SmallIconSize() => GetSystemMetricsForDpi(49 /* SM_CXSMICON */, GetDpiForSystem());

    public void Dispose()
    {
        icon.Dispose();
        current?.Dispose();
    }

    sealed class Command(Action action) : ICommand
    {
        public event EventHandler? CanExecuteChanged { add { } remove { } }
        public bool CanExecute(object? parameter) => true;
        public void Execute(object? parameter) => action();
    }

    [DllImport("user32.dll")] static extern bool DestroyIcon(IntPtr handle);
    [DllImport("user32.dll")] static extern uint GetDpiForSystem();
    [DllImport("user32.dll")] static extern int GetSystemMetricsForDpi(int index, uint dpi);
}

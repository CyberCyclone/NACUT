using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;
using Nacut.Views;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

namespace Nacut;

/// <summary>
/// `NACUT.exe --render-to DIR [--limits-json FILE]` saves PNGs of every panel tab, every settings page and
/// the tray icon, then exits. It reads the logs once with no network or credential access (like the Mac
/// app's `--render-to`). Must run on an interactive desktop.
/// </summary>
static class DebugRenderer
{
    public static string? OutputDir => Program.Arg("--render-to");

    public static async Task RunAsync(AppModel m)
    {
        var dir = OutputDir!;
        try
        {
            Directory.CreateDirectory(dir);
            if (Program.Arg("--limits-json") is { } json)
            {
                m.Engine.DebugSetLimitsJson(File.ReadAllText(json));
                m.Reload();
            }
            var theme = Theme.IsDark ? "dark" : "light";

            var panel = new PanelWindow(m);
            string[] tabs = ["overview", "calendar", "models", "projects"];
            for (var i = 0; i < tabs.Length; i++)
            {
                panel.ShowForRender(i);
                await Task.Delay(900);
                await Save(panel.Root, Path.Combine(dir, $"panel-{tabs[i]}-{theme}.png"));
                await Save(panel.TabContent, Path.Combine(dir, $"tab-{tabs[i]}-{theme}.png"));
            }
            panel.HidePanel();

            var settings = new SettingsWindow(m);
            foreach (var section in new[] { "general", "budget", "sources", "alerts", "pricing" })
            {
                settings.ShowSection(section);
                await Task.Delay(900);
                await Save((FrameworkElement)settings.Content, Path.Combine(dir, $"settings-{section}-{theme}.png"));
            }
            settings.Close();

            foreach (var light in new[] { true, false })
                TrayIcon.SavePreview(m.Status, light, Path.Combine(dir, $"tray-{(light ? "light" : "dark")}-taskbar.png"));
            Log.Write($"Rendered screenshots to {dir}");
        }
        catch (Exception e)
        {
            Log.Write($"Render failed: {e}");
            File.WriteAllText(Path.Combine(dir, "render-error.txt"), e.ToString());
        }
        finally
        {
            Application.Current.Exit();
        }
    }

    static async Task Save(UIElement element, string path)
    {
        var rtb = new RenderTargetBitmap();
        await rtb.RenderAsync(element);
        var pixels = await rtb.GetPixelsAsync();
        using var stream = new InMemoryRandomAccessStream();
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
            (uint)rtb.PixelWidth, (uint)rtb.PixelHeight, 96, 96, pixels.ToArray());
        await encoder.FlushAsync();
        stream.Seek(0);
        await using var file = File.Create(path);
        await stream.AsStreamForRead().CopyToAsync(file);
    }
}

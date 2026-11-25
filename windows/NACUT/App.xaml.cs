using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;
using Nacut.Views;

namespace Nacut;

/// <summary>
/// Tray app: no main window. The tray icon opens the panel; Settings is a normal window.
/// </summary>
public partial class App : Application
{
    public static new App Current => (App)Application.Current;
    internal AppModel Model { get; private set; } = null!;
    internal DispatcherQueue Dispatcher { get; private set; } = null!;

    TrayIcon? tray;
    PanelWindow? panel;
    SettingsWindow? settings;

    public App()
    {
        InitializeComponent();
        // Keep running with no windows open; Quit calls Exit().
        DispatcherShutdownMode = DispatcherShutdownMode.OnExplicitShutdown;
        UnhandledException += (_, e) => Log.Write($"Unhandled: {e.Exception}");
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        Log.Write("Launching");
        Dispatcher = DispatcherQueue.GetForCurrentThread();
        var renderMode = DebugRenderer.OutputDir is not null;
        Model = new AppModel(Dispatcher, renderMode);
        Log.Write("Engine started");

        if (renderMode)
        {
            _ = DebugRenderer.RunAsync(Model);
            return;
        }

        Notifier.Start(onClick: () => Dispatcher.TryEnqueue(ShowPanel));
        Model.AlertRaised += Notifier.Show;
        Log.Write("Notifications ready");
        tray = new TrayIcon(Model);
        Log.Write("Tray icon created");
        if (Packaging.IsPackaged) WidgetProvider.Register();
        AppInstance.GetCurrent().Activated += (_, _) => Dispatcher.TryEnqueue(ShowPanel);

        // Started at login or by the Widgets board: stay in the tray.
        if (!Program.Flag("--background") && !Program.Flag("-RegisterProcessAsComServer") && !Program.Args.Any(a => a.StartsWith("----AppNotificationActivated"))) ShowPanel();
    }

    public void TogglePanel()
    {
        panel ??= new PanelWindow(Model);
        panel.Toggle();
    }

    public void ShowPanel()
    {
        panel ??= new PanelWindow(Model);
        panel.ShowNearTray();
    }

    public void ShowSettings(string? section = null)
    {
        panel?.HidePanel();
        if (settings is null)
        {
            settings = new SettingsWindow(Model);
            settings.Closed += (_, _) => settings = null;
        }
        settings.ShowSection(section);
    }

    public void Quit()
    {
        Model.Engine.Stop();
        WidgetProvider.Unregister();
        tray?.Dispose();
        Notifier.Stop();
        Exit();
    }
}

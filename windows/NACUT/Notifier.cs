using Microsoft.Win32;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using uniffi.nacut;

namespace Nacut;

/// <summary>Windows toast notifications for the core's alerts. Clicking one opens the panel.</summary>
static class Notifier
{
    static bool registered;

    public static void Start(Action onClick)
    {
        try
        {
            var manager = AppNotificationManager.Default;
            manager.NotificationInvoked += (_, _) => onClick();
            manager.Register();
            registered = true;
        }
        catch (Exception e)
        {
            Log.Write($"Notifications unavailable: {e.Message}");
        }
    }

    public static void Show(Alert alert)
    {
        if (!registered) return;
        try
        {
            var n = new AppNotificationBuilder().AddText(alert.title).AddText(alert.body).BuildNotification();
            AppNotificationManager.Default.Show(n);
        }
        catch (Exception e)
        {
            Log.Write($"Notification failed: {e.Message}");
        }
    }

    public static void Stop()
    {
        if (registered) AppNotificationManager.Default.Unregister();
    }
}

/// <summary>
/// Open at login: the package's StartupTask in MSIX builds, otherwise the per-user Run key.
/// </summary>
static class Startup
{
    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    const string Name = "NACUT";
    const string TaskId = "NACUTStartup";

    public static bool Enabled
    {
        get
        {
            if (Packaging.IsPackaged)
            {
                var state = Task.Run(async () => (await Windows.ApplicationModel.StartupTask.GetAsync(TaskId)).State).Result;
                return state is Windows.ApplicationModel.StartupTaskState.Enabled or Windows.ApplicationModel.StartupTaskState.EnabledByPolicy;
            }
            using var key = Registry.CurrentUser.OpenSubKey(RunKey);
            return key?.GetValue(Name) is string;
        }
        set
        {
            if (Packaging.IsPackaged)
            {
                Task.Run(async () =>
                {
                    var task = await Windows.ApplicationModel.StartupTask.GetAsync(TaskId);
                    if (value) await task.RequestEnableAsync();
                    else task.Disable();
                }).Wait();
                return;
            }
            using var key = Registry.CurrentUser.CreateSubKey(RunKey);
            if (value) key.SetValue(Name, $"\"{Environment.ProcessPath}\" --background");
            else key.DeleteValue(Name, false);
        }
    }
}

static class Packaging
{
    /// <summary>True when running from the MSIX package (which adds the Widgets board provider).</summary>
    public static readonly bool IsPackaged = IsPackagedProcess();

    static bool IsPackagedProcess()
    {
        var length = 0u;
        return GetCurrentPackageFullName(ref length, null) != 15700; // APPMODEL_ERROR_NO_PACKAGE
    }

    [System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
    static extern int GetCurrentPackageFullName(ref uint length, char[]? name);
}

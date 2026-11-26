using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;

namespace Nacut;

public static class Program
{
    /// <summary>
    /// `--render-to DIR` writes screenshots of every view and exits (see <see cref="DebugRenderer"/>).
    /// `--background` starts in the tray without opening the panel (used by Open at login).
    /// </summary>
    public static string[] Args { get; private set; } = [];

    [STAThread]
    static int Main(string[] args)
    {
        Args = args;
        Log.Write($"Starting {string.Join(' ', args)}");
        WinRT.ComWrappersSupport.InitializeComWrappers();

        // One instance: a second launch asks the running one to open its panel, then exits.
        if (DebugRenderer.OutputDir is null)
        {
            var main = AppInstance.FindOrRegisterForKey("NACUT.main");
            if (!main.IsCurrent)
            {
                var activation = AppInstance.GetCurrent().GetActivatedEventArgs();
                Task.Run(() => main.RedirectActivationToAsync(activation).AsTask()).Wait();
                return 0;
            }
        }

        Application.Start(p =>
        {
            var context = new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(context);
            new App();
        });
        return 0;
    }

    public static string? Arg(string name) => Logic.CommandLine.Arg(Args, name);

    public static bool Flag(string name) => Logic.CommandLine.Flag(Args, name);
}

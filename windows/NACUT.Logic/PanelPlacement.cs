namespace Nacut.Logic;

/// <summary>A rectangle in physical (screen) pixels.</summary>
public readonly record struct PixelRect(int X, int Y, int Width, int Height);

/// <summary>A point in physical (screen) pixels.</summary>
public readonly record struct PixelPoint(int X, int Y);

/// <summary>Where the tray panel opens, next to the taskbar on whichever edge it's on.</summary>
public static class PanelPlacement
{
    /// <summary>
    /// The panel's bounds on the display whose work area (screen minus taskbar) and outer bounds are given.
    /// Clicked from the tray (the pointer is outside the work area, i.e. on the taskbar), the panel centers on the
    /// pointer along the taskbar; opened any other way (Start menu, a notification) it opens at the tray corner,
    /// where Windows' own flyouts appear. `scale` is the monitor's DPI scale; sizes are in view pixels.
    /// </summary>
    public static PixelRect Place(PixelRect work, PixelRect outer, PixelPoint cursor, double scale, double panelWidth, double maxHeight)
    {
        var w = (int)(panelWidth * scale);
        var h = (int)Math.Min(maxHeight * scale, work.Height - 24 * scale);
        var margin = (int)(12 * scale);
        var onTaskbar = cursor.X < work.X || cursor.X >= work.X + work.Width || cursor.Y < work.Y || cursor.Y >= work.Y + work.Height;
        if (!onTaskbar) cursor = new PixelPoint(work.X + work.Width, work.Y + work.Height);

        // Along an edge: center on the pointer, kept a margin inside the work area.
        int AlongX() => Clamp(cursor.X - w / 2, work.X + margin, work.X + work.Width - w - margin);
        int AlongY() => Clamp(cursor.Y - h / 2, work.Y + margin, work.Y + work.Height - h - margin);

        int x, y;
        if (work.Y > outer.Y) // taskbar on top
        {
            x = AlongX();
            y = work.Y + margin;
        }
        else if (work.X > outer.X) // left
        {
            x = work.X + margin;
            y = AlongY();
        }
        else if (work.Width < outer.Width) // right
        {
            x = work.X + work.Width - w - margin;
            y = AlongY();
        }
        else // bottom (default, and an auto-hidden taskbar)
        {
            x = AlongX();
            y = work.Y + work.Height - h - margin;
        }
        return new PixelRect(x, y, w, h);
    }

    /// <summary>Math.Clamp, except a work area too small for the panel pins it to the leading margin instead of throwing.</summary>
    static int Clamp(int value, int min, int max) => max < min ? min : Math.Clamp(value, min, max);
}

namespace Nacut.Tests;

public class PanelPlacementTests
{
    const double Width = 452, MaxHeight = 780;

    static PixelRect Place(PixelRect work, PixelRect outer, int x, int y, double scale = 1) =>
        PanelPlacement.Place(work, outer, new PixelPoint(x, y), scale, Width, MaxHeight);

    // 1920×1080 with a 48 px taskbar on each edge.
    static readonly PixelRect Screen = new(0, 0, 1920, 1080);
    static readonly PixelRect BottomWork = new(0, 0, 1920, 1032);
    static readonly PixelRect TopWork = new(0, 48, 1920, 1032);
    static readonly PixelRect LeftWork = new(48, 0, 1872, 1080);
    static readonly PixelRect RightWork = new(0, 0, 1872, 1080);

    [Fact]
    public void BottomTaskbarCentersOnThePointerWithinTheMargin()
    {
        Assert.Equal(new PixelRect(774, 240, 452, 780), Place(BottomWork, Screen, 1000, 1050));
        Assert.Equal(new PixelRect(1456, 240, 452, 780), Place(BottomWork, Screen, 1800, 1050)); // kept off the right edge
        Assert.Equal(new PixelRect(12, 240, 452, 780), Place(BottomWork, Screen, 100, 1050));    // kept off the left edge
    }

    [Fact]
    public void TopTaskbarOpensBelowIt() =>
        Assert.Equal(new PixelRect(674, 60, 452, 780), Place(TopWork, Screen, 900, 20));

    [Fact]
    public void LeftTaskbarOpensBesideItCenteredOnThePointer() =>
        Assert.Equal(new PixelRect(60, 110, 452, 780), Place(LeftWork, Screen, 20, 500));

    [Fact]
    public void RightTaskbarOpensBesideItKeptOnScreen() =>
        Assert.Equal(new PixelRect(1408, 288, 452, 780), Place(RightWork, Screen, 1900, 1000));

    [Fact]
    public void OpenedAwayFromTheTaskbarUsesTheTrayCorner()
    {
        // Start menu or a notification: the pointer is anywhere, so open where Windows' flyouts do.
        Assert.Equal(new PixelRect(1456, 240, 452, 780), Place(BottomWork, Screen, 500, 500));
        Assert.Equal(new PixelRect(1408, 288, 452, 780), Place(RightWork, Screen, 500, 500));
        Assert.Equal(new PixelRect(1456, 60, 452, 780), Place(TopWork, Screen, 500, 500));
        Assert.Equal(new PixelRect(60, 288, 452, 780), Place(LeftWork, Screen, 500, 500));
    }

    [Fact]
    public void TheWorkAreaEdgeCountsAsTheTaskbar()
    {
        Assert.Equal(774, Place(BottomWork, Screen, 1000, 1032).X);  // first taskbar row
        Assert.Equal(1456, Place(BottomWork, Screen, 1000, 1031).X); // last work-area row: tray corner
    }

    [Fact]
    public void AutoHiddenTaskbarOpensAtTheBottomRight() =>
        Assert.Equal(new PixelRect(1456, 288, 452, 780), Place(Screen, Screen, 1800, 1079));

    [Fact]
    public void HighDpiScalesSizeAndMargin()
    {
        var outer = new PixelRect(0, 0, 2880, 1620);
        var work = new PixelRect(0, 0, 2880, 1548);
        Assert.Equal(new PixelRect(2184, 360, 678, 1170), Place(work, outer, 2700, 1600, 1.5));
    }

    [Fact]
    public void SecondMonitorLeftOfThePrimaryHasNegativeCoordinates()
    {
        var outer = new PixelRect(-1920, 0, 1920, 1080);
        var work = new PixelRect(-1920, 0, 1920, 1032);
        Assert.Equal(new PixelRect(-464, 240, 452, 780), Place(work, outer, -100, 1050));
    }

    [Fact]
    public void ShortScreensShrinkThePanel()
    {
        var outer = new PixelRect(0, 0, 1920, 748);
        var work = new PixelRect(0, 0, 1920, 700);
        Assert.Equal(new PixelRect(1456, 12, 452, 676), Place(work, outer, 1800, 720));
    }

    [Fact]
    public void ScreensNarrowerThanThePanelPinItToTheMarginInsteadOfThrowing()
    {
        var outer = new PixelRect(0, 0, 400, 648);
        var work = new PixelRect(0, 0, 400, 600);
        Assert.Equal(new PixelRect(12, 12, 452, 576), Place(work, outer, 390, 620));
        // Left taskbar on a short screen: the panel takes the height less the margins.
        var shortOuter = new PixelRect(0, 0, 1920, 500);
        var shortLeft = new PixelRect(48, 0, 1872, 500);
        Assert.Equal(new PixelRect(60, 12, 452, 476), Place(shortLeft, shortOuter, 20, 250));
    }
}

namespace Nacut.Tests;

public class PaletteTests
{
    [Fact]
    public void ArgbSplitsTheChannels()
    {
        Assert.Equal(((byte)255, (byte)0xD9, (byte)0x77, (byte)0x57), Palette.Argb(0xD97757));
        Assert.Equal(((byte)0xC5, (byte)0xFF, (byte)0xFF, (byte)0xFF), Palette.Argb(0xFFFFFF, 0xC5));
        Assert.Equal(((byte)255, (byte)0, (byte)0, (byte)0), Palette.Argb(0));
        // Bits above 0xRRGGBB are ignored.
        Assert.Equal(((byte)255, (byte)0x12, (byte)0x34, (byte)0x56), Palette.Argb(0xFF123456));
    }

    [Fact]
    public void PairsPickTheThemeStep()
    {
        Assert.Equal(0xD97757u, Palette.Coral.For(dark: false));
        Assert.Equal(0xE58262u, Palette.Coral.For(dark: true));
    }

    [Fact]
    public void TableSizes()
    {
        Assert.Equal(8, Palette.Categorical.Count);
        Assert.Equal(5, Palette.Ramp.Count);
        Assert.Equal(Palette.Categorical.Count, Palette.Categorical.Distinct().Count());
    }

    // MARK: Model slots

    [Fact]
    public void SlotsMapToTheCategoricalPalette()
    {
        Assert.Equal(new ColorPair(0x2A78D6, 0x3987E5), Palette.ForSlot(0)); // blue
        Assert.Equal(new ColorPair(0xE34948, 0xE66767), Palette.ForSlot(7)); // red
    }

    [Fact]
    public void NoSlotOrAnUnknownSlotIsOther()
    {
        Assert.Equal(Palette.Other, Palette.ForSlot(null));
        Assert.Equal(Palette.Other, Palette.ForSlot(8));
        Assert.Equal(Palette.Other, Palette.ForSlot(uint.MaxValue));
    }

    // MARK: Ramp

    [Theory]
    [InlineData(0.01, 0)]
    [InlineData(0.2, 0)]
    [InlineData(0.21, 1)]
    [InlineData(0.5, 2)]
    [InlineData(0.7, 3)]
    [InlineData(0.9, 4)]
    [InlineData(1.0, 4)]
    [InlineData(5.0, 4)]
    [InlineData(double.PositiveInfinity, 4)]
    [InlineData(double.Epsilon, 0)]
    public void RampIndex(double fraction, int expected) => Assert.Equal(expected, Palette.RampIndex(fraction));

    [Theory]
    [InlineData(0.0)]
    [InlineData(-0.0)]
    [InlineData(-1.0)]
    [InlineData(double.NaN)]
    [InlineData(double.NegativeInfinity)]
    public void RampHasNoFillForNothing(double fraction) => Assert.Null(Palette.RampIndex(fraction));

    [Theory]
    [InlineData(0.6, false)]
    [InlineData(0.61, true)]
    [InlineData(1.0, true)]
    [InlineData(double.NaN, false)]
    public void LightInkOnDarkSteps(double fraction, bool expected) => Assert.Equal(expected, Palette.LightInkOnRamp(fraction));

    // MARK: Limits and statuses

    [Theory]
    [InlineData(0.0, LimitLevel.Normal)]
    [InlineData(0.6999, LimitLevel.Normal)]
    [InlineData(0.7, LimitLevel.Warning)]
    [InlineData(0.8999, LimitLevel.Warning)]
    [InlineData(0.9, LimitLevel.Critical)]
    [InlineData(1.5, LimitLevel.Critical)]
    [InlineData(-0.1, LimitLevel.Normal)]
    [InlineData(double.NaN, LimitLevel.Normal)]
    [InlineData(double.PositiveInfinity, LimitLevel.Critical)]
    public void ForLimit(double utilization, LimitLevel expected) => Assert.Equal(expected, Palette.ForLimit(utilization));

    [Theory]
    [InlineData(StatusKind.NoBudget, "")]
    [InlineData(StatusKind.UnderBudget, "")]
    [InlineData(StatusKind.Tight, "")]
    [InlineData(StatusKind.WillExceed, "")]
    [InlineData(StatusKind.Exceeded, "")]
    public void StatusGlyph(StatusKind status, string glyph) => Assert.Equal(glyph, Palette.StatusGlyph(status));

    [Fact]
    public void StatusColors()
    {
        Assert.Equal(Palette.Status(StatusKind.NoBudget), Palette.Status(StatusKind.UnderBudget));
        Assert.Equal(new ColorPair(0xB86E00, 0xF0A23B), Palette.Status(StatusKind.Tight));
        Assert.Equal(new ColorPair(0xC2410C, 0xF97316), Palette.Status(StatusKind.WillExceed));
        Assert.Equal(new ColorPair(0xC0262D, 0xF05252), Palette.Status(StatusKind.Exceeded));
    }
}

namespace Nacut.Tests;

public class TrayRulesTests
{
    // MARK: Label

    [Fact]
    public void NoGaugeShowsADollarSign() => Assert.Equal(new TrayLabel("$", 0.8f, false), TrayRules.Label(null));

    [Theory]
    [InlineData(0.0, "0")]
    [InlineData(0.05, "5")]
    [InlineData(0.094, "9")]
    [InlineData(0.1, "10")]
    [InlineData(0.42, "42")]
    [InlineData(0.994, "99")]
    [InlineData(0.999, "!")] // rounds to 100
    [InlineData(1.0, "!")]
    [InlineData(1.7, "!")]
    [InlineData(-0.2, "0")]
    [InlineData(double.NaN, "0")]
    [InlineData(double.PositiveInfinity, "!")]
    [InlineData(double.NegativeInfinity, "0")]
    public void GaugeText(double fraction, string expected)
    {
        var label = TrayRules.Label(fraction);
        Assert.Equal(expected, label.Text);
        Assert.True(label.HasGauge);
    }

    [Theory]
    [InlineData(0.0, 0.72f)]  // "0"
    [InlineData(0.09, 0.72f)] // "9"
    [InlineData(0.1, 0.62f)]  // "10"
    [InlineData(0.99, 0.62f)] // "99"
    [InlineData(1.0, 0.72f)]  // "!"
    public void TwoDigitsUseASmallerFont(double fraction, float expected) =>
        Assert.Equal(expected, TrayRules.Label(fraction).FontScale);

    [Fact]
    public void LabelDoesNotDependOnTheCurrentCulture()
    {
        var saved = CultureInfo.CurrentCulture;
        try
        {
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("ar-SA");
            Assert.Equal("42", TrayRules.Label(0.42).Text);
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("de-DE");
            Assert.Equal("42", TrayRules.Label(0.42).Text);
        }
        finally
        {
            CultureInfo.CurrentCulture = saved;
        }
    }

    // MARK: Gauge bar

    [Theory]
    [InlineData(8, 2f)] // never thinner than 2 px
    [InlineData(16, 2f)]
    [InlineData(20, 2.5f)]
    [InlineData(32, 4f)]
    public void BarHeight(int size, float expected) => Assert.Equal(expected, TrayRules.BarHeight(size));

    [Theory]
    [InlineData(0.0, 16, 2f)]   // at least a dot as wide as the bar is tall
    [InlineData(0.5, 16, 7.5f)]
    [InlineData(1.0, 16, 15f)]
    [InlineData(2.0, 16, 15f)]  // clamped to the track
    [InlineData(-1.0, 16, 2f)]
    [InlineData(double.NaN, 16, 2f)]
    [InlineData(0.1, 32, 4f)]
    [InlineData(0.5, 33, 16f)]
    public void GaugeWidth(double fraction, int size, float expected) =>
        Assert.Equal(expected, TrayRules.GaugeWidth(fraction, size));

    // MARK: Overflow

    const string Exe = @"C:\Program Files\NACUT\NACUT.exe";

    static (string?, object?) Entry(string? path, object? promoted) => (path, promoted);

    [Fact]
    public void NoEntriesMeansNotInOverflow() => Assert.False(TrayRules.IsInOverflow(Exe, []));

    [Fact]
    public void UnknownExeMeansNotInOverflow()
    {
        Assert.False(TrayRules.IsInOverflow(null, [Entry(Exe, 0)]));
        Assert.False(TrayRules.IsInOverflow("", [Entry(Exe, 0)]));
    }

    [Fact]
    public void PromotedIconIsNotInOverflow() =>
        Assert.False(TrayRules.IsInOverflow(Exe, [Entry(@"C:\Program Files\NACUT\NACUT.exe", 1)]));

    [Fact]
    public void UnpromotedIconIsInOverflow() =>
        Assert.True(TrayRules.IsInOverflow(Exe, [Entry(@"C:\Program Files\NACUT\NACUT.exe", 0)]));

    [Fact]
    public void MissingIsPromotedMeansOverflow() =>
        Assert.True(TrayRules.IsInOverflow(Exe, [Entry(@"C:\Program Files\NACUT\NACUT.exe", null)]));

    [Theory]
    [InlineData("1")]  // REG_SZ
    [InlineData(1L)]   // REG_QWORD
    [InlineData(true)]
    public void IsPromotedMustBeADwordOne(object promoted) =>
        Assert.True(TrayRules.IsInOverflow(Exe, [Entry(@"C:\Program Files\NACUT\NACUT.exe", promoted)]));

    [Fact]
    public void PathsMatchCaseInsensitively() =>
        Assert.False(TrayRules.IsInOverflow(Exe, [Entry(@"c:\program files\nacut\nacut.EXE", 1)]));

    [Fact]
    public void PackagedPathsMatchOnFolderAndFile()
    {
        // Explorer records WindowsApps paths under a known-folder GUID instead of the drive path.
        const string exe = @"C:\Program Files\WindowsApps\CaseyGibson.NACUT_1.0.0.0_x64__abc123\NACUT.exe";
        const string recorded = @"{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}\WindowsApps\CaseyGibson.NACUT_1.0.0.0_x64__abc123\NACUT.exe";
        Assert.True(TrayRules.IsInOverflow(exe, [Entry(recorded, 0)]));
    }

    [Fact]
    public void ForwardSlashesInTheExePathStillMatch() =>
        Assert.True(TrayRules.IsInOverflow("C:/Program Files/NACUT/NACUT.exe", [Entry(@"D:\Apps\NACUT\NACUT.exe", 0)]));

    [Fact]
    public void SameFileNameInAnotherFolderDoesNotMatch() =>
        Assert.False(TrayRules.IsInOverflow(Exe, [Entry(@"C:\Other\NACUT.exe", 0), Entry(@"C:\Tools\NOTNACUT\NACUT.exe", 0)]));

    [Fact]
    public void ExeAtADriveRootMatchesOnFileName() =>
        Assert.True(TrayRules.IsInOverflow(@"C:\NACUT.exe", [Entry(@"D:\Anything\NACUT.exe", 0)]));

    [Fact]
    public void EntriesWithoutAPathAreSkipped() =>
        Assert.True(TrayRules.IsInOverflow(Exe, [Entry(null, 1), Entry(@"C:\Program Files\NACUT\NACUT.exe", 0)]));

    [Fact]
    public void TheFirstMatchingEntryDecides()
    {
        Assert.False(TrayRules.IsInOverflow(Exe, [Entry(Exe, 1), Entry(Exe, 0)]));
        Assert.True(TrayRules.IsInOverflow(Exe, [Entry(Exe, 0), Entry(Exe, 1)]));
    }

    [Fact]
    public void StopsReadingEntriesAfterAMatch()
    {
        static IEnumerable<(string?, object?)> Entries()
        {
            yield return (Exe, 1);
            throw new InvalidOperationException("read past the match");
        }
        Assert.False(TrayRules.IsInOverflow(Exe, Entries()));
    }
}

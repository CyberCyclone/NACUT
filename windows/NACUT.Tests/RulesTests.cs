namespace Nacut.Tests;

public class CommandLineTests
{
    [Fact]
    public void ArgReturnsTheValueAfterTheName()
    {
        string[] args = ["--render-to", @"C:\shots", "--limits-json", "limits.json"];
        Assert.Equal(@"C:\shots", CommandLine.Arg(args, "--render-to"));
        Assert.Equal("limits.json", CommandLine.Arg(args, "--limits-json"));
    }

    [Fact]
    public void ArgIsNullWhenMissingOrLast()
    {
        Assert.Null(CommandLine.Arg([], "--render-to"));
        Assert.Null(CommandLine.Arg(["--background"], "--render-to"));
        Assert.Null(CommandLine.Arg(["--background", "--render-to"], "--render-to"));
    }

    [Fact]
    public void ArgUsesTheFirstOccurrence() =>
        Assert.Equal("a", CommandLine.Arg(["--x", "a", "--x", "b"], "--x"));

    [Fact]
    public void ArgTakesTheNextTokenEvenIfItLooksLikeASwitch() =>
        Assert.Equal("--background", CommandLine.Arg(["--render-to", "--background"], "--render-to"));

    [Fact]
    public void FlagMatchesWholeArgumentsExactly()
    {
        string[] args = ["--background", "-RegisterProcessAsComServer"];
        Assert.True(CommandLine.Flag(args, "--background"));
        Assert.True(CommandLine.Flag(args, "-RegisterProcessAsComServer"));
        Assert.False(CommandLine.Flag(args, "--Background"));
        Assert.False(CommandLine.Flag(args, "--back"));
        Assert.False(CommandLine.Flag([], "--background"));
    }
}

public class RulesTests
{
    [Theory]
    [InlineData("Cycle", RangeChoice.Cycle)]
    [InlineData("Last30", RangeChoice.Last30)]
    [InlineData("AllTime", RangeChoice.AllTime)]
    [InlineData("", RangeChoice.Cycle)]
    [InlineData("last30", RangeChoice.Cycle)] // names are case-sensitive, as written by the app
    [InlineData("Weekly", RangeChoice.Cycle)]
    [InlineData("7", RangeChoice.Cycle)]      // a number that isn't a range
    [InlineData("-1", RangeChoice.Cycle)]
    public void ParseRange(string stored, RangeChoice expected) => Assert.Equal(expected, Rules.ParseRange(stored));

    [Fact]
    public void ParseRangeOfNothingIsTheCycle() => Assert.Equal(RangeChoice.Cycle, Rules.ParseRange(null));

    [Fact]
    public void RangeNamesMatchTheCoreStatRange()
    {
        // The app saves StatRange.ToString() and maps RangeChoice back by these names.
        Assert.Equal(new[] { "Cycle", "Last30", "AllTime" }, Enum.GetNames<RangeChoice>());
        foreach (var r in Enum.GetValues<RangeChoice>()) Assert.Equal(r, Rules.ParseRange(r.ToString()));
    }

    [Theory]
    [InlineData(false, false, false)]
    [InlineData(false, true, true)]
    [InlineData(true, false, true)]
    [InlineData(true, true, true)]
    public void ShowingLimitsTurnsLimitFetchingOn(bool limitsEnabled, bool displayIsLimits, bool expected) =>
        Assert.Equal(expected, Rules.LimitsEnabledAfterDisplayChange(limitsEnabled, displayIsLimits));
}

namespace Nacut.Tests;

public class FormatsTests
{
    static readonly CultureInfo EnUs = CultureInfo.GetCultureInfo("en-US");
    static readonly CultureInfo DeDe = CultureInfo.GetCultureInfo("de-DE");
    static readonly DateTime Now = new(2026, 11, 26, 12, 0, 0);

    // MARK: Duration

    [Theory]
    [InlineData(0.0, "<1m")]
    [InlineData(59.9, "<1m")]
    [InlineData(-5.0, "<1m")]
    [InlineData(60.0, "1m")]
    [InlineData(119.0, "1m")]
    [InlineData(3599.0, "59m")]
    [InlineData(3600.0, "1h 0m")]
    [InlineData(3660.0, "1h 1m")]
    [InlineData(47 * 3600.0 + 59 * 60, "47h 59m")]
    [InlineData(48 * 3600.0, "2d 0h")]
    [InlineData(50 * 3600.0 + 30 * 60, "2d 2h")]
    [InlineData(10 * 86400.0 + 5 * 3600, "10d 5h")]
    public void Duration(double secs, string expected) => Assert.Equal(expected, Formats.Duration(secs));

    [Theory]
    [InlineData(double.NaN)]
    [InlineData(double.PositiveInfinity)]
    [InlineData(double.NegativeInfinity)]
    public void DurationOfNonFiniteIsUnderAMinute(double secs) => Assert.Equal("<1m", Formats.Duration(secs));

    [Fact]
    public void DurationBeyondIntRangeDoesNotOverflow()
    {
        var text = Formats.Duration(1e12);
        Assert.EndsWith("h", text);
        Assert.DoesNotContain("-", text);
    }

    // MARK: Relative

    [Theory]
    [InlineData(0, "just now")]
    [InlineData(30, "just now")]
    [InlineData(-120, "just now")] // a clock a little ahead
    [InlineData(60, "1 min ago")]
    [InlineData(5 * 60 + 59, "5 min ago")]
    [InlineData(59 * 60 + 59, "59 min ago")]
    [InlineData(3600, "1 hr ago")]
    [InlineData(23 * 3600 + 59 * 60, "23 hr ago")]
    [InlineData(24 * 3600, "yesterday")]
    [InlineData(47 * 3600, "yesterday")]
    [InlineData(48 * 3600, "2 days ago")]
    [InlineData(29 * 86400 + 3600, "29 days ago")]
    public void Relative(int secondsAgo, string expected) =>
        Assert.Equal(expected, Formats.Relative(Now.AddSeconds(-secondsAgo), Now, EnUs));

    [Fact]
    public void RelativeFromThirtyDaysShowsTheDateInTheGivenCulture()
    {
        var t = new DateTime(2026, 10, 27, 9, 0, 0);
        Assert.Equal("Oct 27", Formats.Relative(t, Now, EnUs));
        Assert.Equal(t.ToString("MMM d", DeDe), Formats.Relative(t, Now, DeDe));
        Assert.NotEqual("Oct 27", Formats.Relative(t, Now, DeDe));
    }

    // MARK: Updated

    [Fact]
    public void UpdatedBeforeTheFirstScanIsLoading() => Assert.Equal("Loading…", Formats.Updated(null, Now));

    [Theory]
    [InlineData(0, "Updated just now")]
    [InlineData(59, "Updated just now")]
    [InlineData(-30, "Updated just now")]
    [InlineData(60, "Updated 1m ago")]
    [InlineData(119, "Updated 1m ago")]
    [InlineData(2 * 3600, "Updated 120m ago")]
    public void Updated(int secondsAgo, string expected) =>
        Assert.Equal(expected, Formats.Updated(Now.AddSeconds(-secondsAgo), Now));

    // MARK: HourLabel

    [Theory]
    [InlineData(0, "12 am")]
    [InlineData(6, "6 am")]
    [InlineData(11, "11 am")]
    [InlineData(12, "12 pm")]
    [InlineData(18, "6 pm")]
    [InlineData(23, "11 pm")]
    public void HourLabel(int hour, string expected)
    {
        Assert.Equal(expected, Formats.HourLabel(hour, CultureInfo.InvariantCulture));
        Assert.Equal(expected, Formats.HourLabel(hour, EnUs));
    }

    // MARK: Price

    [Theory]
    [InlineData(0.0, "$0")]
    [InlineData(0.0004, "$0")]
    [InlineData(0.001, "$0.001")]
    [InlineData(0.08, "$0.08")]
    [InlineData(0.3, "$0.3")]
    [InlineData(0.1234, "$0.123")]
    [InlineData(1.0, "$1")]
    [InlineData(1.234, "$1.23")]
    [InlineData(3.75, "$3.75")]
    [InlineData(15.0, "$15")]
    [InlineData(1234.5, "$1234.5")]
    public void PriceInvariant(double value, string expected) =>
        Assert.Equal(expected, Formats.Price(value, CultureInfo.InvariantCulture));

    [Fact]
    public void PriceFollowsTheGivenCulture()
    {
        // Why the app passes the invariant culture: German would otherwise show "$1,5".
        Assert.Equal("$1,5", Formats.Price(1.5, DeDe));
        Assert.Equal("$0,25", Formats.Price(0.25, DeDe));
        Assert.Equal("$1.5", Formats.Price(1.5, EnUs));
    }

    // MARK: KeepFinite

    [Theory]
    [InlineData(double.NaN, 3.0, 3.0)]
    [InlineData(double.PositiveInfinity, 3.0, 3.0)]
    [InlineData(double.NegativeInfinity, 3.0, 3.0)]
    [InlineData(2.5, 3.0, 2.5)]
    [InlineData(0.0, 3.0, 0.0)]
    public void KeepFinite(double value, double previous, double expected) =>
        Assert.Equal(expected, Formats.KeepFinite(value, previous));

    // MARK: Truncate

    [Theory]
    [InlineData("", 5, "")]
    [InlineData("abc", 3, "abc")]
    [InlineData("abc", 10, "abc")]
    [InlineData("abcd", 3, "ab…")]
    [InlineData("abc", 1, "…")]
    public void Truncate(string s, int max, string expected) => Assert.Equal(expected, Formats.Truncate(s, max));

    [Fact]
    public void TruncateKeepsTooltipsWithinTheTrayLimit()
    {
        var text = Formats.Truncate(new string('x', 200), 127);
        Assert.Equal(127, text.Length);
        Assert.EndsWith("…", text);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    public void TruncateRejectsAMaxBelowOne(int max) =>
        Assert.Throws<ArgumentOutOfRangeException>(() => Formats.Truncate("abc", max));

    // MARK: IsAdminKey

    [Theory]
    [InlineData("sk-ant-admin01-abc", true)]
    [InlineData("  sk-ant-admin01-abc \n", true)]
    [InlineData("sk-ant-api03-abc", false)]
    [InlineData("SK-ANT-ADMIN01-abc", false)]
    [InlineData("x sk-ant-admin01", false)]
    [InlineData("", false)]
    [InlineData("   ", false)]
    public void IsAdminKey(string key, bool expected) => Assert.Equal(expected, Formats.IsAdminKey(key));

    // MARK: LimitLabel

    [Theory]
    [InlineData("seven_day", "7-day", "Weekly")]
    [InlineData("five_hour", "5h", "5h")]
    [InlineData("seven_day_opus", "Opus week", "Opus week")]
    [InlineData("Seven_Day", "7-day", "7-day")]
    public void LimitLabel(string id, string label, string expected) => Assert.Equal(expected, Formats.LimitLabel(id, label));
}

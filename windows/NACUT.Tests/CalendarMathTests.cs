namespace Nacut.Tests;

public class CalendarMathTests
{
    static readonly CultureInfo EnUs = CultureInfo.GetCultureInfo("en-US");
    static readonly CultureInfo DeDe = CultureInfo.GetCultureInfo("de-DE");
    static readonly DateOnly Today = new(2026, 11, 26);

    static CultureInfo InvariantStartingOn(DayOfWeek first)
    {
        var c = (CultureInfo)CultureInfo.InvariantCulture.Clone();
        c.DateTimeFormat.FirstDayOfWeek = first;
        return c;
    }

    // MARK: Months

    [Fact]
    public void NoDataShowsJustThisMonth() =>
        Assert.Equal(new[] { new DateOnly(2026, 11, 1) }, CalendarMath.Months([], Today));

    [Fact]
    public void MonthsRunFromTheFirstDataThroughThisMonth() =>
        Assert.Equal(
            new[] { new DateOnly(2026, 9, 1), new DateOnly(2026, 10, 1), new DateOnly(2026, 11, 1) },
            CalendarMath.Months([new DateOnly(2026, 11, 3), new DateOnly(2026, 9, 15)], Today));

    [Fact]
    public void MonthsCrossTheYear() =>
        Assert.Equal(
            new[] { new DateOnly(2025, 12, 1), new DateOnly(2026, 1, 1), new DateOnly(2026, 2, 1) },
            CalendarMath.Months([new DateOnly(2025, 12, 31)], new DateOnly(2026, 2, 10)));

    [Fact]
    public void FutureDaysDoNotAddMonths() =>
        Assert.Equal(new[] { new DateOnly(2026, 11, 1) }, CalendarMath.Months([new DateOnly(2027, 1, 5), new DateOnly(2026, 11, 30)], Today));

    // MARK: Scale

    [Fact]
    public void ScaleWithoutSpendIsOne()
    {
        Assert.Equal(1, CalendarMath.ScaleMax([]));
        Assert.Equal(1, CalendarMath.ScaleMax([0, -3, double.NaN]));
    }

    [Fact]
    public void ScaleIsThe95thPercentileDayWithSpend()
    {
        Assert.Equal(5, CalendarMath.ScaleMax([5]));
        Assert.Equal(10, CalendarMath.ScaleMax([.. Enumerable.Range(1, 10).Select(i => (double)i), 0, 0]));
        // 30 days: index (int)(30 × 0.95) = 28, so one outlier doesn't wash out the rest.
        Assert.Equal(29, CalendarMath.ScaleMax([1000, .. Enumerable.Range(1, 29).Select(i => (double)i)]));
    }

    [Fact]
    public void ScaleIsAtLeastOneCent() => Assert.Equal(0.01, CalendarMath.ScaleMax([0.001, 0.002]));

    // MARK: Layout

    [Fact]
    public void SundayFirstWeek()
    {
        var layout = CalendarMath.Layout(new DateOnly(2026, 11, 1), CultureInfo.InvariantCulture); // Nov 1 2026 is a Sunday
        Assert.Equal(0, layout.Lead);
        Assert.Equal(30, layout.Days);
        Assert.Equal(new[] { "Su", "Mo", "Tu", "We", "Th", "Fr", "Sa" }, layout.Headers);
    }

    [Fact]
    public void MondayFirstWeek()
    {
        var layout = CalendarMath.Layout(new DateOnly(2026, 11, 1), InvariantStartingOn(DayOfWeek.Monday));
        Assert.Equal(6, layout.Lead);
        Assert.Equal(new[] { "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su" }, layout.Headers);
    }

    [Fact]
    public void LayoutUsesAnyDayOfTheMonth()
    {
        var layout = CalendarMath.Layout(new DateOnly(2026, 11, 26), EnUs);
        Assert.Equal(0, layout.Lead);
        Assert.Equal(30, layout.Days);
    }

    [Theory]
    [InlineData(2026, 11, 0, 6)] // Sunday the 1st
    [InlineData(2027, 2, 1, 0)]  // Monday the 1st
    [InlineData(2026, 8, 6, 5)]  // Saturday the 1st
    [InlineData(2024, 2, 4, 3)]  // Thursday the 1st
    public void LeadFollowsTheCulturesFirstDay(int year, int month, int enUsLead, int deDeLead)
    {
        var first = new DateOnly(year, month, 1);
        Assert.Equal(enUsLead, CalendarMath.Layout(first, EnUs).Lead);
        Assert.Equal(deDeLead, CalendarMath.Layout(first, DeDe).Lead);
        Assert.Equal(DeDe.DateTimeFormat.ShortestDayNames[(int)DayOfWeek.Monday], CalendarMath.Layout(first, DeDe).Headers[0]);
        Assert.Equal(EnUs.DateTimeFormat.ShortestDayNames[(int)DayOfWeek.Sunday], CalendarMath.Layout(first, EnUs).Headers[0]);
    }

    [Theory]
    [InlineData(2024, 2, 29)]
    [InlineData(2026, 2, 28)]
    [InlineData(2026, 4, 30)]
    [InlineData(2026, 12, 31)]
    public void DaysInMonth(int year, int month, int days) =>
        Assert.Equal(days, CalendarMath.Layout(new DateOnly(year, month, 1), EnUs).Days);

    [Fact]
    public void CellsFillRowsFromRowOne()
    {
        Assert.Equal((1, 0), CalendarMath.Cell(0, 0));
        Assert.Equal((1, 6), CalendarMath.Cell(6, 0));
        Assert.Equal((2, 0), CalendarMath.Cell(6, 1));
        Assert.Equal((6, 1), CalendarMath.Cell(6, 30)); // Aug 2026 (en-US): the 31st in the sixth week
    }

    [Fact]
    public void EveryMonthFitsSixWeeks()
    {
        foreach (var culture in new[] { EnUs, DeDe, InvariantStartingOn(DayOfWeek.Saturday) })
        {
            for (var m = new DateOnly(2024, 1, 1); m < new DateOnly(2028, 1, 1); m = m.AddMonths(1))
            {
                var layout = CalendarMath.Layout(m, culture);
                Assert.InRange(layout.Lead, 0, 6);
                Assert.InRange(CalendarMath.Cell(layout.Lead, layout.Days - 1).Row, 1, 6);
            }
        }
    }

    // MARK: Summary

    static KeyValuePair<DateOnly, double> Day(int month, int day, double cost) => new(new DateOnly(2026, month, day), cost);

    [Fact]
    public void EmptyMonth()
    {
        var s = CalendarMath.Summary([Day(10, 3, 5)], new DateOnly(2026, 11, 1));
        Assert.Equal(new MonthStats(0, 0, null, null, 0), s);
    }

    [Fact]
    public void SummaryCountsOnlyDaysWithSpendInTheMonth()
    {
        var s = CalendarMath.Summary(
            [Day(11, 2, 4), Day(11, 3, 0), Day(11, 9, 8), Day(10, 31, 100), Day(12, 1, 100), new(new DateOnly(2025, 11, 5), 100)],
            new DateOnly(2026, 11, 15));
        Assert.Equal(12, s.Total);
        Assert.Equal(2, s.ActiveDays);
        Assert.Equal(6, s.AveragePerActiveDay);
        Assert.Equal(new DateOnly(2026, 11, 9), s.BusiestDay);
        Assert.Equal(8, s.BusiestCost);
    }

    [Fact]
    public void BusiestTieGoesToTheEarliestDay()
    {
        var s = CalendarMath.Summary([Day(11, 20, 7), Day(11, 4, 7), Day(11, 12, 7)], new DateOnly(2026, 11, 1));
        Assert.Equal(new DateOnly(2026, 11, 4), s.BusiestDay);
    }
}

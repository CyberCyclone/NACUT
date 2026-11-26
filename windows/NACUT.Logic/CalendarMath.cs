using System.Globalization;

namespace Nacut.Logic;

/// <summary>A month's grid: blank cells before day 1, the day count and the weekday headers, in the culture's week order.</summary>
public sealed record MonthLayout(int Lead, int Days, IReadOnlyList<string> Headers);

/// <summary>A month's totals over days with spend.</summary>
public sealed record MonthStats(double Total, int ActiveDays, double? AveragePerActiveDay, DateOnly? BusiestDay, double BusiestCost);

/// <summary>The calendar tab's date maths (CalendarTab.swift).</summary>
public static class CalendarMath
{
    /// <summary>Every month from the first one with data through `today`'s month (just today's month without data).</summary>
    public static List<DateOnly> Months(IEnumerable<DateOnly> days, DateOnly today)
    {
        var thisMonth = new DateOnly(today.Year, today.Month, 1);
        var first = thisMonth;
        foreach (var d in days)
            if (d < first) first = d;
        var months = new List<DateOnly>();
        for (var mo = new DateOnly(first.Year, first.Month, 1); mo <= thisMonth; mo = mo.AddMonths(1)) months.Add(mo);
        return months;
    }

    /// <summary>
    /// The cost that fills the darkest color, shared by all months so they're comparable: the 95th-percentile
    /// day with spend (at least $0.01), or 1 with none.
    /// </summary>
    public static double ScaleMax(IEnumerable<double> costs)
    {
        var sorted = costs.Where(c => c > 0).Order().ToList();
        if (sorted.Count == 0) return 1;
        return Math.Max(sorted[Math.Min((int)(sorted.Count * 0.95), sorted.Count - 1)], 0.01);
    }

    public static MonthLayout Layout(DateOnly month, CultureInfo culture)
    {
        var first = new DateOnly(month.Year, month.Month, 1);
        var firstDay = (int)culture.DateTimeFormat.FirstDayOfWeek;
        var names = culture.DateTimeFormat.ShortestDayNames;
        var headers = Enumerable.Range(0, 7).Select(c => names[(firstDay + c) % 7]).ToList();
        var lead = ((int)first.DayOfWeek - firstDay + 7) % 7;
        return new MonthLayout(lead, DateTime.DaysInMonth(month.Year, month.Month), headers);
    }

    /// <summary>Grid row and column of the `dayIndex`-th day (0 = the 1st); row 0 holds the headers.</summary>
    public static (int Row, int Column) Cell(int lead, int dayIndex)
    {
        var slot = lead + dayIndex;
        return (1 + slot / 7, slot % 7);
    }

    /// <summary>Totals for `month` over the days in `costs` with spend; ties for busiest go to the earliest day.</summary>
    public static MonthStats Summary(IEnumerable<KeyValuePair<DateOnly, double>> costs, DateOnly month)
    {
        var days = costs.Where(kv => kv.Key.Year == month.Year && kv.Key.Month == month.Month && kv.Value > 0).ToList();
        if (days.Count == 0) return new MonthStats(0, 0, null, null, 0);
        var total = days.Sum(kv => kv.Value);
        var busiest = days.OrderByDescending(kv => kv.Value).ThenBy(kv => kv.Key).First();
        return new MonthStats(total, days.Count, total / days.Count, busiest.Key, busiest.Value);
    }
}

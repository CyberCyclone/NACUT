using System.Text.Json.Nodes;

namespace Nacut.Tests;

public class WidgetCardTests
{
    static WidgetLimit Limit(string id, double u) => new(id, id + " label", u, $"{Math.Round(u * 100)}%", id + " detail");

    static WidgetInput Input(bool showsLimits = true, IReadOnlyList<WidgetLimit>? limits = null, int days = 14) => new(
        ShowsLimits: showsLimits,
        Plan: "Max 5x",
        Limits: limits ?? new[] { Limit("five_hour", 0.42), Limit("seven_day", 0.75), Limit("seven_day_opus", 0.95), Limit("extra", 0.1) },
        Verdict: "On pace to stay under",
        CycleLabel: "This cycle",
        Spent: "$123.45",
        Today: "$4.20",
        Projected: "$300.00",
        Headline: "On track",
        Days: [.. Enumerable.Range(1, days).Select(i => new WidgetDay($"D{i}", $"${i}"))],
        Updated: "Updated 9:41 AM");

    static JsonObject Data(WidgetInput input, string size) => JsonNode.Parse(WidgetCard.Data(input, size))!.AsObject();
    static JsonObject Template(string size) => JsonNode.Parse(WidgetCard.Template(size))!.AsObject();
    static string? Str(JsonNode? node) => node?.GetValue<string>();

    // MARK: Bars and styles

    [Theory]
    [InlineData(0.0, 1, 100)]   // never a zero-width column
    [InlineData(0.005, 1, 100)]
    [InlineData(0.42, 42, 58)]
    [InlineData(0.5, 50, 50)]
    [InlineData(1.0, 100, 1)]
    [InlineData(1.5, 100, 1)]
    [InlineData(-0.2, 1, 100)]
    [InlineData(double.NaN, 1, 100)]
    [InlineData(double.PositiveInfinity, 100, 1)]
    public void BarWeights(double utilization, int fill, int rest) =>
        Assert.Equal((fill, rest), WidgetCard.BarWeights(utilization));

    [Theory]
    [InlineData(0.0, "accent")]
    [InlineData(0.6999, "accent")]
    [InlineData(0.7, "warning")]
    [InlineData(0.8999, "warning")]
    [InlineData(0.9, "attention")]
    [InlineData(1.2, "attention")]
    [InlineData(double.NaN, "accent")]
    public void LimitStyle(double utilization, string style) => Assert.Equal(style, WidgetCard.LimitStyle(utilization));

    // MARK: Data

    [Fact]
    public void SmallCardsShowTwoLimitsOthersThree()
    {
        Assert.Equal(2, Data(Input(), "small")["limits"]!.AsArray().Count);
        Assert.Equal(3, Data(Input(), "medium")["limits"]!.AsArray().Count);
        Assert.Equal(3, Data(Input(), "large")["limits"]!.AsArray().Count);
    }

    [Fact]
    public void LimitEntries()
    {
        var limits = Data(Input(), "large")["limits"]!.AsArray();
        var first = limits[0]!.AsObject();
        Assert.Equal("five_hour label", Str(first["label"]));
        Assert.Equal("42%", Str(first["percent"]));
        Assert.Equal(42, first["fill"]!.GetValue<int>());
        Assert.Equal(58, first["rest"]!.GetValue<int>());
        Assert.Equal("five_hour detail", Str(first["detail"]));
        Assert.Equal("accent", Str(first["color"]));
        // The weekly window is renamed; colors follow the 70% / 90% thresholds.
        Assert.Equal("Weekly", Str(limits[1]!["label"]));
        Assert.Equal("warning", Str(limits[1]!["color"]));
        Assert.Equal("attention", Str(limits[2]!["color"]));
    }

    [Fact]
    public void ShowLimitsNeedsLimitsToShow()
    {
        Assert.True(Data(Input(), "medium")["showLimits"]!.GetValue<bool>());
        Assert.False(Data(Input(showsLimits: false), "medium")["showLimits"]!.GetValue<bool>());
        Assert.False(Data(Input(limits: []), "medium")["showLimits"]!.GetValue<bool>());
        Assert.Empty(Data(Input(limits: []), "medium")["limits"]!.AsArray());
    }

    [Fact]
    public void WeekIsTheLastSevenDaysInOrder()
    {
        var week = Data(Input(), "medium")["week"]!.AsArray();
        Assert.Equal(new[] { "D8", "D9", "D10", "D11", "D12", "D13", "D14" }, week.Select(d => Str(d!["day"])));
        Assert.Equal("$14", Str(week[6]!["cost"]));
        Assert.Equal(3, Data(Input(days: 3), "medium")["week"]!.AsArray().Count);
        Assert.Empty(Data(Input(days: 0), "medium")["week"]!.AsArray());
    }

    [Fact]
    public void TextFields()
    {
        var data = Data(Input(), "large");
        Assert.Equal("Max 5x", Str(data["plan"]));
        Assert.Equal("On pace to stay under", Str(data["verdict"]));
        Assert.Equal("This cycle", Str(data["cycleLabel"]));
        Assert.Equal("$123.45", Str(data["spent"]));
        Assert.Equal("$4.20", Str(data["today"]));
        Assert.Equal("$300.00", Str(data["projected"]));
        Assert.Equal("On track", Str(data["headline"]));
        Assert.Equal("API value $123.45 this cycle · $4.20 today", Str(data["apiValue"]));
        Assert.Equal("Updated 9:41 AM", Str(data["updated"]));
    }

    [Fact]
    public void DataDoesNotDependOnTheCurrentCulture()
    {
        var saved = CultureInfo.CurrentCulture;
        try
        {
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("en-US");
            var us = WidgetCard.Data(Input(), "large");
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("de-DE");
            Assert.Equal(us, WidgetCard.Data(Input(), "large"));
        }
        finally
        {
            CultureInfo.CurrentCulture = saved;
        }
    }

    // MARK: Template

    [Theory]
    [InlineData("small")]
    [InlineData("medium")]
    [InlineData("large")]
    public void EveryCardIsAnAdaptiveCard16WithLimitAndCostViews(string size)
    {
        var card = Template(size);
        Assert.Equal("AdaptiveCard", Str(card["type"]));
        Assert.Equal("1.6", Str(card["version"]));
        Assert.Equal("http://adaptivecards.io/schemas/adaptive-card.json", Str(card["$schema"]));
        var body = card["body"]!.AsArray();
        Assert.Equal("${showLimits}", Str(body[0]!["$when"]));
        Assert.Equal("${!showLimits}", Str(body[1]!["$when"]));

        var limitRow = body[0]!["items"]!.AsArray()[1]!.AsObject();
        Assert.Equal("${limits}", Str(limitRow["$data"]));
        var bar = limitRow["items"]!.AsArray()[1]!["columns"]!.AsArray();
        Assert.Equal("${fill}", Str(bar[0]!["width"]));
        Assert.Equal("${color}", Str(bar[0]!["style"]));
        Assert.Equal("${rest}", Str(bar[1]!["width"]));
        Assert.Equal("emphasis", Str(bar[1]!["style"]));
    }

    [Fact]
    public void SmallCardIsCompactAndOpensTheAppWhenTapped()
    {
        var card = Template("small");
        var body = card["body"]!.AsArray();
        Assert.Equal(2, body.Count);
        Assert.Equal(2, body[0]!["items"]!.AsArray().Count); // title + limits
        Assert.Equal(3, body[1]!["items"]!.AsArray().Count); // cycle, spent, today
        Assert.Empty(card["actions"]!.AsArray());
        Assert.Equal("Action.Execute", Str(card["selectAction"]!["type"]));
        Assert.Equal("open", Str(card["selectAction"]!["verb"]));
    }

    [Theory]
    [InlineData("medium", 2)]
    [InlineData("large", 3)]
    public void LargerCardsAddDetailAndButtons(string size, int bodyCount)
    {
        var card = Template(size);
        var body = card["body"]!.AsArray();
        Assert.Equal(bodyCount, body.Count);
        var limitItems = body[0]!["items"]!.AsArray();
        Assert.Equal(new[] { "Plan usage · ${plan}", null, "${verdict}", "${apiValue}" }, limitItems.Select(i => Str(i!["text"])));
        var costItems = body[1]!["items"]!.AsArray();
        Assert.Equal(5, costItems.Count);
        Assert.Equal("${headline}", Str(costItems[3]!["text"]));
        var week = costItems[4]!["columns"]!.AsArray()[0]!;
        Assert.Equal("${week}", Str(week["$data"]));
        Assert.Equal(new[] { "${day}", "${cost}" }, week["items"]!.AsArray().Select(i => Str(i!["text"])));

        var actions = card["actions"]!.AsArray();
        Assert.Equal(new[] { "open", "refresh" }, actions.Select(a => Str(a!["verb"])));
        Assert.Null(card["selectAction"]);
    }

    [Fact]
    public void OnlyLargeCardsShowWhenTheyUpdated()
    {
        var last = Template("large")["body"]!.AsArray()[^1]!;
        Assert.Equal("TextBlock", Str(last["type"]));
        Assert.Equal("${updated}", Str(last["text"]));
        Assert.DoesNotContain("${updated}", WidgetCard.Template("medium"));
        Assert.DoesNotContain("${updated}", WidgetCard.Template("small"));
    }

    [Fact]
    public void TextBlocksOnlyCarryTheOptionsTheyUse()
    {
        var title = Template("medium")["body"]!.AsArray()[0]!["items"]!.AsArray()[0]!.AsObject();
        Assert.Equal(
            new[] { "type", "text", "size", "weight", "isSubtle" },
            title.Select(p => p.Key));
        Assert.True(title["isSubtle"]!.GetValue<bool>());
        Assert.Equal("Bolder", Str(title["weight"]));
    }
}

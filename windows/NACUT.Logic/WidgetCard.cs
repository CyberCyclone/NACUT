using System.Text.Json.Nodes;

namespace Nacut.Logic;

/// <summary>One plan limit for a widget card. `Percent` and `Detail` are already formatted by the core.</summary>
public sealed record WidgetLimit(string Id, string Label, double Utilization, string Percent, string Detail);

/// <summary>One day of the week strip: short day name and formatted cost.</summary>
public sealed record WidgetDay(string Day, string Cost);

/// <summary>Everything a widget card shows, already formatted (money, dates and verdicts come from the core).</summary>
public sealed record WidgetInput(
    bool ShowsLimits,
    string Plan,
    IReadOnlyList<WidgetLimit> Limits,
    string Verdict,
    string CycleLabel,
    string Spent,
    string Today,
    string Projected,
    string Headline,
    IReadOnlyList<WidgetDay> Days,
    string Updated);

/// <summary>
/// The Windows 11 Widgets board cards (small, medium, large): an Adaptive Card template and its data, showing plan
/// limits or cost like the Mac widget.
/// </summary>
public static class WidgetCard
{
    /// <summary>Adaptive Cards container style for a limit's bar.</summary>
    public static string LimitStyle(double utilization) => Palette.ForLimit(utilization) switch
    {
        LimitLevel.Critical => "attention",
        LimitLevel.Warning => "warning",
        _ => "accent",
    };

    /// <summary>
    /// Column weights for a limit's filled and empty parts, out of about 100 (the Widgets board has no progress
    /// element). Each is at least 1, since a zero weight would collapse the column.
    /// </summary>
    public static (int Fill, int Rest) BarWeights(double utilization)
    {
        var pct = double.IsNaN(utilization) ? 0 : (int)Math.Round(Math.Clamp(utilization, 0, 1) * 100);
        return (Math.Max(pct, 1), Math.Max(100 - pct, 1));
    }

    /// <summary>The card data as JSON: two limits on small cards, three otherwise, and the last 7 days.</summary>
    public static string Data(WidgetInput input, string size)
    {
        var limits = input.Limits.Take(size == "small" ? 2 : 3).Select(l =>
        {
            var (fill, rest) = BarWeights(l.Utilization);
            return (JsonNode)new JsonObject
            {
                ["label"] = Formats.LimitLabel(l.Id, l.Label),
                ["percent"] = l.Percent,
                ["fill"] = fill,
                ["rest"] = rest,
                ["detail"] = l.Detail,
                ["color"] = LimitStyle(l.Utilization),
            };
        }).ToList();
        var data = new JsonObject
        {
            ["showLimits"] = input.ShowsLimits && limits.Count > 0,
            ["plan"] = input.Plan,
            ["limits"] = new JsonArray([.. limits]),
            ["verdict"] = input.Verdict,
            ["cycleLabel"] = input.CycleLabel,
            ["spent"] = input.Spent,
            ["today"] = input.Today,
            ["projected"] = input.Projected,
            ["headline"] = input.Headline,
            ["apiValue"] = $"API value {input.Spent} this cycle · {input.Today} today",
            ["week"] = new JsonArray([.. input.Days.TakeLast(7).Select(d => (JsonNode)new JsonObject
            {
                ["day"] = d.Day,
                ["cost"] = d.Cost,
            })]),
            ["updated"] = input.Updated,
        };
        return data.ToJsonString();
    }

    /// <summary>The Adaptive Card template for `small`, `medium` or `large` (anything else is laid out like medium).</summary>
    public static string Template(string size)
    {
        var limitRow = new JsonObject
        {
            ["type"] = "Container",
            ["$data"] = "${limits}",
            ["spacing"] = "Small",
            ["items"] = new JsonArray(
                new JsonObject
                {
                    ["type"] = "ColumnSet",
                    ["columns"] = new JsonArray(
                        Column("stretch", Text("${label}", weight: "Bolder", size: "Small")),
                        Column("auto", Text("${percent}", weight: "Bolder", size: "Small"))),
                },
                new JsonObject
                {
                    ["type"] = "ColumnSet",
                    ["spacing"] = "Small",
                    ["columns"] = new JsonArray(
                        new JsonObject { ["type"] = "Column", ["width"] = "${fill}", ["style"] = "${color}", ["minHeight"] = "6px", ["items"] = new JsonArray() },
                        new JsonObject { ["type"] = "Column", ["width"] = "${rest}", ["style"] = "emphasis", ["minHeight"] = "6px", ["spacing"] = "None", ["items"] = new JsonArray() }),
                },
                Text("${detail}", size: "Small", subtle: true, spacing: "None")),
        };

        var limitsBody = new JsonArray(
            Text("Plan usage · ${plan}", size: "Small", subtle: true, weight: "Bolder"),
            limitRow);
        if (size != "small")
        {
            limitsBody.Add(Text("${verdict}", size: "Small", wrap: true));
            limitsBody.Add(Text("${apiValue}", size: "Small", subtle: true, wrap: true));
        }

        var costBody = new JsonArray(
            Text("${cycleLabel}", size: "Small", subtle: true, weight: "Bolder"),
            Text("${spent}", size: "ExtraLarge", weight: "Bolder", spacing: "None"),
            Text("Today ${today}", size: "Small"));
        if (size != "small")
        {
            costBody.Add(Text("${headline}", size: "Small", wrap: true));
            costBody.Add(new JsonObject
            {
                ["type"] = "ColumnSet",
                ["spacing"] = "Medium",
                ["columns"] = new JsonArray(new JsonObject
                {
                    ["type"] = "Column",
                    ["$data"] = "${week}",
                    ["width"] = "stretch",
                    ["items"] = new JsonArray(
                        Text("${day}", size: "Small", subtle: true, align: "Center"),
                        Text("${cost}", size: "Small", weight: "Bolder", align: "Center", spacing: "None")),
                }),
            });
        }

        var body = new JsonArray(
            new JsonObject { ["type"] = "Container", ["$when"] = "${showLimits}", ["items"] = limitsBody },
            new JsonObject { ["type"] = "Container", ["$when"] = "${!showLimits}", ["items"] = costBody });
        if (size == "large") body.Add(Text("${updated}", size: "Small", subtle: true, spacing: "Medium"));

        var card = new JsonObject
        {
            ["type"] = "AdaptiveCard",
            ["$schema"] = "http://adaptivecards.io/schemas/adaptive-card.json",
            ["version"] = "1.6",
            ["body"] = body,
            ["actions"] = size == "small" ? new JsonArray() : new JsonArray(
                new JsonObject { ["type"] = "Action.Execute", ["title"] = "Open NACUT", ["verb"] = "open" },
                new JsonObject { ["type"] = "Action.Execute", ["title"] = "Refresh", ["verb"] = "refresh" }),
        };
        if (size == "small") card["selectAction"] = new JsonObject { ["type"] = "Action.Execute", ["verb"] = "open" };
        return card.ToJsonString();
    }

    static JsonObject Text(string text, string? size = null, string? weight = null, bool subtle = false, bool wrap = false,
        string? spacing = null, string? align = null)
    {
        var o = new JsonObject { ["type"] = "TextBlock", ["text"] = text };
        if (size is not null) o["size"] = size;
        if (weight is not null) o["weight"] = weight;
        if (subtle) o["isSubtle"] = true;
        if (wrap) o["wrap"] = true;
        if (spacing is not null) o["spacing"] = spacing;
        if (align is not null) o["horizontalAlignment"] = align;
        return o;
    }

    static JsonObject Column(string width, JsonObject item) =>
        new() { ["type"] = "Column", ["width"] = width, ["items"] = new JsonArray(item) };
}

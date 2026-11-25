# NACUT

**Not Another Claude Usage Tracker.** (It is one.) A native macOS menu bar app, with a desktop widget, that tracks your Claude usage:
plan limits on a Max or Pro subscription, and cost on API pay-as-you-go.

> Not affiliated with or endorsed by Anthropic. Claude is a trademark of Anthropic.

- **Menu bar:** on a subscription, your 5-hour and weekly limits (`5h 3% · Week 15%`). On the API, today's cost and this cycle's total.
  A gauge icon fills as you approach your limit or budget. Settings → General → *Show usage as* switches a subscription to cost.
- **Panel:** limit rings with reset times and a pace forecast ("On pace for ~16% of your weekly limit by reset"), or cycle spend with a
  budget ring and forecast ("Budget runs out ~Nov 27"), plus cumulative and 30-day charts, a swipeable calendar heatmap with per-day detail, per-model costs and
  token breakdown, projects, top sessions, where you use Claude (desktop / VS Code / CLI), subagent share,
  and a weekday × hour heatmap.
- **Widget:** small / medium / large, from the macOS widget gallery.
- **Alerts:** notifications at 50/80/100% of budget, when the forecast says you'll go over, and when a plan limit passes 90%.

## Where the numbers come from

| Source | What it covers | Setup |
|---|---|---|
| Local logs (default) | Claude Code in the desktop app's Code tab, VS Code, and the CLI. Priced at Anthropic list prices. | None |
| Max/Pro plan limits | 5-hour, weekly and per-model weekly limit %, including desktop-app chats. Uses the Claude Code CLI's sign-in; when it expires (about every 8 h) the app has `claude` refresh it with a one-word Haiku prompt. | Install the [Claude Code CLI](https://claude.com/claude-code) and sign in once, then Settings → Data Sources → toggle on and allow Keychain access |
| Anthropic Usage & Cost API | Exact billed cost (after discounts) | Settings → Data Sources → paste an Admin key (Console) or a `read:analytics` key (Enterprise) |

In **Hybrid** mode, billed API data is used for every day it covers (it lags 4–24 h). Later hours
come from local logs, scaled by your recent billed ÷ list-price ratio. The Models tab shows billed vs local per day.

History is cached in `~/Library/Application Support/NACUT/`, so totals survive Claude Code's transcript cleanup.

## Build & install

Requires Xcode 26 and [XcodeGen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`).

```bash
xcodegen generate
xcodebuild -project NACUT.xcodeproj -scheme NACUT -configuration Release -derivedDataPath build/DerivedData build
cp -R build/DerivedData/Build/Products/Release/NACUT.app /Applications/
open /Applications/NACUT.app
```

Then turn on **Settings → General → Open at login**.

### On the work Mac

Clone or copy this folder, build as above, then in Settings:
1. **Budget →** choose *API pay-as-you-go* and set your monthly budget.
2. **Data Sources →** paste your analytics key and press Connect. The status line says whether your organization allows it;
   if it doesn't, the app keeps using local logs. On a shared org, choose yourself under "Count usage from".

### If the widget doesn't appear in the gallery

The app is ad-hoc signed ("Sign to Run Locally"). If macOS won't list the widget, add your Apple ID in
Xcode → Settings → Accounts (a free Personal Team is fine), set `DEVELOPMENT_TEAM` and
`CODE_SIGN_IDENTITY: "Apple Development"` in `project.yml`, and rebuild.

## Development

```bash
cd Packages/UsageCore && swift test                                   # unit tests
REAL_LOGS=1 swift test -c release --filter RealLogs                    # totals + scan timing from this Mac's logs
/Applications/NACUT.app/Contents/MacOS/NACUT --render-to /tmp/shots   # render panel tabs to PNG
```

Prices live in `Packages/UsageCore/Sources/UsageCore/Pricing/Pricing.json`; custom rates can be set in Settings → Pricing.

## License

[MIT](LICENSE) © 2026 Casey Gibson

NACUT is an independent project and is not affiliated with, endorsed by, or sponsored by Anthropic.
"Claude" and related marks are trademarks of Anthropic.
Plan limits are read from an undocumented endpoint that Claude Code uses, which may change without notice.

# NACUT

**Not Another Claude Usage Tracker.** (It is one.) A native app for **macOS, Windows and Linux** that tracks your
Claude usage from the menu bar or tray: plan limits on a Max or Pro subscription, and cost on API pay-as-you-go.

> Not affiliated with or endorsed by Anthropic. Claude is a trademark of Anthropic.

- **Menu bar / tray:** on a subscription, your 5-hour and weekly limits (`5h 3% · Week 15%`). On the API, today's cost and
  this cycle's total. On macOS that text is in the menu bar; Windows and Linux trays can't show text, so the icon shows the
  worst limit (or budget used) as a number over a gauge bar, and the tooltip has the full text.
  Settings → General → *Show usage as* switches a subscription to cost.
- **Panel:** limit rings with reset times and a pace forecast ("On pace for ~16% of your weekly limit by reset"), or cycle spend with a
  budget ring and forecast ("Budget runs out ~Nov 27"), plus cumulative and 30-day charts, a swipeable calendar heatmap with per-day detail, per-model costs and
  token breakdown, projects, top sessions, where you use Claude (desktop / VS Code / CLI), subagent share,
  and a weekday × hour heatmap.
- **Widgets:** the macOS widget gallery (small / medium / large) and the Windows 11 Widgets board (MSIX build).
- **Alerts:** notifications at 50/80/100% of budget, when the forecast says you'll go over, and when a plan limit passes 90%.
- **`nacut` CLI:** totals, limits and status-bar output (Waybar, Polybar…) on every platform.

## Where the numbers come from

| Source | What it covers | Setup |
|---|---|---|
| Local logs (default) | Claude Code in the desktop app's Code tab, VS Code, and the CLI. Priced at Anthropic list prices. | None |
| Max/Pro plan limits | 5-hour, weekly and per-model weekly limit %, including desktop-app chats. Uses the Claude Code CLI's sign-in; when it expires (about every 8 h) the app has `claude` refresh it with a one-word Haiku prompt. | Install the [Claude Code CLI](https://claude.com/claude-code) and sign in once (`claude`, then `/login`), then Settings → Data Sources → toggle on. On macOS, allow Keychain access. |
| Anthropic Usage & Cost API | Exact billed cost (after discounts) | Settings → Data Sources → paste an Admin key (Console) or a `read:analytics` key (Enterprise) |

In **Hybrid** mode, billed API data is used for every day it covers (it lags 4–24 h). Later hours
come from local logs, scaled by your recent billed ÷ list-price ratio. The Models tab shows billed vs local per day.

History is cached, so totals survive Claude Code's transcript cleanup:

| | History cache | Settings | API key |
|---|---|---|---|
| macOS | `~/Library/Application Support/NACUT/` | UserDefaults | Keychain |
| Windows | `%APPDATA%\NACUT\` | `%APPDATA%\NACUT\settings.json` | Credential Manager |
| Linux | `~/.local/share/nacut/` | `~/.config/nacut/settings.json` | Secret Service (GNOME Keyring, KWallet) |

On Windows, Claude Code installed inside **WSL** is included automatically (Settings → Data Sources → *Include Claude Code in WSL*).
Any other folder of transcripts can be added under *Local transcript folders*.

## macOS

Requires Xcode 26, [XcodeGen](https://github.com/yonaskolb/XcodeGen) and Rust (`brew install xcodegen rustup && rustup default stable`).

```bash
apple/scripts/build-core.sh                      # Rust core → apple/NacutCore (XCFramework + Swift bindings)
cd apple && xcodegen generate
xcodebuild -project NACUT.xcodeproj -scheme NACUT -configuration Release -derivedDataPath ../build/DerivedData build
cp -R ../build/DerivedData/Build/Products/Release/NACUT.app /Applications/
open /Applications/NACUT.app
```

Then turn on **Settings → General → Open at login**.

**If the widget doesn't appear in the gallery:** the app is ad-hoc signed ("Sign to Run Locally"). Add your Apple ID in
Xcode → Settings → Accounts (a free Personal Team is fine), set `DEVELOPMENT_TEAM` and
`CODE_SIGN_IDENTITY: "Apple Development"` in `project.yml`, and rebuild.

## Windows 10 / 11

WinUI 3 app for x64 and ARM64. Requires Rust, the .NET 10 SDK and Visual Studio 2022 Build Tools (C++ workload and Windows SDK).
On ARM64 PCs also install LLVM (`winget install LLVM.LLVM`) and add `C:\Program Files\LLVM\bin` to PATH; the TLS library needs clang.

```powershell
windows\scripts\build-core.ps1                   # Rust core DLL + C# bindings (uniffi-bindgen-cs)
cd windows\NACUT
dotnet build -c Release -p:Platform=x64         # or ARM64; the app is in bin\<platform>\Release\...\NACUT.exe
```

**MSIX with the Widgets board:** `windows\scripts\package.ps1 -Install` builds a signed package and installs it. Without a
code-signing certificate it creates a self-signed test certificate and trusts it on this PC (run as administrator).
Then open the Widgets board (Win + W) → **+** → **NACUT**. Unpackaged builds have everything except the widget.

New tray icons start in the overflow menu (**^**); drag NACUT onto the taskbar to keep it visible.
Windows SmartScreen warns about unsigned downloads; choose *More info → Run anyway*, or build it yourself.

## Linux (GNOME, KDE, others)

GTK4 + libadwaita app. Requires GTK 4.14+ and libadwaita 1.5+ (Ubuntu 24.04, Fedora 40, or newer) and Rust.

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libdbus-1-dev pkg-config     # Ubuntu / Debian
(cd core && cargo build --release -p nacut-cli)
cd linux && cargo build --release
cargo install cargo-deb && cargo deb --no-build && sudo apt install ./target/debian/nacut_*.deb
```

The tray icon uses StatusNotifierItem. Ubuntu shows it through its built-in AppIndicator extension; on stock GNOME install
the *AppIndicator and KStatusNotifierItem Support* extension. KDE, Cinnamon, XFCE and Waybar support it natively.
GNOME on Wayland doesn't let apps place windows, so the panel opens where the compositor puts it.

**Status bars:** `nacut status --waybar` prints Waybar JSON. For example, in `~/.config/waybar/config`:

```json
"custom/nacut": { "exec": "nacut status --waybar", "return-type": "json", "interval": 60, "on-click": "nacut-gtk" }
```

## How it's built

One shared Rust core, a native UI on each platform:

```
core/      nacut-core (parsing, pricing, forecasts, limits, API, engine), nacut-ffi (UniFFI), nacut-cli (`nacut`)
shared/    Pricing.json and test fixtures used by every platform
apple/     macOS app + widget (SwiftUI) on the core via UniFFI (Swift bindings)
windows/   Windows app + Widgets board provider (WinUI 3, C#) on the core via UniFFI (uniffi-bindgen-cs)
linux/     Linux app (GTK4 + libadwaita, Rust) on the core directly
```

| | macOS | Windows | Linux |
|---|---|---|---|
| Menu bar / tray text | ✓ | Tooltip + number icon | Tooltip + number icon |
| Panel (all four tabs) | ✓ | ✓ | ✓ |
| Widget | ✓ | ✓ (MSIX) | — (use Waybar) |
| Notifications | ✓ | ✓ | ✓ |
| Open at login | ✓ | ✓ | ✓ (XDG autostart) |
| Plan limits, API, Hybrid | ✓ | ✓ | ✓ |
| WSL transcripts | — | ✓ | — |

## Development

```bash
cd core && cargo test                     # core tests (parsing, pricing, forecast, API paging, engine)
cargo run -p nacut-cli -- summary         # totals and per-model cost from this computer's logs
cargo run -p nacut-cli -- status --waybar # status-bar JSON (Waybar, Polybar…)
```

Every app can render its views to PNGs without network or credential access, which is how the UI is checked:

```bash
/Applications/NACUT.app/Contents/MacOS/NACUT --render-to /tmp/shots --limits-json shared/fixtures/limits-live.json
NACUT.exe --render-to C:\shots --limits-json shared\fixtures\limits-live.json
nacut-gtk --render-to /tmp/shots --limits-json shared/fixtures/limits-live.json
```

Prices live in `shared/Pricing.json`; custom rates can be set in Settings → Pricing.

## License

[MIT](LICENSE) © 2026 Casey Gibson

NACUT is an independent project and is not affiliated with, endorsed by, or sponsored by Anthropic.
"Claude" and related marks are trademarks of Anthropic.
Plan limits are read from an undocumented endpoint that Claude Code uses, which may change without notice.

# Codex Meter

[简体中文](README.md) | [English](README_EN.md)

A lightweight native Rust quota widget for Windows 10/11 x64, built with Win32 and GDI. It runs without a WebView, Electron, browser, or Node.js runtime.

## Screenshot

![Codex Meter on Windows showing quota, the Token calendar, and reset cards](docs/images/codex-meter-windows.png)

An actual screenshot of the main widget and reset-card details. Quotas, Token counts, and dates reflect the account state when the screenshot was taken. The dash for today's Tokens means the server has not returned data for that date. The application currently uses a Chinese interface; this English README explains its controls.

## Download and install

1. Open the [Releases page](https://github.com/zhangyixing3/codex-meter/releases/latest) and download `CodexMeter-Windows-x64.zip`, rather than a Source code archive.
2. Extract the ZIP to a folder. Do not run the installer from inside the archive.
3. Install Codex Desktop or Codex CLI and sign in with your ChatGPT account.
4. For portable use, double-click `CodexMeter.exe`. To install it, double-click `Install.cmd`. This copies the application to `%LOCALAPPDATA%\Programs\CodexMeter` and creates desktop and Start menu shortcuts. Administrator privileges are not required.
5. Wait for the initial data load. Updates run automatically every five minutes, and you can also click Refresh.

**The release is already compiled. End users do not need Rust or a compiler.** The current release targets Windows x64; 32-bit Windows and Windows on ARM have not been validated.

To update, exit the running application from its tray menu, extract the new release, and run `Install.cmd` again.

To uninstall, exit the application and run `Uninstall.cmd` from the extracted release or `%LOCALAPPDATA%\Programs\CodexMeter`. Uninstalling removes the application, shortcuts, and its startup entry; cached data is kept by default. Run `Uninstall.ps1 -RemoveData` to also remove this application's data. The CMD launchers set the PowerShell execution policy only for their own process; they do not change the system policy.

## Usage

- The two bars show **remaining quota**, calculated as `100 - usedPercent`. Window lengths and reset times come from the server rather than fixed assumptions of five hours and seven days.
- The Token calendar covers the last 30 days. Total and daily Tokens come from the account interface. Token counts cannot be converted into quota percentages.
- Click `◇` to toggle always-on-top. The `−` and `×` buttons hide the widget in the system tray. Drag the upper-left area to move it.
- Double-click the tray icon to restore the window. Right-click the widget or tray icon for Refresh (`刷新`), always-on-top (`置顶`), launch at startup (`开机启动`), settings directory (`设置目录`), and Exit (`退出`). Startup is disabled by default.
- Automatic refresh runs every five minutes with a 25-second timeout. During a refresh, the application briefly starts the local Codex app-server and closes the helper afterward. Failures preserve the previous cache and mark it STALE.
- Clicking Refresh immediately shows a busy state. The completion timestamp includes seconds. Repeated clicks during a refresh do not start duplicate requests. Click the bottom status to inspect a failure.
- The quota-reset summary (`使用限额重置`) shows the available count and nearest expiration. Click it to view each card's type, status, expiration, and server-provided description. Dates explicitly use the Windows local time zone. Viewing details does not consume a reset card.
- Reset cards appear in one column, sorted by expiration. All returned cards are shown without scrolling or pagination. The window adjusts its height to the card count and scales to fit the screen. Drag to move it; press Esc or click Close to dismiss it. Details update with refreshed data.
- The widget and details panel share a gray-blue to dark-green background, teal accents, and teal-to-yellow quota bars.

## Troubleshooting

- **Today's Tokens show `—`:** the server has not returned statistics matching today's Windows local date, or the installed Codex version does not support the interface. A dash means unknown, not zero.
- **Refresh fails / STALE:** click the bottom status for details. Check your network connection, Codex sign-in, and Codex version. Cached values are not current server readings.
- **Codex cannot be found:** configure the path as described below. Select the actual `codex.exe`, not a `.cmd` launcher.
- **The application stays running after closing the window:** the close button hides it in the tray. Choose Exit (`退出`) from the tray menu to fully stop it.
- **Can I use a reset card here?** This application only displays card information. Use the official Codex interface to apply a reset.

## Data and privacy

Codex Meter calls `initialize`, `account/rateLimits/read`, and `account/usage/read` through the local Codex stdio JSON-RPC app-server. It uses Codex-managed sign-in and does not read or copy login tokens, start model conversations, or request quota resets.

See the [OpenAI App Server documentation](https://learn.chatgpt.com/docs/app-server). Interface availability can change between Codex versions. If an older version lacks the Token interface, the widget shows unavailable data instead of inventing values.

Settings and cached aggregates are stored in `%LOCALAPPDATA%\CodexMeter`. The cache contains quota values, dates, Token totals, and general status, without conversation text or credentials. Quota and Token readings have independent timestamps. When switching accounts, exit the application, remove `snapshot.json` from that directory, and restart to avoid briefly seeing the old cache.

Missing or null values appear as `—` or gray calendar cells. Only an explicitly returned zero is treated as zero. Calendar dates follow the server's `startDate`, and today's value is matched against the Windows local date. The server's date boundaries may differ from local midnight. Heatmap colors are normalized within the displayed 30-day period.

If Codex cannot be found automatically, create `config.json` in the settings directory:

```json
{"codex_path":"C:\\path\\to\\codex.exe"}
```

Alternatively, set `CODEX_METER_CODEX_PATH`. The application requires a native `.exe`; for npm installations it attempts to locate the bundled native binary. Sign in to Codex with a ChatGPT account to read subscription quota; API-key usage generally does not provide this quota.

## Build and verification

Developers need Rust and a Windows linker: MSVC Build Tools or GNU MinGW.

```powershell
cargo test --locked
cargo build --release --locked
.\scripts\Package.ps1
```

`scripts/Build.ps1` runs tests, builds the release, and packages it. It defaults to `%LOCALAPPDATA%\CodexMeterBuild` to avoid older GNU linkers' issues with non-ASCII build-output paths. For a GNU Rust toolchain, it can reuse MinGW from an existing `miniconda3\envs\r_4.3` installation under the user profile, when present. Use `-MingwBin` for another MinGW directory and `-TargetDir` for another output directory. If packaging a manually built release in a custom target directory, pass that directory to `Package.ps1 -TargetDir`.

Developer commands:

- `cargo run`: run a debug build.
- `--demo`: show explicitly labeled demonstration data.
- `--check`: read the actual account and output sanitized JSON; redirect or pipe output for this GUI executable.
- `--hidden`: start in the system tray.
- `--preview path.bmp`: render an offscreen development preview using the same drawing code as the window; can be combined with `--demo`.

`scripts/Smoke.ps1` checks tray startup, restore, reset details, always-on-top, hiding, single-instance behavior, and exit. It reports visible/hidden working set and private memory. Exit any running instance before using it. See [VERIFICATION.md](VERIFICATION.md) for the development-machine validation record in Chinese. Installation has not yet been tested on a separate clean Windows machine.

Memory use depends on the machine and runtime state. Hiding in the tray releases reclaimable resident pages, reducing the working set; this does not imply an equivalent reduction in private committed memory. Codex helper-process overhead during refresh must be counted separately. The current application supports one account at a time.

## License

Released under the [MIT License](LICENSE). You may use, modify, and redistribute the software while retaining its copyright and license notice.

## Acknowledgments

Thanks to [Cartmancxx/codex-agent-usage-wallpaper](https://github.com/Cartmancxx/codex-agent-usage-wallpaper) for the inspiration. Its idea of showing remaining quota and Token usage directly on the desktop inspired this project's desktop-card design and experience.

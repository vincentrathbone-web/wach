# WACH — handover

Notes for whoever picks this up next (a person or a new Claude session). Short on purpose: the plan and progress table are in [PLAN.md](PLAN.md), the look in [DESIGN.md](DESIGN.md).

Last updated: 2026-11-17 (after phase 5)

## Where we are

| Phase | State |
|---|---|
| Research, plan, design | Done |
| 0 · Setup | Done |
| 1 · Two accounts side by side | Done — 2 manual checks left (need a phone to scan QR codes) |
| 2 · Shell & split view | Done |
| 3 · Inside WhatsApp | Done — real-notification check needs a phone |
| 4 · Windows integration | Done |
| 5 · Installer | Done — installed on this PC; clean-user test left |

## How to build, run and test

- Rust: `CARGO_HOME=D:\Dev\toolshed\cargo`, `RUSTUP_HOME=D:\Dev\toolshed\rustup` (user env vars; a fresh shell may need `D:\Dev\toolshed\cargo\bin` on PATH). The repo pins the **MSVC** toolchain; the machine default stays GNU.
- Tauri CLI: `D:\Dev\toolshed\tauri-cli\node_modules\.bin\tauri.cmd`. NSIS: `D:\Dev\toolshed\nsis`.
- Build: `cd src-tauri; cargo build` → `target\debug\wach.exe`.
- Unit + token tests: `cd src-tauri; cargo test` (rules, config, shortcuts, toast text, and `tests/tokens.rs` which fails on raw colours/sizes outside `ui/tokens.css`).
- UI self-test (debug build only): set `WACH_SELFTEST=1`, start `wach.exe`, read stderr. It drives split/single/swap/shortcuts/sheets/divider/add/rename/remove and checks where Rust really placed each webview. It also fakes WhatsApp events through the debug-only `debug_simulate` command (unread, toast, zoom, crashes). Last run: **all 30 checks pass**.
- **Only one copy runs** (single-instance). Kill `wach.exe` before rebuilding, or the build fails with "Access is denied" and a new start just focuses the old copy.
- Installer: kill `wach.exe`, add `D:\Dev\toolshed\nsis` to PATH, run `tauri.cmd build` from `src-tauri` → `target\release\bundle\nsis\WACH_0.1.0_x64-setup.exe` (~1.2 MB, ~5 min). `/S` installs silently. Program goes to `%LOCALAPPDATA%\WACH\`.
- App data: `%LOCALAPPDATA%\app.wach.desktop\` (moved from `%LOCALAPPDATA%\WACH\` on first start, see `move_old_data`) → `config.json`, `closed.json`, `accounts\<id>\` (one WebView2 folder per account), `ui\` (our interface's own WebView2 folder), `wach.png` (toast icon).
- On this PC: the installed release build, no accounts yet (the 3 fake test accounts were removed so the user can add real ones). The self-test needs 3 accounts — add test ones first when running it.

## Code map

- `src-tauri/src/lib.rs` — app state, all UI commands (only the `ui` webview may call them), start-up, window events.
- `accounts.rs` — creates one account webview (own data folder, navigation lock, new-window block, permissions, title → unread), parks/places it.
- `hooks.rs` — WebView2 via `with_webview`: native notifications → toast, context-menu clean-up, crash → reload, offline page, our shortcuts inside WhatsApp panes, focus tracking.
- `toast.rs`, `tray.rs`, `platform.rs` (open in browser, AppUserModelID), `shortcuts.rs`, `rules.rs` + `config.rs` (pure, unit tested).
- `ui/` — `tokens.css` (all design values), `app.css` (only `var(--…)`), `app.js`, `selftest.js`, `index.html`.

## What works (verified)

- Two WhatsApp Web logins side by side, each with its own QR code → storage really separate.
- Panes follow window resize; below 700 px wide it switches to one pane and back.
- Single/Split, swap, Ctrl+Tab, Ctrl+1–9, divider ratio, sheets park the webviews, add/rename/remove account, input validation, layout saved.
- Removed account's folder is deleted at the next start (`closed.json`).
- Token rule enforced by a test (it caught the old placeholder stylesheet).
- Unread from the page title → rail badge + status bar total; test toast shows; Ctrl+= / Ctrl+0 zoom per account and is saved.
- Page crash → marked + reloaded; whole browser crash → webview rebuilt in the same pane, other pane untouched; 4th crash within a minute is not reloaded.
- Installer: silent install, Start-menu shortcut with AUMID `app.wach.desktop`, installed app runs, silent uninstall removes program/shortcut/entry and keeps account data, reinstall.
- Close → hides to tray; starting WACH again brings the same copy back; with tray off, close quits and saves the window; a saved position off every screen is moved back on screen.

## Still to do / open

- **Needs the user:** scan QR codes, then check (a) both stay logged in after restart, (b) a message to a parked account (not in a pane) shows a toast within 10 s and clicking it opens the right chat.
- Not tested yet: the offline page (needs the network cut), tray tooltip/dot and taskbar dot checked by eye only.
- Installer: test on a clean Windows user; tick "delete app data" on uninstall once by hand (should remove `%LOCALAPPDATA%\app.wach.desktop`, checked by reading Tauri's NSIS script only).
- `ui/selftest.js` ships inside the release build (harmless: the debug commands it needs don't exist there). Could be left out later.
- An empty `EBWebView` folder appears in the data root (WebView2 default folder from the main window); harmless.
- Fable 5.2 review of PLAN.md (was blocked by usage credits).
- Optional: shared WebView2 profiles (Option B) to cut memory — see Issues.

## Issues found

- `channel = "stable"` in `rust-toolchain.toml` picked the GNU host → `windres` failed. Fixed by naming `stable-x86_64-pc-windows-msvc`.
- **Memory:** ~370 MB private per account (each data folder = its own browser, GPU and utility processes) + ~150 MB for our UI webview. Over the plan's 150 MB line. Option B (one environment, a WebView2 *profile* per account) would share those processes, but Tauri 2.12 does not pass `profile_name` through to wry (wry 0.57 has `with_profile_name`). Needs a small Tauri patch or upstream PR.
- WhatsApp Web shows a horizontal scrollbar when a pane is under ~600 px wide. Ctrl+− per account helps; maybe default split panes to 90 % zoom later.
- The WhatsApp page's square corners sit over our pane's rounded bottom corners (cosmetic).
- **Task Manager** lists WACH's WebView2 processes as separate "WebView2 Manager" / "Utility" rows, not under WACH. That is how current Windows 11 shows every WebView2 app (Outlook and Search look the same here); the processes really are children of `wach.exe`. Tested: dropping our process AppUserModelID changes nothing. Each account's own data folder = its own process tree, so more accounts = more rows. Option B (shared profiles) would merge them into one tree.
- Snap Layouts on the maximise button are not available with our custom title bar (known gap).

## Lessons learnt

- Tauri 2.12 exposes most of what we need on the webview builder (`on_permission_request`, `on_document_title_changed`, `on_new_window`, `on_navigation`, `on_download`, `data_directory`). Lower-level WebView2 bits go through `with_webview` + `webview2-com` 0.39 (same version as Tauri).
- WebView2's native `NotificationReceived` event means **no script in WhatsApp's page and no IPC from the remote page at all**.
- Native child webviews always draw above our HTML. Menus/dialogs therefore "park" the WhatsApp panes while open.
- Creating a webview inside a normal (sync) Tauri command deadlocks on Windows: it runs on the main thread and waits for itself. Create from a background thread.
- Webviews may finish starting after the UI has laid out → Rust remembers the last pane rectangles and applies them when a webview appears.
- `PrintWindow` screenshots do include WebView2 content; listing child windows (`WRY_WEBVIEW`) with their rectangles is a quick way to check placement.
- Tauri's NSIS installs to `%LOCALAPPDATA%\<productName>` but its "delete app data" box removes `%LOCALAPPDATA%\<identifier>`. Keep app data under the identifier (`app_local_data_dir()`), never under the product name.
- Window position is saved only on exit (`ExitRequested`), so `Stop-Process` loses a move. To test it, turn tray off and send WM_CLOSE.
- `element.replaceChildren(x ? node : null)` prints the word "null" — filter children first (our `h()` helper already does).
- PowerShell `Set-Content` on Rust files can change line endings; prefer the editor tools for code edits.

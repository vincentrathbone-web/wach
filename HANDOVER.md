# WACH — handover

Notes for whoever picks this up next (a person or a new Claude session). Short on purpose: the plan and progress table are in [PLAN.md](PLAN.md), the look in [DESIGN.md](DESIGN.md).

Last updated: 2026-11-18 (0.2.0 released and installed; updater key copied into the repo folder, git-ignored)

## Drag-and-drop, Save-as downloads, calls (released in v0.3.1, 2026-11-25)

- v0.3.1 published (installer, .sig, latest.json); the update feed answers 0.3.1. The user updated to 0.3.1 in-app; with the box ticked for Personal, **voice calls work** on Personal (confirmed by the user, 2026-11-25). Drag-and-drop into chats and Save-as downloads also confirmed working by the user on the real accounts.

- **Drag and drop** of files into a chat: Tauri's drop handler swallowed the drop; account webviews now use `.disable_drag_drop_handler()` (accounts.rs). Needs a real check on a logged-in chat.
- **Downloads**: no `on_download` on the builder any more. `hooks.rs` takes WebView2 `DownloadStarting` (`ICoreWebView2_4`), hides Edge's bubble, takes a deferral; `downloads.rs` opens Windows' Save-as window (`IFileSaveDialog`, owner = main window) after the event returns, then sets `ResultFilePath` or cancels. When done: toast "Saved <name>" (click = open the file, button "Show in folder"); a failed download (not user-cancelled) gives "Couldn't save". Tested in a debug copy: dialog shows with name + type + Downloads, file lands where picked, toast shown with both buttons (toast clicks not tested — synthetic clicks don't work).
- Debug-only test hook: `WACH_TEST_DOWNLOAD=1` starts a tiny blob download in the first account 20 s after start (also `debug_simulate` kind `download`).
- **Voice calls on Personal**: not a WACH block. The user confirmed plain Edge on web.whatsapp.com shows the same (video button only → "download the app"). Investigated the official Windows app (`_wa-analysis\2.2644`, string search only): it is a WebView2 wrapper, but it injects ~20 WinRT host objects into the page (`WinRTAdapter.*Bridge`: `VoipBridge`, `VoipSignalingBridge` with `SendVoipSignalingXmpp`, `AbPropsBridge`, `SQLiteBridge`, `ClientKeyBridge`, …). Inside the app the web client hands calls to Meta's native VoIP engine (`WhatsApp.VoIP.dll` → native `WhatsAppNative.Voip`), not to browser WebRTC. In a plain browser, calling depends on the server flag `enableWebCalling` (seen in `WhatsApp.AbProps.dll`), switched on per account (on for Business, not yet for Personal). Copying the bridge would mean shipping/driving Meta's private engine and posing as the official client; forcing the flag would mean changing WhatsApp's page. Both break WhatsApp's terms (ban risk) and WACH's "no code in WhatsApp's page" rule → not done by default.
- **Opt-in "Experimental: force voice & video calls"** (per account, off by default, warning in the account sheet). The user tested the idea in Edge first: Personal could call. `Account.force_calling` → `accounts.rs` adds `src-tauri/src/force_calling.js` as a Tauri `initialization_script` (main frame, web.whatsapp.com only). It wraps `window.require('WAWebABProps').getABPropConfigValue` so `enable_web_calling` and `web_voip_call_tab_new_call` return true. **This is now the only script WACH puts into WhatsApp's page**, and only for accounts with the box ticked. Changing the box rebuilds that account's webview (`recreate_webview`). If WhatsApp renames the module or keys, the script just does nothing.
- Tested in a debug copy (3 test accounts, QR page): `WACH_TEST_CALLING=1` reports through the page title → account with the box: `patched=true web_calling=true`; account without: `patched=false web_calling=false`. Self-test: ALL PASS. Not tested: the toggle click in the UI (synthetic clicks don't reach WACH) and a real call inside WACH (needs the user).
- Debug builds now print each account's page title to stderr (only ever run on test data).

## Themes (released in v0.3.0, 2026-11-18)

- 5 new themes + Night Signal: Glass, Daylight, Graphite, Ember, High Contrast. Approved by the user from the preview artifact https://claude.ai/artifact/Wf8wUESa5fGRgpm8M8yhgR ("All five"); picker = **theme button just above Settings** in the rail (user's choice).
- How it works: see DESIGN.md §7 (theme blocks in `tokens.css`, `data-theme` on `<html>`, `rules::THEMES`/`theme_spec`, `set_settings { theme }`, `apply_theme` in lib.rs for Acrylic, webview background and tray dot).
- **The main window is now always created transparent** (for Glass). Watch for: a flash of desktop at the edges while resizing fast, or anything odd with the window shadow. Not seen in testing.
- Glass see-through needs Windows 11 (`platform::glass_supported`, build ≥ 22000); otherwise solid (`data-solid`).
- Tested: unit tests (theme list ↔ tokens.css), self-test 51/51 (10 new theme checks), screenshots of all six themes and of the picker in the real app.
- Self-test can leave the theme picker open: `WACH_SELFTEST_SHEET=theme`.

## Release state

- Public repo: https://github.com/vincentrathbone-web/wach (MIT). Release **v0.2.0** with installer, `.sig` and `latest.json`; the update feed URL answers 200.
- Installed on the user's PC: 0.2.0, both real accounts logged in (Personal, Business), checked by screenshot.
- **v0.2.1 released** (2026-11-18, no app changes, to test the updater). The installed 0.2.0 found it on its own ~30 s after start: status bar showed "Update 0.2.1 ready · Restart". Clicking it (download → passive install → restart) is left to the user, since synthetic clicks don't reach WACH.
- **v0.2.2 released** (2026-11-18): update chip moved to the title bar, right of the logo. The installed 0.2.0 found it ("Update 0.2.2 ready · Restart"); the user clicked it and it **updated fine** (installed version now 0.2.2, WACH restarted). **The full self-update path works.**
- Updater key: two copies on this PC, `%USERPROFILE%\.tauri\wach-updater.key` (what `release.ps1` uses) and `wach-updater.key` (+ `.pub`) in the repo folder, git-ignored by `*.key` / `*.key.pub` (checked with `git check-ignore`). The user should also keep one off this PC.

### Login incident during the 0.2.0 install (read this before touching profile folders)

- After installing 0.2.0 both accounts showed the QR code. The folder move itself worked (data intact, path now max 231 chars).
- Cause found: the Option B profiles (`profiles\…\WV2Profile_<hex>`) **never held a login on disk** — no `last-wid-md` / `WALid` in their Local Storage, and the IndexedDB stopped changing ~15 min after the user scanned. The backup taken right before the install is the same. Why the Option B login didn't stick is **not known** (maybe the session was dropped by WhatsApp while we kept restarting/killing WACH in tests). Watch for it: if a login disappears again, check `Local Storage\leveldb` for `last-wid-md` first.
- Fix used: the older one-folder-per-account data (`accounts\<id>\EBWebView\Default`, still logged in) copied into `wv\EBWebView\WV2Profile_<base32>`. A Default folder works as a profile folder as is. Tested in `D:\wtest5` first, including a forced close + restart, then copied into the real data.
- Cleaned up on 2026-11-18 at the user's request: the pre-install backup, the QR-only profiles, the old `accounts\` folder and the test folders `D:\wtest`, `D:\wtest3`–`D:\wtest5` are all deleted. **The live `wv\` profiles are now the only copy of the logins** — if they break, the user scans the QR codes again.
- Never keep a second running copy of a live login (two copies of one linked device can get it logged out).
- Quick way to tell if a WhatsApp profile is logged in: `Select-String -SimpleMatch last-wid-md` over `<profile>\Local Storage\leveldb\*.log`.

## 0.2.0 — what's new

- **Start with Windows**: `HKCU\…\Run` value `WACH` = `"<exe>" --hidden` (no plugin). `--hidden` keeps the window in the tray; accounts still load. Off by default; Settings + tray toggle. Turning it off only removes an entry that points at *this* exe.
- **Tray menu** (rebuilt on every unread/setting change): accounts with unread → click opens; Mute (1 hour / until on / unmute, stored as `settings.muted_until`); Start with Windows; Check for updates / Install update; Show; Quit.
- **Updates**: `tauri-plugin-updater` from Rust only (no JS permission). Endpoint `https://github.com/vincentrathbone-web/wach/releases/latest/download/latest.json`; public key in `tauri.conf.json`; private key `%USERPROFILE%\.tauri\wach-updater.key`, copy in the repo folder (no password, **git-ignored, never commit it; lose it and installed copies can't update**). Checks 30 s after start and every 6 h (release builds only); chip "Update x ready · Restart" (title bar, right of the logo, since the 0.2.2 code; status bar in 0.2.0/0.2.1); installs only on click (NSIS passive, WACH restarts).
- **Releases**: `scripts\release.ps1 -Version x.y.z -Notes "…"` builds + signs into `release\x.y.z\` (installer, .sig, latest.json). Add `-Publish` to commit, tag, push and create the GitHub release.
- **Shorter data paths**: `profiles\` → `wv\`, profile names hex (32) → base32 (20). Chromium's cache paths reached 249 of Windows' 260 chars here; past 260 WhatsApp shows "A database error occurred on your browser". Old folders are moved on start (`Store::migrate_profiles`, retries ~10 s while the old WebView2 lets go); Chromium then re-creates an empty stub of the "last used" old profile, which is deleted on the next start only if it is newer than the moved one.

## Option B (merged in 0.1.x)

All accounts share one WebView2 data folder (`wv\`) = one browser process tree; each account is a WebView2 **profile** (`wv\EBWebView\WV2Profile_<base32 of id>`). Needs our vendored `src-tauri/patches/tauri-runtime-wry` (search "WACH patch": `data_store_identifier` → profile name, lowercase because WebView2 lowercases profile folder names).

Measured, 3 test accounts on the QR page + WACH's UI, ~40 s after start:

| | Browser trees | Processes | Private memory |
|---|---|---|---|
| main (one folder per account) | 4 | 29 | 1306 MB |
| this branch (profiles) | 2 | 18 | 1151 MB |

Saves ~150 MB with 3 accounts and fewer Task Manager rows. Trade-offs: a crash of the shared browser process takes all accounts down at once (each reloads on its own); switching meant scanning QR codes again (old `accounts\<id>` folders were left alone; deleted on the user's PC since). Debug builds also accept `WACH_DATA_DIR` to test in a separate folder. Self-test (now 40/40), unit tests pass, cleanup of removed profiles tested.

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
| 6 · Updates, tray, autostart (0.2.0) | Done — public repo, v0.2.0 → v0.2.2; self-update from 0.2.0 to 0.2.2 worked on the user's PC |
| 7 · Themes (0.3.0) | Released v0.3.0; installed 0.2.2 offers it |

## How to build, run and test

- Rust: `CARGO_HOME=D:\Dev\toolshed\cargo`, `RUSTUP_HOME=D:\Dev\toolshed\rustup` (user env vars; a fresh shell may need `D:\Dev\toolshed\cargo\bin` on PATH). The repo pins the **MSVC** toolchain; the machine default stays GNU.
- Tauri CLI: `D:\Dev\toolshed\tauri-cli\node_modules\.bin\tauri.cmd`. NSIS: `D:\Dev\toolshed\nsis`.
- Build: `cd src-tauri; cargo build` → `target\debug\wach.exe`.
- Unit + token tests: `cd src-tauri; cargo test` (rules, config, shortcuts, toast text, and `tests/tokens.rs` which fails on raw colours/sizes outside `ui/tokens.css`).
- UI self-test (debug build only): set `WACH_SELFTEST=1`, start `wach.exe`, read stderr. It drives split/single/swap/shortcuts/sheets/divider/add/rename/remove and checks where Rust really placed each webview. It also fakes WhatsApp events through the debug-only `debug_simulate` command (unread, toast, zoom, crashes), and checks mute, autostart and the update check. It leaves Settings open at the end for a screenshot. Last run: **all 51 checks pass** (new: theme button placement, each theme applies + saves, unknown theme rejected, picker shows 6 different previews and parks the webviews). It resets the theme to Night Signal first. `WACH_SELFTEST_SHEET=theme` leaves the theme picker open instead of Settings. Run it with `WACH_DATA_DIR` set to a **short** folder (e.g. `D:\wtest3`, with a `config.json` holding 3 test accounts `TestWork0001`, `TestHome0002`, `TestFam00003`) — a deep folder pushes WebView2 paths past 260 chars and WhatsApp's database breaks.
- **Only one copy runs** (single-instance). Kill `wach.exe` before rebuilding, or the build fails with "Access is denied" and a new start just focuses the old copy.
- Installer / release: quit WACH, then `scripts\release.ps1 -Version x.y.z` (builds + signs, ~5 min) → `release\x.y.z\WACH_x.y.z_x64-setup.exe` (~2 MB) + `.sig` + `latest.json`. `/S` installs silently. Program goes to `%LOCALAPPDATA%\WACH\`.
- App data: `%LOCALAPPDATA%\app.wach.desktop\` (moved from `%LOCALAPPDATA%\WACH\` on first start, see `move_old_data`) → `config.json`, `closed.json`, `wv\` (the shared WebView2 folder, one profile per account; old `accounts\<id>\` folders from 0.1 may still be there on other PCs), `ui\` (our interface's own WebView2 folder), `wach.png` (toast icon).
- On this PC: installed 0.2.0 with the user's two real accounts. **Never run test builds on the real data folder** — debug builds must use `WACH_DATA_DIR` (release builds ignore it, so don't test release builds while the real copy holds logins). The self-test needs 3 accounts — add test ones first when running it.
- Saved window position can be `-60000,-60000` after a hidden start; WACH opens maximised/on screen anyway (minor, worth a look).

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

- **Needs the user:** (a) both stay logged in after restart: checked on 0.2.0 (also after a forced close), but see the login incident above; (b) a message to a parked account (not in a pane) shows a toast within 10 s and clicking it opens the right chat.
- Not tested yet: the offline page (needs the network cut), tray tooltip/dot and taskbar dot checked by eye only.
- Installer: test on a clean Windows user; tick "delete app data" on uninstall once by hand (should remove `%LOCALAPPDATA%\app.wach.desktop`, checked by reading Tauri's NSIS script only).
- `ui/selftest.js` ships inside the release build (harmless: the debug commands it needs don't exist there). Could be left out later.
- An empty `EBWebView` folder appears in the data root (WebView2 default folder from the main window); harmless.
- Fable 5.2 review of PLAN.md (was blocked by usage credits).
- Checked: installed 0.2.2 shows "Update 0.3.0 ready · Restart" in the title bar, right of the logo. Installing 0.3.0 by click is up to the user; then check the theme button and Glass on the real logins.

## Issues found

- `channel = "stable"` in `rust-toolchain.toml` picked the GNU host → `windres` failed. Fixed by naming `stable-x86_64-pc-windows-msvc`.
- **Memory:** ~370 MB private per account (each data folder = its own browser, GPU and utility processes) + ~150 MB for our UI webview. Over the plan's 150 MB line. **Solved with Option B** (one environment, a WebView2 *profile* per account) via our vendored `tauri-runtime-wry` patch. An upstream Tauri PR would let us drop the patch.
- WhatsApp Web shows a horizontal scrollbar when a pane is under ~600 px wide. Ctrl+− per account helps; maybe default split panes to 90 % zoom later.
- The WhatsApp page's square corners sit over our pane's rounded bottom corners (cosmetic).
- **Task Manager** lists WACH's WebView2 processes as separate "WebView2 Manager" / "Utility" rows, not under WACH. That is how current Windows 11 shows every WebView2 app (Outlook and Search look the same here); the processes really are children of `wach.exe`. Tested: dropping our process AppUserModelID changes nothing. With Option B all accounts share one process tree, so the row count no longer grows per account.
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
- **Rust temporaries in a struct literal live until the whole literal is built.** `StateDto { config: cfg.lock().unwrap().clone(), settings: settings_dto() /* locks cfg again */ }` deadlocked the main thread — and only showed up as a freeze while an account webview was starting (the UI's `get_state` runs re-entrantly inside WebView2's message pump). Take locks in their own `let` first.
- Synthetic mouse clicks (`mouse_event`) don't reach WACH's window reliably; drive the UI through the self-test API instead.
- `element.replaceChildren(x ? node : null)` prints the word "null" — filter children first (our `h()` helper already does).
- PowerShell `Set-Content` on Rust files can change line endings; prefer the editor tools for code edits.

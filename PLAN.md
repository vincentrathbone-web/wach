# WACH — build plan

A small Windows app that shows several WhatsApp Web accounts side by side, each fully separate, without Electron.

Status: 0.2.0 released · 2026-11-18 · look and layout in [DESIGN.md](DESIGN.md) · session notes in [HANDOVER.md](HANDOVER.md)

## Progress

| Milestone | State | Notes |
|---|---|---|
| 0 · Setup | ✅ Done | Tauri 2.12, MSVC toolchain pinned |
| 1 · Spike | ✅ Done (1 manual check left) | Two accounts side by side, separate storage, resize follows. **Measured ~370 MB private per account** (7 WebView2 processes each) → over the 150 MB line. Option B (profiles) now built with a vendored Tauri patch (`with_profile_name`): ~150 MB less with 3 accounts. Stay-logged-in after restart checked with real accounts on 0.2.0. Manual left: notification from a parked account |
| 2 · Shell & split view | ✅ Done | Tokens + token check test, title bar, rail, panes, divider, sheets, shortcuts, config, cleanup. UI self-test: all pass |
| 3 · Inside WhatsApp | ✅ Done (1 manual check left) | Native WebView2 events instead of a page script (see 4.4/4.5). Self-test (simulated events): unread from title → rail/status, toast, zoom keys, renderer crash reload, browser crash recreate, crash limit. Not auto-tested: offline page, real WhatsApp notification |
| 4 · Windows bits | ✅ Done | Close-to-tray, second start focuses first copy, window position saved + off-screen position corrected (tested by script). Tray tooltip/icon dot + taskbar dot run in the self-test without error (looked at by eye only) |
| 5 · Installer | ✅ Done (clean-user test left) | `tauri build` → per-user NSIS setup (1.23 MB in 0.1, ~2 MB in 0.2.0 with the updater). Tested: silent install, Start-menu shortcut carries AUMID `app.wach.desktop`, installed app starts and shows accounts, silent uninstall removes program + shortcut + entry and keeps data, reinstall. Data moved to `%LOCALAPPDATA%\app.wach.desktop` so it never mixes with the install folder |
| 6 · Updates, tray, autostart (0.2.0) | ✅ Released: public repo github.com/vincentrathbone-web/wach (MIT), release v0.2.0, installed on the user's PC with both logins. v0.2.1 and v0.2.2 released (0.2.2 moves the update chip to the title bar); the installed 0.2.0 found 0.2.2 and updated itself on click |
| 7 · Themes (0.3) | ✅ Released as v0.3.0 | 5 new themes (Glass with Windows 11 Acrylic, Daylight, Graphite, Ember, High Contrast) approved from a preview artifact; theme button above Settings with live mini previews; instant switch, saved in config; tray dot + window colour per theme. Unit tests + self-test 51/51 + screenshots of every theme | Signed in-app updates from GitHub Releases (ask, then install), tray menu (accounts with unread, mute all, start with Windows, check for updates), start with Windows hidden in the tray, `scripts\release.ps1`. Also: shorter WebView2 paths (`wv\`, base32 profile names) after hitting the 260-char limit. Self-test 40/40 |

**Decisions made while building**
- **No page script.** Unread comes from Tauri's `on_document_title_changed`; notifications come from WebView2's native `NotificationReceived` event. WhatsApp's page gets no injected code and no IPC capability at all.
- **Inactive accounts are parked** off-window (never hidden), so WebView2 keeps them "visible".
- **Account webviews are created off the main thread** — creating one from a sync command on the main thread deadlocks.

## 1. Goal and scope

**Must have (v1)**
- One entry per WhatsApp account in a left rail; add, rename, close, switch, reorder; remembered between runs.
- **Split view:** two accounts side by side in two vertical panes (like a two-pane file manager), with Single ↔ Split, swap, focus and a resizable divider. Layout "Rail", see DESIGN.md §2.
- **Tokenized design:** every colour, radius, size, spacing, shadow/glow, font and timing comes from tokens in `ui/tokens.css`, so the app can be re-themed by changing one file (DESIGN.md §7).
- Each account fully isolated (its own cookies, IndexedDB, service workers), and stays logged in after a restart.
- Notifications that name the account; clicking one opens the right tab.
- Unread count per tab, total on the tray icon and as a taskbar overlay dot.
- Tray icon (show/hide, quit), close-to-tray, one running copy only.
- Links that leave `web.whatsapp.com` open in the default browser.
- Microphone/camera and notification permissions per account; everything else refused.
- A crashed account reloads on its own.
- A Windows installer.

**Added in 0.2.0:** in-app updates, start with Windows, mute all notifications.

**Themes (0.3.0, released):** Glass (Windows 11 Acrylic), Daylight, Graphite, Ember, High Contrast next to Night Signal; theme button above Settings in the rail. See DESIGN.md §7.

**Not in v1** (may come later): per-account mute, translations, `whatsapp://` links, Enter-key options, resource tools, migrating Altus + accounts (users rescan QR codes once).

**Non-goals, on purpose**
- Never pretend to be the official WhatsApp app: no native "bridges", no `X-WA-WebView2-Version` header, no fake user agent. We are a plain browser showing WhatsApp Web.
- Never suspend or throttle an account in the background: the official app can only do that because WhatsApp's servers wake it with Windows push messages, which we do not get. A paused tab would miss messages.

## 2. Stack

- **Tauri 2** (Rust) on the system **WebView2** runtime (already installed: 157.x, updated by Windows).
- **UI: plain HTML/CSS/JavaScript, no bundler and no npm.** Tauri serves a static folder. This keeps the dependency count to the Rust crates only.
- Rust crates beyond Tauri: `webview2-com` (low-level WebView2 calls Tauri does not expose), `windows` (taskbar, registry), plus Tauri plugins `single-instance` and a toast library (see 4.5). Each extra crate must be justified in review.

## 3. Architecture

```
Main window (Tauri, no system frame)
├── "ui" webview: our own chrome, full window, behind the accounts
│     title bar 40 px · rail 72 px · pane headers 44 px · status bar 28 px
└── one webview per account
     ├── 0, 1 or 2 placed in the pane rectangles (Single / Split)
     ├── the rest kept alive but out of view (see below)
     ├── url: https://web.whatsapp.com/
     ├── own WebView2 profile: %LOCALAPPDATA%\app.wach.desktop\wv\EBWebView\WV2Profile_<base32 id>\
     └── initialization script: wach-page.js
```

- Multiple webviews in one window needs Tauri's `unstable` feature (`Window::add_child`). **Risk**, checked in the spike.
- **Layout is owned by Rust.** The ui webview reports the two pane body rectangles (on resize, divider drag, Single/Split) through one command; Rust validates them and calls `set_position` / `set_size` on the account webviews. Up to two account webviews are on screen at once.
- **Custom title bar:** `decorations: false`, drag regions via `data-tauri-drag-region`, our own min/max/close buttons. Keep Windows 11 Snap Layouts on the maximise button if Tauri allows it; otherwise note it as a known gap.
- Switching accounts changes which webviews sit in the panes. **Open question for the spike:** hiding a webview sets WebView2 `IsVisible=false`, which lets Chromium throttle timers and may slow the WhatsApp connection or notifications. If so, keep every account "visible" and park inactive ones out of view (or behind the ui), never hide them.
- The ui webview talks to Rust through Tauri commands. Account webviews get **one** command only, `wach_page_event`, allowed through a capability for `https://web.whatsapp.com/*`. Rust checks that the calling webview's label is a known account and takes the account id from the label, never from the message.

### Account isolation

Option A (start here): give each account webview its own data directory (`WebviewBuilder::data_directory`). Simple and known to work, but each data directory starts its own WebView2 browser process group (roughly +80–150 MB per account).

Option B (if A is too heavy): one shared environment with a WebView2 **profile** per account (`ProfileName`), which shares one browser process. Tauri does not expose profiles, so this needs creating the controller ourselves via `webview2-com`. Decide after measuring A in the spike. **Built on branch `option-b-profiles`** with a one-change patch of tauri-runtime-wry (Tauri's `data_store_identifier` → WebView2 profile name). 3 accounts: 1151 MB / 2 process trees vs 1306 MB / 4 trees on main. See HANDOVER.md.

## 4. Features and how each is built

| # | Feature | How | Borrowed from |
|---|---|---|---|
| 4.1 | Stay on WhatsApp | `on_navigation`: allow only `https://web.whatsapp.com` (port 443); anything else http(s) → default browser, no user/password in URL; other schemes dropped | Altus + `security.ts` |
| 4.2 | New windows / `_blank` links | WebView2 `NewWindowRequested` (via `with_webview`): always cancel; open safe http(s) URLs in the browser. `wa.me` / `api.whatsapp.com` stay in the page | Altus + preload; official app |
| 4.3 | Permissions | WebView2 `PermissionRequested`, set **before** the first navigation: allow notifications and mic/camera only if the request comes from `web.whatsapp.com` itself and the tab's switch is on; deny everything else; never show WebView2's own prompt | Altus + `permissions.ts`; official app |
| 4.4 | Unread count | **Built:** Tauri `on_document_title_changed` → `rules::unread_from_title` (0–100000) → rail badge, tray tooltip/icon, taskbar overlay. ~~Page script~~ | Altus + preload |
| 4.5 | Notifications | **Built:** WebView2 `NotificationReceived` (`ICoreWebView2_24`), marked handled → our own toast "Account · title" via the `windows` crate under AUMID `app.wach.desktop` (registered in HKCU at start) → click shows the window, puts the account in the focused pane and calls `ReportClicked()` so WhatsApp opens the chat. ~~Page script wrapping `window.Notification`~~ | official app (native toasts) |
| 4.6 | Tray | Tauri tray: show/hide, quit; tooltip lists unread per account; icon swaps when total > 0 | Altus + `main.ts` |
| 4.7 | Taskbar dot | `Window::set_overlay_icon` when total unread > 0 | Altus + |
| 4.8 | Single instance | `tauri-plugin-single-instance`: second start focuses the window | Altus + |
| 4.9 | Window position | Save x, y, width, height, maximized 350 ms after changes; on start **make sure the saved rectangle is on a connected monitor**, else centre on the primary one | official app (`EnsureIsVisible`) |
| 4.10 | Crash recovery | WebView2 `ProcessFailed` per account → reload that account only, at most N times per minute | Altus + |
| 4.11 | Downloads | `on_download`: let WebView2 save to Downloads; show "saved · show in folder" later if wanted | — |
| 4.12 | Context menu | WebView2 `ContextMenuRequested`: remove back/forward/reload/print/share/inspect/save-as items | official app |
| 4.13 | Offline screen | On navigation failure show our own small "Can't reach WhatsApp — Retry" page | official app |
| 4.14 | Zoom | Ctrl +/−/0 per account, 50–250 % | Altus + |
| 4.15 | Closing an account | Remove its webview, record its id; delete its data folder at the **next** start (files are locked while running); never delete when several accounts vanish at once; keep the last closed one for "reopen" | Altus + `partitions.ts` |
| 4.16 | WebView2 options | Turn off Microsoft data reporting where WebView2 allows; no extra browser flags unless measured | official app |
| 4.17 | Split view | Single/Split switch, two pane rectangles from the ui webview, account per pane, focused pane (glow border, gets keys), swap, focus, close pane, divider drag (ratio saved), auto-Single below ~700 px. One account is in one pane at most. Shortcuts: `Ctrl+\`, `Ctrl+Tab`, `Ctrl+U`, `Ctrl+1–9` (DESIGN.md §2) | WACH's own |
| 4.18 | Design tokens | `ui/tokens.css` holds every design value; components use role tokens only; theme = a file that overrides tokens; Rust reads window/tray colours from the theme, not hard-coded; a check script fails the build on raw hex/px/shadow outside `tokens.css` (DESIGN.md §7) | WACH's own |
| 4.19 | App icon | Split bubble logo, master `assets/logo.svg`, exported to `icons/` (ICO + PNGs) and three tray states (normal, unread, muted) (DESIGN.md §5) | WACH's own |

### Page script (`wach-page.js`) — dropped

Not needed: native WebView2 events cover unread and notifications (see 4.4/4.5). Kept here for reference only. It would have had to:
- do nothing unless `location.origin === "https://web.whatsapp.com"`, and only in the top frame;
- not touch WhatsApp's own objects beyond `window.Notification` and reading `<title>`;
- send only the small fixed message set (`unread`, `notify`, `notify-click-ack`, `zoom`).

## 5. Data and settings

`%LOCALAPPDATA%\app.wach.desktop\` (the program itself installs to `%LOCALAPPDATA%\WACH\`)
- `config.json` — accounts `[ {id, name, color, notifications, media} ]`, layout `{mode: "single"|"split", left, right, focused, ratio}`, window state, settings (close-to-tray, theme, glow size, `muted_until`). Written atomically (temp file + rename).
- `wv\` — one shared WebView2 folder; each account is a profile in it (Option B, see HANDOVER.md).
- `closed.json` — accounts waiting for deletion.
- Start with Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `WACH` = `"<exe>" --hidden`.
- Updater signing key: `%USERPROFILE%\.tauri\wach-updater.key` + a git-ignored copy in the repo folder. Never committed.

Ids: random, `[A-Za-z0-9_-]{12}`, validated everywhere they become a path.

## 6. Milestones (each ends with something you can run)

0. **Setup** — install `rustup` (MSVC toolchain) and Visual Studio C++ Build Tools; `cargo create-tauri-app` with vanilla template, strip it.
1. **Spike (go/no-go)** — window + 2 account webviews with separate data folders, **placed side by side**. Pass when: both log in, both stay logged in after restart, a third out-of-view account still delivers a notification within 10 s, resizing the window keeps both panes in place without flicker, and memory per account is written down. Also try Option B if A costs > 150 MB per account.
2. **Shell and split view** — `ui/tokens.css` + Night Signal theme, custom title bar, rail, pane headers, status bar; Single/Split, swap, focus, divider; add/rename/close/reorder accounts, `config.json`, closed-account cleanup; logo and icon set in place.
3. **Page script** — unread counts, notifications with names, toast click → right tab, link rules, permissions.
4. **Windows bits** — tray, overlay dot, single instance, window position check, crash reload, context menu, offline page, zoom.
5. **Installer** — Tauri NSIS bundle, per-user install, Start-menu shortcut with AppUserModelID, uninstall removes `%LOCALAPPDATA%\app.wach.desktop` (all accounts) only if the user ticks Tauri's "delete app data" box.

## 7. Testing

- `cargo test` for the pure parts: URL rules, unread parsing, permission decisions, id validation, cleanup planning, monitor-visibility check.
- A manual checklist per milestone (log in, restart, notifications from a background account, link opening, crash a renderer via `chrome://crash` equivalent / killing the process).

## 8. Risks

| Risk | Impact | Plan |
|---|---|---|
| Tauri multi-webview is "unstable" | API may change | Pin Tauri version; isolate window/webview code in one module |
| Hidden webviews throttled | Missed or late messages | Spike test; switch by z-order instead of hide |
| Memory with one data folder per account | Heavy with many accounts | Done: Option B (profiles), ~150 MB less with 3 accounts |
| Shared browser process (Option B) | One crash takes all accounts down | Each account reloads on its own (crash recovery) |
| Lost updater key | Installed copies can never update | Two copies on the PC + one off the PC (user) |
| Toast clicks unpackaged | Clicks do nothing | AUMID + shortcut from installer; pick toast crate in milestone 3 |
| WhatsApp Web blocks or changes | App breaks | Keep page script minimal; we use only `<title>` and `Notification` |
| Calls in WhatsApp Web inside WebView2 | May not work | Test in milestone 3; if unsupported, say so in the README |
| Two native webviews next to an HTML layout | Panes lag or flicker on resize | Rust owns the rectangles; batch updates per frame; test in the spike |
| Custom title bar | Lose Snap Layouts / native feel | Try Tauri's options first; fall back to the system frame if it feels wrong |

## 9. Ground rules

- Security first, then stability, then resource use (same order as Altus +).
- No telemetry. The only network traffic is WhatsApp Web itself.
- `_wa-analysis/` holds copies of WhatsApp's own files for reference only: never commit or ship them.
- No hard-coded design values: colours, radii, sizes, spacing, shadows, fonts and timings go through tokens (DESIGN.md §7).
- The app loads nothing from the internet except WhatsApp Web: fonts and icons ship inside `ui/`.

# WACH — build plan

A small Windows app that shows several WhatsApp Web accounts side by side, each fully separate, without Electron.

Status: draft for review · 2026-11-17

## 1. Goal and scope

**Must have (v1)**
- One tab per WhatsApp account; add, rename, close, switch, reorder; remembered between runs.
- Each account fully isolated (its own cookies, IndexedDB, service workers), and stays logged in after a restart.
- Notifications that name the account; clicking one opens the right tab.
- Unread count per tab, total on the tray icon and as a taskbar overlay dot.
- Tray icon (show/hide, quit), close-to-tray, one running copy only.
- Links that leave `web.whatsapp.com` open in the default browser.
- Microphone/camera and notification permissions per account; everything else refused.
- A crashed account reloads on its own.
- A Windows installer.

**Not in v1** (may come later): auto-update, start with Windows, per-account mute, themes, translations, `whatsapp://` links, Enter-key options, resource tools, migrating Altus + accounts (users rescan QR codes once).

**Non-goals, on purpose**
- Never pretend to be the official WhatsApp app: no native "bridges", no `X-WA-WebView2-Version` header, no fake user agent. We are a plain browser showing WhatsApp Web.
- Never suspend or throttle an account in the background: the official app can only do that because WhatsApp's servers wake it with Windows push messages, which we do not get. A paused tab would miss messages.

## 2. Stack

- **Tauri 2** (Rust) on the system **WebView2** runtime (already installed: 157.x, updated by Windows).
- **UI: plain HTML/CSS/JavaScript, no bundler and no npm.** Tauri serves a static folder. This keeps the dependency count to the Rust crates only.
- Rust crates beyond Tauri: `webview2-com` (low-level WebView2 calls Tauri does not expose), `windows` (taskbar, registry), plus Tauri plugins `single-instance` and a toast library (see 4.5). Each extra crate must be justified in review.

## 3. Architecture

```
Main window (Tauri)
├── "ui" webview: local tab bar  (top strip, ~36 px, our own HTML)
└── one webview per account, all stacked in the area below
     ├── url: https://web.whatsapp.com/
     ├── own data folder: %LOCALAPPDATA%\WACH\accounts\<id>\
     └── initialization script: wach-page.js
```

- Multiple webviews in one window needs Tauri's `unstable` feature (`Window::add_child`). **Risk**, checked in the spike.
- Switching tabs changes which account webview is on top. **Open question for the spike:** hiding a webview sets WebView2 `IsVisible=false`, which lets Chromium throttle timers and may slow the WhatsApp connection or notifications. If so, keep every account "visible" and switch by z-order or by moving inactive ones out of view, never by hiding.
- The ui webview talks to Rust through Tauri commands. Account webviews get **one** command only, `wach_page_event`, allowed through a capability for `https://web.whatsapp.com/*`. Rust checks that the calling webview's label is a known account and takes the account id from the label, never from the message.

### Account isolation

Option A (start here): give each account webview its own data directory (`WebviewBuilder::data_directory`). Simple and known to work, but each data directory starts its own WebView2 browser process group (roughly +80–150 MB per account).

Option B (if A is too heavy): one shared environment with a WebView2 **profile** per account (`ProfileName`), which shares one browser process. Tauri does not expose profiles, so this needs creating the controller ourselves via `webview2-com`. Decide after measuring A in the spike.

## 4. Features and how each is built

| # | Feature | How | Borrowed from |
|---|---|---|---|
| 4.1 | Stay on WhatsApp | `on_navigation`: allow only `https://web.whatsapp.com` (port 443); anything else http(s) → default browser, no user/password in URL; other schemes dropped | Altus + `security.ts` |
| 4.2 | New windows / `_blank` links | WebView2 `NewWindowRequested` (via `with_webview`): always cancel; open safe http(s) URLs in the browser. `wa.me` / `api.whatsapp.com` stay in the page | Altus + preload; official app |
| 4.3 | Permissions | WebView2 `PermissionRequested`, set **before** the first navigation: allow notifications and mic/camera only if the request comes from `web.whatsapp.com` itself and the tab's switch is on; deny everything else; never show WebView2's own prompt | Altus + `permissions.ts`; official app |
| 4.4 | Unread count | Page script watches `<title>` with a MutationObserver, reads the number, sends `{kind:"unread", count}`; Rust validates 0–100000, updates tab bar, tray tooltip and overlay | Altus + preload |
| 4.5 | Notifications | Page script wraps `window.Notification`: add "Account · " to the title, then forward `{kind:"notify", title, body, tag}` to Rust, which shows a Windows toast; toast click → show window, select tab, call the page's click handler. Toast clicks for an unpackaged app need an AppUserModelID and a Start-menu shortcut (installer). Library: check whether `tauri-plugin-notification` reports clicks on Windows; if not, use `tauri-winrt-notification` / `windows` toast APIs | Altus + preload; official app (native toasts) |
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

### Page script (`wach-page.js`)

Runs at document start in every account webview. Must:
- do nothing unless `location.origin === "https://web.whatsapp.com"`, and only in the top frame;
- not touch WhatsApp's own objects beyond `window.Notification` and reading `<title>`;
- send only the small fixed message set (`unread`, `notify`, `notify-click-ack`, `zoom`).

## 5. Data and settings

`%LOCALAPPDATA%\WACH\`
- `config.json` — accounts `[ {id, name, color, notifications, media} ]`, selected id, window state, settings (close-to-tray). Written atomically (temp file + rename).
- `accounts\<id>\` — WebView2 data per account.
- `closed.json` — accounts waiting for deletion.

Ids: random, `[A-Za-z0-9_-]{12}`, validated everywhere they become a path.

## 6. Milestones (each ends with something you can run)

0. **Setup** — install `rustup` (MSVC toolchain) and Visual Studio C++ Build Tools; `cargo create-tauri-app` with vanilla template, strip it.
1. **Spike (go/no-go)** — window + 2 account webviews with separate data folders. Pass when: both log in, both stay logged in after restart, the hidden account still delivers a notification within 10 s, and memory per account is written down. Also try Option B if A costs > 150 MB per account.
2. **Tabs** — tab bar, add/rename/close/reorder, `config.json`, closed-account cleanup.
3. **Page script** — unread counts, notifications with names, toast click → right tab, link rules, permissions.
4. **Windows bits** — tray, overlay dot, single instance, window position check, crash reload, context menu, offline page, zoom.
5. **Installer** — Tauri NSIS bundle, per-user install, Start-menu shortcut with AppUserModelID, uninstall removes `%LOCALAPPDATA%\WACH` only if the user ticks it.

## 7. Testing

- `cargo test` for the pure parts: URL rules, unread parsing, permission decisions, id validation, cleanup planning, monitor-visibility check.
- A manual checklist per milestone (log in, restart, notifications from a background account, link opening, crash a renderer via `chrome://crash` equivalent / killing the process).

## 8. Risks

| Risk | Impact | Plan |
|---|---|---|
| Tauri multi-webview is "unstable" | API may change | Pin Tauri version; isolate window/webview code in one module |
| Hidden webviews throttled | Missed or late messages | Spike test; switch by z-order instead of hide |
| Memory with one data folder per account | Heavy with many accounts | Measure; Option B (profiles) |
| Toast clicks unpackaged | Clicks do nothing | AUMID + shortcut from installer; pick toast crate in milestone 3 |
| WhatsApp Web blocks or changes | App breaks | Keep page script minimal; we use only `<title>` and `Notification` |
| Calls in WhatsApp Web inside WebView2 | May not work | Test in milestone 3; if unsupported, say so in the README |

## 9. Ground rules

- Security first, then stability, then resource use (same order as Altus +).
- No telemetry. The only network traffic is WhatsApp Web itself.
- `_wa-analysis/` holds copies of WhatsApp's own files for reference only: never commit or ship them.

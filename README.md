# WACH

A small Windows app for using **several WhatsApp accounts at once**, side by side.

Each account runs [WhatsApp Web](https://web.whatsapp.com) in its own private WebView2 profile, so logins, chats and storage never mix. WACH is built with [Tauri 2](https://tauri.app) and the WebView2 that comes with Windows — no Electron, a ~2 MB installer.

> WACH is an independent project. It is not made by, endorsed by or connected to WhatsApp or Meta. It only shows the official WhatsApp Web page; it does not change it, inject code into it, or use any private API.

## Features

- Any number of accounts, each in its own WebView2 profile (separate cookies and storage).
- Single or split view: two accounts side by side, swap panes, drag the divider.
- Notifications that say which account they're from; clicking one opens that account.
- Unread counts on the account rail, the tray icon tooltip and the taskbar icon.
- Tray menu: jump to an account, mute all notifications (1 hour or until turned back on), start with Windows, check for updates.
- Start with Windows, quietly in the tray.
- Signed in-app updates from this repo's GitHub Releases (WACH asks before installing).
- Six themes: Night Signal, Glass (frosted, see-through on Windows 11), Daylight, Graphite, Ember and High Contrast. Pick one with the theme button above Settings.
- Per-account zoom, crash recovery, keyboard shortcuts (`Ctrl+\` split, `Ctrl+Tab` other pane, `Ctrl+U` swap, `Ctrl+1…9` accounts).
- No telemetry.

## Install

Download `WACH_x.y.z_x64-setup.exe` from [Releases](../../releases/latest) and run it. It installs for your user only (no admin rights). Windows 10/11 with the WebView2 runtime (built into Windows 11).

Then click **+**, name the account, and scan the QR code with WhatsApp on your phone (**Settings → Linked devices → Link a device**).

## Build

Needs Rust (MSVC toolchain, pinned in `rust-toolchain.toml`), the Tauri CLI 2 and NSIS.

```powershell
cd src-tauri
cargo test      # unit tests + design-token check
cargo build     # debug build: target\debug\wach.exe
```

Releases: `scripts\release.ps1 -Version x.y.z` (needs the updater signing key, which is never committed: `*.key` is git-ignored).

More detail: [PLAN.md](PLAN.md) (how it works and why), [DESIGN.md](DESIGN.md) (look and design tokens), [HANDOVER.md](HANDOVER.md) (state of the project, lessons learnt).

## Licence

[MIT](LICENSE). `src-tauri/patches/tauri-runtime-wry` is a lightly patched copy of Tauri's `tauri-runtime-wry` (MIT/Apache-2.0, see its own licence files).

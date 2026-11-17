# WACH — design

The look of WACH's own interface: the title bar, the account rail, the pane headers and the status bar. The WhatsApp Web pages inside the panes keep WhatsApp's own look; we never restyle or copy it.

Status: chosen 2026-11-17 · layout **Option A "Rail"**, logo **Split bubble**, colour scheme **"Night Signal"**.
Design canvas (all options, private link): https://claude.ai/artifact/H6ZXDkxhxbAxCWfGTXqkZr

## 1. Principles

- **Deep navy, signal blue.** Dark surfaces step up from Abyss to Raised. Blue is kept for things that are active or need you: the focused pane, unread counts, the main button.
- **One glow at a time.** Only the focused pane (and its account in the rail) glows. Everything else is flat.
- **Everything is a token.** No colour, radius, size, shadow or font is written straight into a rule. Every value comes from a token in `ui/tokens.css` (see §7), so a new theme is one file.
- **Quiet chrome.** Our parts stay thin so the two chats get the room.

## 2. Layout — Option A "Rail"

```
┌──────────────────────────────────────────────────────────────┐
│ [logo] WACH        [ Single | Split ]           _   □   ✕   │  title bar 40 px
├──────┬───────────────────────────────┬───────────────────────┤
│  W●  │ [W Work ▾] ● Left   ⇄ ⤢ ✕     │ [P Personal ▾] ● Right│  pane header 44 px
│  P   │                               │                       │
│  F   │   WhatsApp Web · Work         │  WhatsApp Web ·       │
│  S   │   (focused: glow border)      │  Personal             │
│  +   │                               │                       │
│      │                               │                       │
│  ⚙   │                               │                       │
├──────┴───────────────────────────────┴───────────────────────┤
│ ● 4 accounts connected          Ctrl+\ split · Ctrl+Tab · … │  status bar 28 px
└──────────────────────────────────────────────────────────────┘
  rail 72 px          8 px gap and outer padding between panes
```

- **Title bar** (our own, window frame turned off): logo, name, Single/Split switch in the middle, minimise / maximise / close on the right. The empty parts drag the window.
- **Rail:** one rounded square per account with its initial and colour. A ring and a 4 px side marker show where the account is open: Sky = focused pane, dim blue = other pane, none = not shown. Unread badge on the top-right corner. Then "+" (add account) and settings at the bottom.
- **Panes:** in Split, two panes side by side with an 8 px gap; in Single, one pane fills the space. Each pane has a header:
  - account chip with a menu to pick which account shows in this pane,
  - connection dot and "Left" / "Right",
  - Swap panes, Focus (go to Single with this pane), Close pane.
- The **focused pane** has a Sky border with the glow. Typing and shortcuts go to it.
- **Status bar:** connection summary on the left (adds "· notifications muted" while muted), the main shortcuts on the right.
- **Update chip:** when an update is ready, an "Update x ready · Restart" pill appears in the **title bar, right after the logo and "WACH"** (tokens only; moved there from the status bar in 0.2.2 at the user's request). Clicking it installs.
- **Settings sheet (0.2.0):** Start with Windows checkbox; notifications as a wide segmented control (On / Mute 1 hour / Mute until I turn them on); update row with a "Check for updates" / "Install" button, an error line and the version.
- **Tray menu (0.2.0):** accounts with unread ("Name  (n)", click opens), Mute submenu, Start with Windows check, Check for updates / Install update, Show, Quit.
- Narrow windows (below about 700 px wide): Split turns into Single on its own and comes back when there is room.

### Behaviour

| Action | Result |
|---|---|
| Click an account in the rail | Shows it in the **focused** pane. If it is already in the other pane, the two swap. |
| Ctrl+click / middle-click an account | Shows it in the **other** pane. |
| `Ctrl+\` | Single ↔ Split. |
| `Ctrl+Tab` | Move focus to the other pane. |
| `Ctrl+U` | Swap the two panes. |
| `Ctrl+1`…`Ctrl+9` | Account 1–9 into the focused pane. |
| Drag the divider | Resize the panes (saved; double-click resets to 50/50). |
| Close pane | Back to Single with the other pane. |

One account can be in only one pane at a time (it is one webview).

## 3. Colour — "Night Signal"

| Token | Hex | Use |
|---|---|---|
| `--color-abyss` | `#070B16` | window frame, title bar, gaps |
| `--color-navy` | `#0B1224` | app ground, rail, pane body |
| `--color-panel` | `#111A31` | bars, pane headers, rail buttons |
| `--color-raised` | `#17223F` | hover, menus |
| `--color-line` | `#24304F` | borders, dividers |
| `--color-ink` | `#E7EDFB` | main text |
| `--color-mist` | `#93A1C4` | secondary text, icons |
| `--color-haze` | `#5D6B8F` | resting icons and decoration only (too low-contrast for text) |
| `--color-signal` | `#2A64F6` | main buttons, badges (white text on it ≈ 5:1) |
| `--color-sky` | `#6FB1FF` | focus, links, active icons |
| `--color-ice` | `#B9D8FF` | text on glowing or blue areas |
| `--color-deep` | `#173A99` | selected rows |
| `--color-online` | `#34D399` | connected |
| `--color-waiting` | `#FBBF24` | connecting, phone offline |
| `--color-alert` | `#FF6B81` | logged out, crashed, tray unread dot |

**Account colours** (the user picks one per account): Sky `#6FB1FF`, Violet `#A78BFA`, Mint `#34D399`, Amber `#FBBF24`, Rose `#FF6B81`, Teal `#2DD4BF`. Initials on them use `--color-navy`.

## 4. Glow, shadow, radius, spacing, type

| Group | Token | Value |
|---|---|---|
| Glow | `--glow-color` | `rgba(61, 139, 255, 0.35)` |
| | `--glow-size` | `24px` (0 turns all glow off) |
| | `--glow-focus` | `0 0 0 1px rgba(111,177,255,.25), 0 0 var(--glow-size) var(--glow-color)` |
| | `--glow-button` | `0 0 calc(var(--glow-size) / 2) rgba(42,100,246,.6)` |
| Radius | `--radius-sm` | `6px` (badges' inner bits, segmented items) |
| | `--radius-md` | `8px` (icon buttons, chips) |
| | `--radius-lg` | `12px` (panes) |
| | `--radius-xl` | `14px` (rail buttons) |
| | `--radius-pill` | `999px` (badges, dots) |
| Space | `--space-1` … `--space-6` | `4, 8, 12, 16, 24, 32px` |
| Size | `--size-titlebar` | `40px` |
| | `--size-rail` | `72px` |
| | `--size-pane-header` | `44px` |
| | `--size-statusbar` | `28px` |
| | `--size-rail-button` | `44px` |
| | `--size-icon-button` | `30px` |
| | `--size-icon` | `18px` |
| | `--pane-gap` | `8px` |
| Border | `--border-width` | `1px` |
| | `--focus-ring` | `2px solid var(--color-sky)` |
| Type | `--font-ui` | `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif` |
| | `--font-brand` | `"Segoe UI Variable Display", "Segoe UI", sans-serif` (the canvas uses Sora; see note) |
| | `--font-mono` | `"Cascadia Mono", Consolas, monospace` |
| | `--text-sm` / `--text-md` / `--text-lg` | `12px` / `13px` / `15px` |
| Motion | `--ease` | `cubic-bezier(.2, .7, .2, 1)` |
| | `--speed` | `140ms` (0 when Windows "reduce motion" is on) |

**Fonts note:** the canvas uses Sora / Inter / JetBrains Mono from Google Fonts. The app must not load anything from the internet, so it uses the Windows fonts above. If we want Sora for the logo text, we ship the font file (OFL licence) inside `ui/`.

## 5. Logo and app icon

**Split bubble:** a blue chat bubble (gradient Sky-ish `#7CBCFF` → Signal `#2A64F6`, top-left to bottom-right) with its tail at the bottom left, holding two upright panes: left one navy, right one near-white, each with two short "text" lines.

- Master: `assets/logo.svg`, viewBox `0 0 64 64`.
  - Bubble: `M15 7h34a11 11 0 0 1 11 11v21a11 11 0 0 1-11 11H28l-12 9v-9.3A11 11 0 0 1 4 39V18A11 11 0 0 1 15 7z`
  - Panes: `x 14 / 34, y 16, 16 × 25, rx 3.5`.
- **Small version** (32 px and 16 px): no text lines, panes a bit wider (`x 13 / 34`, `17 × 25`, `rx 3`), flat fill.
- Sizes to export: 16, 20, 24, 32, 40, 48, 64, 256 → `icons/icon.ico`; PNGs for Tauri's `bundle.icon`.
- **Tray states:** normal (small version); unread (small version + `--color-alert` dot, top right); all muted (grey gradient `#8592B4` → `#4A5678`).
- Kept for later: Twin bubbles, W monogram (splash/installer ideas).

## 6. Icons

Line icons on a 24 px grid, 1.75 stroke, round caps and joins, `stroke="currentColor"` so they take the token colour. Kept as inline SVG `<symbol>`s in `ui/icons.svg`.

New chat · Split view · Single view · Swap panes · Focus · Account · Accounts · Bell · Bell off · Settings · Tray · Pin · Search · Close · Minimise · Maximise · Reload · Open in browser · Microphone · Camera · Isolated (lock) · Dark mode · Download · Shortcuts · More · Chevron.

The exact paths are on the "Logo & icon set" board of the canvas.

## 7. Tokens and theming (rules)

1. **All design values live in `ui/tokens.css`** as CSS custom properties on `:root`: colours, radii, spacing, sizes, borders, shadows/glow, fonts, text sizes, motion.
2. Other CSS files use only `var(--…)`. A raw hex, `px` radius, or shadow outside `tokens.css` is a review failure. (A small check script can grep for it.)
3. Components use **role tokens**, not palette tokens, e.g. `--pane-border-focused: var(--color-sky)`. Palette → role mapping sits at the bottom of `tokens.css`, so a theme can change roles without touching components.
4. A **theme** is a file that only overrides tokens, e.g. `ui/themes/night-signal.css` (default). Switching theme = swapping one `<link>` or a `data-theme` attribute on `<html>`.
5. Rust-side colours (window background before the UI loads, tray dot) read from the same theme values in `config.json`, never from hard-coded numbers, so the window doesn't flash a different colour.
6. `--glow-size` is user-adjustable (0–48 px) and `prefers-reduced-motion` sets `--speed: 0ms`.
7. Light theme is not in v1, but nothing may block it: no colour assumptions in code, only tokens.

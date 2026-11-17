// WACH interface: title bar, account rail, two panes, sheets.
// The WhatsApp pages themselves are native webviews placed by Rust over the
// pane bodies; this file only tells Rust where those rectangles are.
"use strict";

const T = window.__TAURI_INTERNALS__;
const invoke = (cmd, args = {}) => T.invoke(cmd, args);
const listen = (event, cb) =>
  T.invoke("plugin:event|listen", { event, target: { kind: "Any" }, handler: T.transformCallback((e) => cb(e.payload)) });

const COLORS = ["sky", "violet", "mint", "amber", "rose", "teal"];
const SPLIT_MIN_WIDTH = 700;

const S = { cfg: null, unread: {}, maximized: false, sheet: false, crashed: {} };
const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

function h(tag, attrs = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const kid of kids.flat()) if (kid !== null && kid !== undefined && kid !== false) el.append(kid);
  return el;
}
function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", "#" + name);
  svg.append(use);
  return svg;
}
const initial = (name) => (Array.from(name.trim())[0] || "?").toUpperCase();
const accounts = () => S.cfg.accounts;
const acc = (id) => accounts().find((a) => a.id === id) || null;
const L = () => S.cfg.layout;
const sideId = (side) => (side === 0 ? L().left : L().right);
const setSide = (side, id) => { if (side === 0) L().left = id; else L().right = id; };
const split = () => L().split && window.innerWidth >= SPLIT_MIN_WIDTH;
const visibleSides = () => (split() ? [0, 1] : [L().focused]);

// ------------------------------------------------------------ layout actions

let saveTimer = 0;
function saveLayout() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try { S.cfg.layout = await invoke("set_layout", { layout: L() }); } catch (e) { console.error(e); }
  }, 150);
}

function commit({ focus = false } = {}) {
  render();
  place();
  saveLayout();
  if (focus) focusPane(L().focused);
}

function focusPane(side) {
  const id = sideId(side);
  if (id && !S.sheet) invoke("focus_account", { id }).catch(() => {});
}

function assign(side, id) {
  const other = 1 - side;
  if (sideId(other) === id) setSide(other, sideId(side)); // it was in the other pane: swap
  setSide(side, id);
}

function showInFocused(id) {
  if (!acc(id)) return;
  assign(L().focused, id);
  commit({ focus: true });
}

function showInOther(id) {
  if (!acc(id)) return;
  L().split = true;
  assign(1 - L().focused, id);
  commit();
}

function swapPanes() {
  [L().left, L().right] = [L().right, L().left];
  commit({ focus: true });
}

function toggleSplit() {
  L().split = !L().split;
  if (L().split && !sideId(1 - L().focused)) {
    const free = accounts().find((a) => a.id !== sideId(L().focused));
    if (free) setSide(1 - L().focused, free.id);
  }
  commit({ focus: true });
}

function otherPane() {
  if (!split()) return;
  L().focused = 1 - L().focused;
  commit({ focus: true });
}

function setFocusedSide(side, { moveKeyboard = false } = {}) {
  if (L().focused === side) return;
  L().focused = side;
  render();
  saveLayout();
  if (moveKeyboard) focusPane(side);
}

function onShortcut(s) {
  if (S.sheet) return;
  switch (s.action) {
    case "toggle-split": return toggleSplit();
    case "other-pane": return otherPane();
    case "swap": return split() && swapPanes();
    case "account": { const a = accounts()[s.n - 1]; if (a) showInFocused(a.id); return; }
  }
}

// ------------------------------------------------------------ placing the webviews

let placeQueued = false;
function place() {
  if (placeQueued) return;
  placeQueued = true;
  requestAnimationFrame(() => {
    placeQueued = false;
    const panes = [];
    if (!S.sheet) {
      for (const side of visibleSides()) {
        const id = sideId(side);
        if (!id || !acc(id)) continue;
        const r = $(`.pane[data-side="${side}"] .pane-body`).getBoundingClientRect();
        if (r.width < 1 || r.height < 1) continue;
        panes.push({ id, rect: { x: r.left, y: r.top, w: r.width, h: r.height } });
      }
    }
    invoke("place_panes", { panes }).catch((e) => console.error("place_panes", e));
  });
}

// ------------------------------------------------------------ rendering

function render() {
  const app = $("#app");
  app.classList.toggle("maximized", S.maximized);
  $("#btn-max use").setAttribute("href", S.maximized ? "#i-restore" : "#i-max");
  $("#btn-max").setAttribute("aria-label", S.maximized ? "Restore" : "Maximize");
  $("#btn-single").setAttribute("aria-pressed", String(!L().split));
  $("#btn-split").setAttribute("aria-pressed", String(L().split));
  document.documentElement.style.setProperty("--glow-size", S.cfg.settings.glow + "px");

  const panes = $("#panes");
  panes.classList.toggle("split", split());
  panes.classList.toggle("single", !split());
  panes.style.setProperty("--ratio", String(L().ratio));

  renderRail();
  for (const side of [0, 1]) renderPane(side);
  renderStatus();
}

function renderRail() {
  const list = $("#rail-list");
  list.replaceChildren(
    ...accounts().map((a, i) => {
      const where = a.id === sideId(L().focused) ? "in-focused" : split() && a.id === sideId(1 - L().focused) ? "in-other" : "";
      const n = S.unread[a.id] || 0;
      return h(
        "button",
        {
          type: "button",
          class: `rail-acc ${where}`,
          "data-color": a.color,
          title: `${a.name}${i < 9 ? `  (Ctrl+${i + 1})` : ""}\nClick: this pane · Ctrl+click: other pane · Right-click: options`,
          "aria-label": `${a.name}${n ? `, ${n} unread` : ""}`,
          onclick: (e) => (e.ctrlKey ? showInOther(a.id) : showInFocused(a.id)),
          onauxclick: (e) => { if (e.button === 1) showInOther(a.id); },
          oncontextmenu: (e) => { e.preventDefault(); accountSheet(a.id); },
        },
        initial(a.name),
        n ? h("span", { class: "badge", text: n > 999 ? "999+" : String(n) }) : null,
        a.notifications ? null : h("span", { class: "muted-mark" }, icon("i-bell-off")),
      );
    }),
  );
}

function renderPane(side) {
  const pane = $(`.pane[data-side="${side}"]`);
  pane.classList.toggle("focused", L().focused === side);
  const a = acc(sideId(side));
  const chip = $(".chip", pane);
  chip.dataset.color = a ? a.color : "";
  chip.setAttribute("aria-label", `Choose the account in the ${side ? "right" : "left"} pane`);
  chip.replaceChildren(
    ...[
      a ? h("span", { class: "avatar", text: initial(a.name) }) : null,
      h("span", { class: "name", text: a ? a.name : "Choose account" }),
      icon("i-chevron"),
    ].filter(Boolean), // replaceChildren would print null as text
  );
  const dot = $(".dot", pane);
  dot.className = "dot" + (!a ? " off" : S.crashed[a.id] ? " warn" : "");
  $(".side-label", pane).textContent = side ? "Right" : "Left";
  $('[data-act="reload"]', pane).hidden = !a;

  const empty = $(".empty", pane);
  if (accounts().length === 0) {
    const logo = icon("logo");
    logo.setAttribute("class", "logo");
    logo.setAttribute("aria-hidden", "true");
    empty.replaceChildren(
      logo,
      h("h3", { text: "Welcome to WACH" }),
      h("p", { text: "Add your first WhatsApp account. Each one gets its own private storage, so you can stay logged in to several at once." }),
      h("button", { type: "button", class: "btn primary", text: "Add account", onclick: addSheet }),
    );
  } else if (!a) {
    empty.replaceChildren(
      h("p", { text: "No account in this pane." }),
      h("button", { type: "button", class: "btn", text: "Choose account", onclick: () => pickSheet(side) }),
    );
  } else {
    empty.replaceChildren(h("p", { text: `Loading ${a.name}…` }));
  }
}

function renderStatus() {
  const n = accounts().length;
  const total = accounts().reduce((t, a) => t + (S.unread[a.id] || 0), 0);
  const crashed = accounts().find((a) => S.crashed[a.id]);
  $("#status .dot").className = "dot " + (crashed ? "warn" : n ? "ok" : "off");
  $("#status-text").textContent = crashed
    ? `${crashed.name} stopped — reloading`
    : n === 0
      ? "No accounts yet"
      : `${n} account${n === 1 ? "" : "s"}${total ? ` · ${total} unread` : ""}${muted() ? " · notifications muted" : " · each in its own storage"}`;
  renderUpdate();
}

const muted = () => (S.settings?.muted_until || 0) > Date.now() / 1000;

// Status-bar chip: only shows when there is something to say about updates.
function renderUpdate() {
  const chip = $("#update-chip");
  const u = S.update || { state: "idle" };
  const text = {
    available: `Update ${u.version} ready · Restart`,
    installing: "Installing update…",
    checking: "Checking for updates…",
  }[u.state];
  chip.hidden = !text;
  chip.textContent = text || "";
  chip.classList.toggle("quiet", u.state !== "available");
  chip.disabled = u.state !== "available";
}

async function installUpdate() {
  S.update = { state: "installing" };
  renderUpdate();
  try { await invoke("install_update"); }
  catch (e) { S.update = { state: "error", message: String(e) }; renderUpdate(); }
}

// ------------------------------------------------------------ sheets (modals)

function openSheet(title, body) {
  S.sheet = true;
  $("#sheet-title").textContent = title;
  $("#sheet-body").replaceChildren(...body);
  $("#sheet").hidden = false;
  place(); // moves the WhatsApp panes out of the way
  const first = $("#sheet input, #sheet button");
  if (first) first.focus();
}

function closeSheet() {
  if (!S.sheet) return;
  S.sheet = false;
  $("#sheet").hidden = true;
  render();
  place();
  focusPane(L().focused);
}

function colorPicker(current, onPick) {
  const wrap = h("div", { class: "swatches", role: "group", "aria-label": "Colour" });
  for (const c of COLORS) {
    wrap.append(
      h("button", {
        type: "button",
        class: "swatch",
        "data-color": c,
        "aria-label": c,
        "aria-pressed": String(c === current),
        onclick: (e) => {
          $$(".swatch", wrap).forEach((s) => s.setAttribute("aria-pressed", "false"));
          e.currentTarget.setAttribute("aria-pressed", "true");
          onPick(c);
        },
      }),
    );
  }
  return wrap;
}

function addSheet() {
  let color = COLORS[accounts().length % COLORS.length];
  const name = h("input", { type: "text", id: "acc-name", maxlength: "40", placeholder: "Work, Personal, Family…", autocomplete: "off" });
  const err = h("p", { class: "error" });
  const submit = async () => {
    try {
      const a = await invoke("add_account", { name: name.value, color });
      accounts().push(a);
      closeSheet();
      showInFocused(a.id);
    } catch (e) {
      err.textContent = String(e);
    }
  };
  name.addEventListener("keydown", (e) => { if (e.key === "Enter") submit(); });
  openSheet("Add account", [
    h("div", { class: "field" }, h("label", { for: "acc-name", text: "Name" }), name),
    h("div", { class: "field" }, h("span", { class: "label", text: "Colour" }), colorPicker(color, (c) => (color = c))),
    h("p", { class: "note", text: "WhatsApp will show a QR code. On your phone open WhatsApp → Settings → Linked devices → Link a device, and scan it." }),
    err,
    h("div", { class: "row end" },
      h("button", { type: "button", class: "btn", text: "Cancel", onclick: closeSheet }),
      h("button", { type: "button", class: "btn primary", text: "Add account", onclick: submit })),
  ]);
}

function accountSheet(id) {
  const a = acc(id);
  if (!a) return;
  let color = a.color;
  const name = h("input", { type: "text", id: "acc-name", maxlength: "40", value: a.name, autocomplete: "off" });
  const notif = h("input", { type: "checkbox", id: "acc-notif", checked: a.notifications });
  const media = h("input", { type: "checkbox", id: "acc-media", checked: a.media });
  const err = h("p", { class: "error" });
  const idx = accounts().indexOf(a);
  const move = async (to) => {
    await invoke("move_account", { id, to });
    accounts().splice(idx, 1);
    accounts().splice(to, 0, a);
    accountSheet(id);
  };
  let armed = false;
  const remove = h("button", {
    type: "button",
    class: "btn danger",
    text: "Remove account",
    onclick: async (e) => {
      if (!armed) {
        armed = true;
        e.currentTarget.classList.add("armed");
        e.currentTarget.textContent = "Click again to remove";
        return;
      }
      try {
        await invoke("remove_account", { id });
        S.cfg.accounts = accounts().filter((x) => x.id !== id);
        if (L().left === id) L().left = null;
        if (L().right === id) L().right = null;
        delete S.unread[id];
        closeSheet();
        saveLayout();
      } catch (er) {
        err.textContent = String(er);
      }
    },
  });
  const save = async () => {
    try {
      const updated = await invoke("update_account", {
        id,
        patch: { name: name.value, color, notifications: notif.checked, media: media.checked },
      });
      Object.assign(a, updated);
      closeSheet();
    } catch (e) {
      err.textContent = String(e);
    }
  };
  name.addEventListener("keydown", (e) => { if (e.key === "Enter") save(); });
  openSheet(a.name, [
    h("div", { class: "field" }, h("label", { for: "acc-name", text: "Name" }), name),
    h("div", { class: "field" }, h("span", { class: "label", text: "Colour" }), colorPicker(color, (c) => (color = c))),
    h("div", {},
      h("label", { class: "toggle", for: "acc-notif" }, h("span", { text: "Notifications" }), notif),
      h("label", { class: "toggle", for: "acc-media" }, h("span", { text: "Microphone & camera (calls, voice notes)" }), media)),
    h("div", { class: "row" },
      h("button", { type: "button", class: "btn", text: "Move up", disabled: idx === 0, onclick: () => move(idx - 1) }),
      h("button", { type: "button", class: "btn", text: "Move down", disabled: idx === accounts().length - 1, onclick: () => move(idx + 1) }),
      h("button", { type: "button", class: "btn", text: "Reload", onclick: () => { invoke("reload_account", { id }); closeSheet(); } })),
    h("p", { class: "note", text: "Removing signs this account out of WACH and deletes its local data the next time WACH starts. Your chats stay on your phone." }),
    err,
    h("div", { class: "row end" }, remove,
      h("button", { type: "button", class: "btn", text: "Cancel", onclick: closeSheet }),
      h("button", { type: "button", class: "btn primary", text: "Save", onclick: save })),
  ]);
}

function pickSheet(side) {
  const items = accounts().map((a) =>
    h("button", {
      type: "button",
      class: "list-item" + (sideId(side) === a.id ? " current" : ""),
      "data-color": a.color,
      onclick: () => { L().focused = side; assign(side, a.id); closeSheet(); commit({ focus: true }); },
    },
    h("span", { class: "avatar", text: initial(a.name) }),
    h("span", { text: a.name }),
    h("span", { class: "meta", text: S.unread[a.id] ? `${S.unread[a.id]} unread` : sideId(1 - side) === a.id ? `in ${side ? "left" : "right"} pane` : "" })),
  );
  openSheet(`${side ? "Right" : "Left"} pane`, [
    h("div", { class: "list" }, items),
    h("div", { class: "row end" },
      h("button", { type: "button", class: "btn", text: "Add account", onclick: addSheet }),
      h("button", { type: "button", class: "btn", text: "Cancel", onclick: closeSheet })),
  ]);
}

function settingsSheet() {
  const st = S.cfg.settings;
  const tray = h("input", { type: "checkbox", id: "set-tray", checked: st.close_to_tray });
  const glow = h("input", { type: "range", id: "set-glow", min: "0", max: "48", step: "2", value: String(st.glow) });
  tray.addEventListener("change", () => { st.close_to_tray = tray.checked; invoke("set_settings", { closeToTray: tray.checked }); });

  const err = h("p", { class: "error" });
  const auto = h("input", { type: "checkbox", id: "set-auto", checked: !!S.settings?.autostart });
  auto.addEventListener("change", async () => {
    try { S.settings = await invoke("set_settings", { autostart: auto.checked }); err.textContent = ""; }
    catch (e) { err.textContent = String(e); }
    auto.checked = !!S.settings?.autostart;
  });

  const mu = S.settings?.muted_until || 0;
  const current = !muted() ? "off" : mu >= Number.MAX_SAFE_INTEGER ? "forever" : "hour";
  const muteSeg = h("div", { class: "segmented wide", role: "group", "aria-label": "Mute all notifications" },
    [["off", "On"], ["hour", "Mute 1 hour"], ["forever", "Mute until I turn them on"]].map(([v, t]) =>
      h("button", {
        type: "button",
        "aria-pressed": String(v === current),
        text: t,
        onclick: async (e) => {
          S.settings = await invoke("set_settings", { mute: v });
          $$("button", muteSeg).forEach((b) => b.setAttribute("aria-pressed", "false"));
          e.currentTarget.setAttribute("aria-pressed", "true");
          renderStatus();
        },
      })));

  const updText = h("span", { class: "note", text: updateLine() });
  const updBtn = h("button", {
    type: "button",
    class: "btn",
    text: S.update?.state === "available" ? `Install ${S.update.version}` : "Check for updates",
    onclick: async () => {
      if (S.update?.state === "available") return installUpdate();
      updBtn.disabled = true;
      updText.textContent = "Checking…";
      try { S.update = await invoke("check_update"); } catch (e) { S.update = { state: "error", message: String(e) }; }
      updText.textContent = updateLine();
      updBtn.disabled = false;
      updBtn.textContent = S.update?.state === "available" ? `Install ${S.update.version}` : "Check for updates";
      renderUpdate();
    },
  });
  glow.addEventListener("input", () => { st.glow = Number(glow.value); render(); });
  glow.addEventListener("change", () => invoke("set_settings", { glow: st.glow }));
  const keys = [
    ["Ctrl+\\", "Single / split"], ["Ctrl+Tab", "Other pane"], ["Ctrl+U", "Swap panes"],
    ["Ctrl+1…9", "Account 1–9 in this pane"], ["Ctrl + / − / 0", "Zoom this account"], ["Ctrl+click", "Open in the other pane"],
  ];
  openSheet("Settings", [
    h("label", { class: "toggle", for: "set-tray" }, h("span", { text: "Closing the window keeps WACH in the tray" }), tray),
    h("label", { class: "toggle", for: "set-auto" }, h("span", { text: "Start with Windows (in the tray, accounts load in the background)" }), auto),
    h("div", { class: "field" }, h("span", { class: "label", text: "Notifications from all accounts" }), muteSeg),
    h("div", { class: "field" }, h("label", { for: "set-glow", text: "Glow" }), glow),
    h("div", { class: "list" }, keys.map(([k, t]) => h("div", { class: "toggle" }, h("span", { text: t }), h("kbd", { text: k })))),
    h("div", { class: "toggle" }, updText, updBtn),
    err,
    h("p", { class: "note", text: `WACH ${S.version || ""} · each account is a separate WebView2 profile under %LOCALAPPDATA%\\app.wach.desktop. Updates come signed from GitHub. No telemetry.` }),
    h("div", { class: "row end" },
      h("button", { type: "button", class: "btn danger", text: "Quit WACH", onclick: () => invoke("window_cmd", { action: "quit" }) }),
      h("button", { type: "button", class: "btn primary", text: "Done", onclick: closeSheet })),
  ]);
}

function updateLine() {
  const u = S.update || { state: "idle" };
  switch (u.state) {
    case "available": return `Version ${u.version} is ready to install.`;
    case "up-to-date": return `You have the latest version (${S.version}).`;
    case "checking": return "Checking…";
    case "installing": return "Installing…";
    case "error": return u.message;
    default: return `Version ${S.version}`;
  }
}

// ------------------------------------------------------------ wiring

function wire() {
  $("#btn-single").onclick = () => { if (L().split) toggleSplit(); };
  $("#btn-split").onclick = () => { if (!L().split) toggleSplit(); };
  $("#btn-min").onclick = () => invoke("window_cmd", { action: "minimize" });
  $("#btn-max").onclick = () => invoke("window_cmd", { action: "toggle-maximize" });
  $("#btn-close").onclick = () => invoke("window_cmd", { action: "close" });
  $("#btn-add").onclick = addSheet;
  $("#btn-settings").onclick = settingsSheet;

  $("#titlebar").addEventListener("mousedown", (e) => {
    if (e.button !== 0 || e.target.closest("button")) return;
    if (e.detail === 2) invoke("window_cmd", { action: "toggle-maximize" });
    else invoke("window_cmd", { action: "drag" });
  });
  for (const el of $$("#edges [data-edge]")) {
    el.addEventListener("mousedown", (e) => {
      if (e.button === 0) invoke("window_resize", { edge: el.dataset.edge });
    });
  }

  for (const pane of $$(".pane")) {
    const side = Number(pane.dataset.side);
    $(".pane-head", pane).addEventListener("mousedown", () => setFocusedSide(side, { moveKeyboard: true }));
    pane.addEventListener("click", (e) => {
      const act = e.target.closest("[data-act]")?.dataset.act;
      if (act === "pick") pickSheet(side);
      else if (act === "swap") swapPanes();
      else if (act === "focus") { L().focused = side; L().split = false; commit({ focus: true }); }
      else if (act === "close") { L().focused = 1 - side; L().split = false; commit({ focus: true }); }
      else if (act === "reload" && sideId(side)) invoke("reload_account", { id: sideId(side) });
    });
  }

  const divider = $("#divider");
  divider.addEventListener("pointerdown", (e) => {
    divider.setPointerCapture(e.pointerId);
    divider.classList.add("dragging");
  });
  divider.addEventListener("pointermove", (e) => {
    if (!divider.hasPointerCapture(e.pointerId)) return;
    const r = $("#panes").getBoundingClientRect();
    L().ratio = Math.min(0.8, Math.max(0.2, (e.clientX - r.left) / r.width));
    $("#panes").style.setProperty("--ratio", String(L().ratio));
    place();
  });
  divider.addEventListener("pointerup", (e) => {
    divider.releasePointerCapture(e.pointerId);
    divider.classList.remove("dragging");
    saveLayout();
  });
  divider.addEventListener("dblclick", () => { L().ratio = 0.5; commit(); });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && S.sheet) return closeSheet();
    if (!e.ctrlKey || e.altKey) return;
    const map = { "\\": "toggle-split", Tab: "other-pane", u: "swap", U: "swap" };
    if (map[e.key]) { e.preventDefault(); onShortcut({ action: map[e.key] }); }
    else if (/^[1-9]$/.test(e.key)) { e.preventDefault(); onShortcut({ action: "account", n: Number(e.key) }); }
  });
  $("#sheet").addEventListener("mousedown", (e) => { if (e.target.id === "sheet") closeSheet(); });

  let lastSplit = null;
  new ResizeObserver(() => {
    if (lastSplit !== split()) { lastSplit = split(); render(); }
    place();
  }).observe($("#panes"));
}

async function main() {
  const st = await invoke("get_state");
  S.cfg = st.config;
  S.unread = st.unread || {};
  S.maximized = st.maximized;
  S.settings = st.settings;
  S.version = st.version;
  S.update = st.update;
  // First run with accounts but no layout yet: fill the panes in order.
  if (!L().left && accounts()[0]) L().left = accounts()[0].id;
  if (!L().right && accounts()[1] && accounts()[1].id !== L().left) L().right = accounts()[1].id;

  wire();
  render();
  place();

  await listen("unread", ({ id, count }) => { S.unread[id] = count; renderRail(); renderStatus(); });
  await listen("show-account", (id) => { closeSheet(); showInFocused(id); });
  await listen("shortcut", onShortcut);
  await listen("account-focused", (id) => {
    const side = L().left === id ? 0 : L().right === id ? 1 : -1;
    if (side >= 0) setFocusedSide(side);
  });
  await listen("account-crashed", ({ id, reloading }) => {
    S.crashed[id] = true;
    render();
    setTimeout(() => { delete S.crashed[id]; render(); }, reloading ? 6000 : 30000);
  });
  await listen("relayout", () => place());
  await listen("settings", (s) => { S.settings = s; renderStatus(); });
  await listen("update", (u) => { S.update = u; renderUpdate(); });
  $("#update-chip").onclick = installUpdate;
  await listen("window-state", ({ maximized }) => {
    if (S.maximized !== maximized) { S.maximized = maximized; render(); }
  });

  if (st.selftest) {
    const s = document.createElement("script");
    s.src = "selftest.js";
    s.onload = () =>
      window.WACH_SELFTEST({
        S, L, invoke, listen, sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
        swapPanes, toggleSplit, showInFocused, onShortcut, settingsSheet, closeSheet, commit,
      }).catch((e) => invoke("debug_log", { line: "FAIL crashed: " + e }));
    document.head.append(s);
  }
}

main().catch((e) => {
  document.body.textContent = "WACH could not start its interface: " + e;
});

// Debug-only self-test (WACH_SELFTEST=1 with a debug build). Drives the UI
// the way a user would and checks where Rust actually put each webview.
"use strict";

window.WACH_SELFTEST = async function (api) {
  const { S, L, invoke, sleep } = api;
  const log = (line) => invoke("debug_log", { line });
  let fails = 0;
  const check = (name, ok, extra = "") => {
    if (!ok) fails++;
    return log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " — " + extra : ""}`);
  };
  const bounds = () => invoke("debug_bounds");
  const shown = (b, id) => b[id] && b[id][0] > -1000;
  const settle = () => sleep(500);
  const ids = S.cfg.accounts.map((a) => a.id);
  const [A, B, C] = ids;

  await log(`start: ${ids.length} accounts`);
  const t0 = Date.now();
  let b = await bounds();
  while (Object.keys(b).length < ids.length && Date.now() - t0 < 30000) { await sleep(250); b = await bounds(); }
  await log(`all webviews up after ${Date.now() - t0} ms`);
  await settle();
  b = await bounds();
  const rest = ids.filter((id) => id !== L().left && id !== L().right);
  await check("split shows left+right, parks the rest", shown(b, L().left) && shown(b, L().right) && rest.every((id) => !shown(b, id)), JSON.stringify(b));
  await check("left pane is left of right pane", b[L().left][0] < b[L().right][0]);

  const [l0, r0] = [L().left, L().right];
  api.swapPanes(); await settle(); b = await bounds();
  await check("swap exchanges panes", L().left === r0 && L().right === l0 && b[r0][0] < b[l0][0]);

  api.toggleSplit(); await settle(); b = await bounds();
  const f = L().focused === 0 ? L().left : L().right;
  const o = L().focused === 0 ? L().right : L().left;
  await check("single shows only the focused account", shown(b, f) && !shown(b, o));
  api.toggleSplit(); await settle(); b = await bounds();
  await check("split again shows both", shown(b, f) && shown(b, o));

  api.showInFocused(C); await settle(); b = await bounds();
  await check("rail click puts account in focused pane", (L().focused === 0 ? L().left : L().right) === C && shown(b, C));

  api.onShortcut({ action: "account", n: 1 }); await settle(); b = await bounds();
  await check("Ctrl+1 shows account 1 in focused pane", (L().focused === 0 ? L().left : L().right) === A && shown(b, A));

  api.onShortcut({ action: "other-pane" }); await settle();
  const fBefore = L().focused;
  api.onShortcut({ action: "other-pane" }); await settle();
  await check("Ctrl+Tab moves focus between panes", L().focused !== fBefore);

  api.settingsSheet(); await settle(); b = await bounds();
  await check("open sheet parks every webview", ids.every((id) => !shown(b, id)));
  api.closeSheet(); await settle(); b = await bounds();
  await check("closing sheet brings panes back", shown(b, L().left) && shown(b, L().right));

  // Divider: ratio 0.3 → left pane narrower than right.
  L().ratio = 0.3; api.commit(); await settle(); b = await bounds();
  await check("divider ratio changes pane widths", b[L().left][2] < b[L().right][2], `${b[L().left][2]} vs ${b[L().right][2]}`);
  L().ratio = 0.5; api.commit(); await settle();

  // Add, rename, remove an account through the real commands.
  let added = null;
  try {
    added = await invoke("add_account", { name: "  Selftest  ", color: "teal" });
    S.cfg.accounts.push(added);
    await check("add_account trims name and creates id", added.name === "Selftest" && /^[A-Za-z0-9_-]{12}$/.test(added.id));
  } catch (e) { await check("add_account", false, String(e)); }
  try { await invoke("add_account", { name: "x", color: "#ff0000" }); await check("add_account rejects bad colour", false); }
  catch { await check("add_account rejects bad colour", true); }
  try { await invoke("add_account", { name: "   ", color: "sky" }); await check("add_account rejects empty name", false); }
  catch { await check("add_account rejects empty name", true); }
  if (added) {
    await sleep(2500);
    b = await bounds();
    await check("new account webview exists and is parked", b[added.id] && !shown(b, added.id));
    const upd = await invoke("update_account", { id: added.id, patch: { name: "Renamed", notifications: false } });
    await check("update_account renames and toggles", upd.name === "Renamed" && upd.notifications === false);
    await invoke("remove_account", { id: added.id });
    S.cfg.accounts = S.cfg.accounts.filter((a) => a.id !== added.id);
    await sleep(500);
    b = await bounds();
    await check("remove_account closes its webview", !b[added.id]);
  }
  try { await invoke("place_panes", { panes: [{ id: A, rect: { x: -5, y: 0, w: 10, h: 10 } }] }); await check("place_panes rejects bad rect", false); }
  catch { await check("place_panes rejects bad rect", true); }
  api.commit(); await settle();

  const st = await invoke("get_state");
  await check("layout saved in Rust", st.config.layout.left === L().left && st.config.layout.right === L().right);

  await log(`done: ${fails === 0 ? "ALL PASS" : fails + " FAILED"}`);
};

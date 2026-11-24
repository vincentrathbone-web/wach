// WACH — opt-in "Force voice & video calls" for one account (account settings).
// Added only to accounts where the user turned it on. It makes WhatsApp Web
// report its own calling switches as on, so the call buttons appear before
// WhatsApp rolls web calling out to the account. Nothing is sent anywhere.
// WhatsApp may change these names at any time; then this simply does nothing.
(() => {
  if (location.origin !== "https://web.whatsapp.com") return;
  const FORCED = { enable_web_calling: true, web_voip_call_tab_new_call: true };
  let tries = 0;
  const patch = () => {
    let ab = null;
    try {
      if (typeof window.require === "function") ab = window.require("WAWebABProps");
    } catch (_) {
      ab = null; // module not loaded yet
    }
    if (ab && typeof ab.getABPropConfigValue === "function") {
      if (!ab.getABPropConfigValue.__wach) {
        const original = ab.getABPropConfigValue;
        const wrapped = function (key, ...rest) {
          return Object.prototype.hasOwnProperty.call(FORCED, key) ? FORCED[key] : original.call(this, key, ...rest);
        };
        wrapped.__wach = true;
        ab.getABPropConfigValue = wrapped;
      }
      return;
    }
    if (++tries < 300) setTimeout(patch, 200); // give up after ~60 s
  };
  patch();
})();

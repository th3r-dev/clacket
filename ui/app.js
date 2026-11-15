// Clacket settings window. Talks only to Clacket's own Rust commands over IPC.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);
const h = (tag, attrs = {}, ...kids) => {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === false || v == null) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "text") el.textContent = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  el.append(...kids.flat().filter((k) => k != null));
  return el;
};

let S = null; // last state from get_state
const selected = new Set();
let focusCode = "KeyA";

const FX_LABELS = {
  "mouse:left": "Left click", "mouse:right": "Right click", "mouse:middle": "Middle click", "mouse:up": "Button release",
  scroll: "Scroll tick", move: "Movement", tap: "Tap", tap2: "Two-finger tap", swipe: "Swipe", pinch: "Pinch",
};
const DEVICE_LABELS = {
  usb_storage: "USB storage", phone: "Phone", input: "Keyboard or mouse", audio: "Audio device", usb_other: "Other USB",
  charger: "Charger", headphones: "Headphones", display: "Display", sd_card: "SD card",
};

// ---------- commands ----------
async function call(cmd, args, errorEl) {
  try {
    if (errorEl) errorEl.textContent = "";
    return await invoke(cmd, args);
  } catch (e) {
    if (e === "PRO_REQUIRED") openUpgrade();
    else if (errorEl) errorEl.textContent = e;
    else banner(String(e), null);
    throw e;
  }
}
const quiet = (p) => p.catch(() => {});

async function refresh() {
  S = await invoke("get_state");
  render();
}

// ---------- rendering ----------
function render() {
  const s = S.settings;
  document.documentElement.classList.toggle("mac", S.platform === "macos");
  $("enabled").checked = s.enabled;
  $("enabled-text").textContent = s.enabled ? "On" : "Off";
  $("pro-badge").hidden = !S.pro;
  document.body.classList.toggle("is-pro", S.pro);

  $("preset").replaceChildren(...s.presets.map((p) => h("option", { value: p.name, selected: p.name === s.active_preset, text: p.name })));
  // presets are Pro; free users see a single Default and no preset buttons
  document.querySelector(".preset-actions").hidden = !S.pro;
  $("preset").disabled = !S.pro && s.presets.length < 2;
  $("preset-rename").disabled = $("preset-delete").disabled = s.active_preset === "Default";

  renderBanners();
  renderKeyboard();
  renderSide();
  renderMouse();
  renderSound();
}

function banner(text, action) {
  $("banners").append(h("div", { class: "banner", role: "status" }, h("p", { text }), action));
}

function renderBanners() {
  $("banners").replaceChildren();
  if (!S.input_ok) {
    const mac = S.platform === "macos";
    banner(
      mac ? "Clacket can't hear your keys. Allow Input Monitoring in System Settings, then reopen Clacket."
          : S.platform === "linux" ? "Clacket can't read input devices. Add your user to the input group, then log out and back in."
          : "Clacket can't hear your keys. Restart Clacket.",
      mac ? h("button", { class: "primary", text: "Open System Settings", onclick: () => { quiet(invoke("request_permission")); quiet(invoke("open_link", { which: "permissions" })); } }) : null,
    );
  }
  if (!S.has_output) banner("No output device found. Connect speakers or headphones.", null);
  if (S.license_note) banner(S.license_note, null);
}

// ----- keyboard -----
function layoutId() { return S.settings.layout_override || S.detected_layout; }

function renderKeyboard() {
  const id = layoutId();
  $("layout-name").textContent = `${id.toUpperCase()}${S.settings.layout_override ? "" : ", detected"}`;
  $("layout-select").value = S.settings.layout_override || "";
  const custom = new Set(Object.keys(currentSounds()).filter((k) => k.startsWith("key:")).map((k) => k.slice(4)));
  const main = h("div", { class: "block" });
  const nav = h("div", { class: "block" });
  S.layouts[id].forEach((row, i) => {
    const [left, right = ""] = row.split("|");
    main.append(rowEl(left, i, custom));
    nav.append(rowEl(right, i, custom));
  });
  $("keyboard").replaceChildren(main, nav);
  sizeKeyboard();
}

function rowEl(tokens, i, custom) {
  const row = h("div", { class: "krow" + (i === 0 ? " fn" : ""), role: "row" });
  for (const tok of tokens.trim().split(/\s+/).filter(Boolean)) {
    const [code, w = "1"] = tok.split(":");
    if (code === "_") { const g = h("span", { class: "gap" }); g.style.setProperty("--w", w); row.append(g); continue; }
    const label = legend(code);
    const isCustom = custom.has(code);
    const long = [...label].length > 3 && +w < 1.5;
    const k = h("button", {
      class: "key" + (isCustom ? " custom" : "") + (long ? " long" : ""), role: "gridcell", "data-code": code,
      "aria-label": `${label}${isCustom ? ", custom sound" : ""}`,
      "aria-selected": selected.has(code) ? "true" : "false",
      tabindex: code === focusCode ? "0" : "-1",
      title: label, text: label,
      onclick: (e) => pick(code, e.shiftKey || e.metaKey || e.ctrlKey),
    });
    k.style.setProperty("--w", w);
    row.append(k);
  }
  return row;
}

// modifier legends as printed on this platform's keyboards (display only)
const PLATFORM_LEGENDS = {
  macos: { MetaLeft: "⌘", MetaRight: "⌘", AltLeft: "⌥", AltRight: "⌥", ControlLeft: "⌃", ControlRight: "⌃", Backspace: "⌫", Enter: "return", ShiftLeft: "⇧", ShiftRight: "⇧", CapsLock: "⇪", Tab: "⇥", Escape: "esc" },
  windows: { MetaLeft: "Win", MetaRight: "Win" },
};
function legend(code) { return PLATFORM_LEGENDS[S.platform]?.[code] ?? S.labels[code] ?? code; }

function sizeKeyboard() {
  const kb = $("keyboard");
  const units = 15 + 0.35 + 3.2; // main block + gap + nav cluster
  const u = Math.max(22, Math.min(52, kb.clientWidth / units / 1.1));
  kb.style.setProperty("--u", `${u}px`);
}
new ResizeObserver(() => S && sizeKeyboard()).observe(document.documentElement);

function pick(code, add) {
  if (!add) selected.clear();
  if (add && selected.has(code)) selected.delete(code); else selected.add(code);
  focusCode = code;
  renderKeyboard();
  renderSide();
  document.querySelector(`.key[data-code="${code}"]`)?.focus();
}

// arrow keys move between keys by position; Space/Enter selects (Shift adds)
$("keyboard").addEventListener("keydown", (e) => {
  const keys = [...document.querySelectorAll(".key")];
  const cur = document.activeElement;
  if (!cur?.classList.contains("key")) return;
  if (e.key === " " || e.key === "Enter") { e.preventDefault(); pick(cur.dataset.code, e.shiftKey); return; }
  const dir = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
  if (!dir) return;
  e.preventDefault();
  const r = cur.getBoundingClientRect();
  const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
  let best = null, bestD = Infinity;
  for (const k of keys) {
    if (k === cur) continue;
    const b = k.getBoundingClientRect();
    const dx = b.left + b.width / 2 - cx, dy = b.top + b.height / 2 - cy;
    if (dir[0] && (Math.sign(dx) !== dir[0] || Math.abs(dy) > r.height / 2)) continue;
    if (dir[1] && (Math.sign(dy) !== dir[1] || Math.abs(dy) < r.height / 2)) continue;
    const d = Math.abs(dx) + Math.abs(dy) * 3;
    if (d < bestD) { bestD = d; best = k; }
  }
  if (best) {
    cur.tabIndex = -1; best.tabIndex = 0; best.focus(); focusCode = best.dataset.code;
  }
});

function currentSounds() {
  const p = S.settings.presets.find((p) => p.name === S.settings.active_preset);
  return p ? p.sounds : {};
}
function baseSwitch() {
  return S.settings.presets.find((p) => p.name === S.settings.active_preset)?.base_switch ?? "cream";
}
function switchLabel(id) { return S.switches.find((s) => s.id === id)?.label ?? id; }

function describe(ref) {
  if (!ref) return `${switchLabel(baseSwitch())} (switch default)`;
  if (ref.file) return "Your file";
  const parts = ref.builtin.split("/");
  const sw = parts[0], clip = parts[parts.length - 1];
  if (sw === "fx") return `Clacket ${clip.replace(/_/g, " ")}`;
  const clipName = { GENERIC_R0: "press", SPACE: "space bar", ENTER: "enter", BACKSPACE: "backspace" }[clip] ?? clip;
  return `${switchLabel(sw)}, ${clipName}`;
}

/** <select> of every sound: default, then built-in clips grouped by switch, then fx. */
function soundSelect(currentRef, onchange, label) {
  const opts = [h("option", { value: "", text: "Default" })];
  const groups = {};
  for (const n of S.builtin) {
    const [sw] = n.split("/");
    if (n.includes("GENERIC_R") && !n.endsWith("GENERIC_R0")) continue;
    (groups[sw] ??= []).push(n);
  }
  for (const [sw, names] of Object.entries(groups)) {
    const g = h("optgroup", { label: sw === "fx" ? "Clacket effects" : switchLabel(sw) });
    for (const n of names) g.append(h("option", { value: n, text: describe({ builtin: n }), selected: currentRef?.builtin === n }));
    opts.push(g);
  }
  if (currentRef?.file) opts.push(h("option", { value: "__file", text: "Your file", selected: true }));
  return h("select", { "aria-label": label, onchange: (e) => onchange(e.target.value) }, opts);
}

function renderSide() {
  const side = $("side");
  if (!selected.size) {
    side.replaceChildren(h("h3", { text: "Pick a key" }), h("p", { class: "now", text: "Select a key to change its sound. Press it on your keyboard or click it here." }));
    return;
  }
  const codes = [...selected];
  const title = codes.length === 1 ? legend(codes[0]) : `${codes.length} keys`;
  if (!S.pro) {
    side.replaceChildren(
      h("h3", { text: title }),
      h("p", { class: "now", text: "Give each key its own sound with Clacket Pro, $5 once." }),
      h("div", { class: "stack" },
        h("button", { class: "primary", text: "Upgrade – $5", onclick: () => quiet(invoke("open_link", { which: "buy" })) }),
        h("button", { text: "I have a key", onclick: openUpgrade })),
    );
    return;
  }
  const ids = codes.map((c) => "key:" + c);
  const sounds = currentSounds();
  const refs = ids.map((i) => sounds[i]);
  const same = refs.every((r) => JSON.stringify(r) === JSON.stringify(refs[0]));
  const err = h("p", { class: "error", role: "alert" });
  side.replaceChildren(
    h("h3", { text: title }),
    h("p", { class: "now", text: same ? describe(refs[0]) : "Mixed sounds" }),
    h("div", { class: "stack" },
      soundSelect(same ? refs[0] : null, (v) => {
        if (v === "__file") return;
        quiet(call("set_sound", { ids, sound: v ? { builtin: v } : null }, err).then(() => quiet(call("preview", { event: ids[0] }))));
      }, `Sound for ${title}`),
      h("button", { text: "Choose file…", onclick: () => quiet(call("import_sound", { ids }, err)) }),
      h("button", { text: "Preview", onclick: () => quiet(call("preview", { event: ids[0] }, err)) }),
      h("button", { text: "Reset to default", disabled: refs.every((r) => !r), onclick: () => quiet(call("set_sound", { ids, sound: null }, err)) }),
      err),
  );
}

// ----- mouse & devices -----
// ----- mouse & devices: icon tiles -----
const FX_FEEL = {
  "mouse:left": "Left click", "mouse:right": "Right click", "mouse:middle": "Middle click",
  "mouse:up": "Button release", scroll: "Scroll tick", move: "Pointer movement",
  tap: "Tap to click", tap2: "Two-finger tap", swipe: "Swipe", pinch: "Pinch",
};
// schematic line icons; .hl fills with the accent. 24x24, currentColor stroke.
function fxIcon(id) {
  const S = (inner) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${inner}</svg>`;
  const mouse = (hl) => S(`<rect x="6" y="2.5" width="12" height="19" rx="6"/><line x1="12" y1="3.2" x2="12" y2="10"/>${hl}`);
  const dev = id.startsWith("plug:") ? id.split(":")[1] : null;
  const M = {
    "mouse:left": mouse(`<path class="hl" d="M12 3.4 A5.6 5.6 0 0 0 6.4 9 H12 Z"/>`),
    "mouse:right": mouse(`<path class="hl" d="M12 3.4 A5.6 5.6 0 0 1 17.6 9 H12 Z"/>`),
    "mouse:middle": mouse(`<rect class="hl" x="11" y="4.5" width="2" height="4.5" rx="1" stroke="none"/>`),
    "mouse:up": mouse(`<path class="hl" d="M9.8 6.5 l2.2 -2.2 l2.2 2.2" fill="none" stroke="var(--accent)"/>`),
    scroll: mouse(`<rect x="11" y="4.6" width="2" height="4" rx="1"/><path class="hl" d="M10 4 l2 -1.6 l2 1.6 M10 9.4 l2 1.6 l2 -1.6" stroke="var(--accent)"/>`),
    move: S(`<path class="hl" d="M5 3.5 l6.5 15 l2.3 -5.7 l5.7 -2.3 Z" stroke="none"/><path d="M3 10 h2.5 M3 13.5 h3.2"/>`),
    tap: S(`<circle cx="12" cy="12" r="6.5"/><circle class="hl" cx="12" cy="12" r="2.6" stroke="none"/>`),
    tap2: S(`<circle cx="12" cy="12" r="7"/><circle class="hl" cx="9" cy="12" r="2.1" stroke="none"/><circle class="hl" cx="15" cy="12" r="2.1" stroke="none"/>`),
    swipe: S(`<path d="M3.5 15 q8.5 -10 16.5 -3.5"/><path class="hl" d="M20 11.5 l0.6 4.2 l-4.2 -0.8" stroke="var(--accent)"/>`),
    pinch: S(`<circle class="hl" cx="12" cy="12" r="1.8" stroke="none"/><path d="M4 4 l4.5 4.5 M8.5 4 H4 V8.5 M20 20 l-4.5 -4.5 M15.5 20 H20 V15.5"/>`),
  };
  const D = {
    usb_storage: `<rect x="7" y="8" width="10" height="11.5" rx="1.5"/><rect x="10" y="4" width="4" height="4"/><path class="hl" d="M10.6 11.5 v4.5 M13.4 11.5 v4.5" stroke="var(--accent)"/>`,
    phone: `<rect x="7" y="2.5" width="10" height="19" rx="2.5"/><line x1="10.5" y1="18.7" x2="13.5" y2="18.7"/>`,
    input: `<rect x="3" y="7" width="18" height="10" rx="2"/><path class="hl" d="M6.5 10.3 h1 M10 10.3 h1 M13.5 10.3 h1 M17 10.3 h0.5 M8 13.6 h8" stroke="var(--accent)"/>`,
    audio: `<path d="M4 9 h3.5 l5 -4 v14 l-5 -4 H4 Z"/><path class="hl" d="M16 9.2 a4 4 0 0 1 0 5.6" stroke="var(--accent)"/>`,
    usb_other: `<path d="M12 21 V5"/><circle class="hl" cx="12" cy="3.6" r="1.6" stroke="none"/><path d="M12 12 l3.6 -2.6 M15.6 9.4 v2.8 h-2.2 M12 15.4 l-3.6 -2.6 M8.4 12.8 v1.8"/>`,
    charger: `<path class="hl" d="M13 2.5 L5.5 13 h4.8 l-1 8.5 L18.5 10.5 h-4.8 Z" stroke="none"/>`,
    headphones: `<path d="M5 14 v-2 a7 7 0 0 1 14 0 v2"/><rect class="hl" x="3.4" y="13.4" width="3.6" height="6.2" rx="1.6" stroke="none"/><rect class="hl" x="17" y="13.4" width="3.6" height="6.2" rx="1.6" stroke="none"/>`,
    display: `<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M9 20 h6 M12 16 v4"/>`,
    sd_card: `<path d="M8.5 3 H15 a2 2 0 0 1 2 2 v14 a2 2 0 0 1 -2 2 H9 a2 2 0 0 1 -2 -2 V6.5 Z M8.5 3 L7 5 v1.5"/><path class="hl" d="M9.6 5 v2 M11.6 5 v2 M13.6 5 v2" stroke="var(--accent)"/>`,
  };
  if (dev) return S(D[dev] || "");
  return M[id] || S("");
}

function fxTile(id, label, chip, dir) {
  const on = !S.settings.off.includes(id);
  const ref = currentSounds()[id];
  const lock = !S.pro;
  const err = h("p", { class: "error", role: "alert" });
  const ico = h("span", { class: "fx-ico", "aria-hidden": "true" });
  ico.innerHTML = fxIcon(id);
  const tile = h("div", { class: "fxtile" + (lock ? " locked" : ""), "data-custom": ref ? "1" : null, "data-dir": dir || null },
    chip ? h("span", { class: "fx-chip" + (dir ? " " + dir : ""), text: chip }) : null,
    h("label", { class: "fx-toggle", title: on ? "On" : "Off" },
      h("input", { type: "checkbox", checked: on, disabled: lock, "aria-label": `${label} sound on`,
        onchange: (e) => quiet(call("set_event_on", { ids: [id], on: e.target.checked }, err)) }),
      h("span", { class: "fx-sw", "aria-hidden": "true" })),
    h("button", { class: "fx-preview", type: "button", "aria-label": `Preview ${label}`,
      onclick: () => { tile.classList.remove("hit"); void tile.offsetWidth; tile.classList.add("hit"); quiet(call("preview", { event: id }, err)); } },
      ico, h("span", { class: "fx-label", text: label })),
    (() => {
      const sel = soundSelect(ref, (v) => v !== "__file" && quiet(call("set_sound", { ids: [id], sound: v ? { builtin: v } : null }, err)), `Sound for ${label}`);
      sel.disabled = lock;
      const d = h("details", { class: "fx-change" }, h("summary", { text: "Change sound" }), sel,
        h("button", { text: "Choose file…", disabled: lock, onclick: () => quiet(call("import_sound", { ids: [id] }, err)) }), err);
      return d;
    })(),
  );
  return tile;
}

function renderMouse() {
  const panel = $("panel-mouse");
  panel.classList.toggle("locked", !S.pro);
  $("mouse-upsell").replaceChildren(...(S.pro ? [] : [h("div", { class: "upsell" },
    h("p", { text: "Mouse, trackpad and plug-in sounds come with Clacket Pro. You can preview any of them now." }),
    h("button", { class: "primary", text: "Upgrade – $5", onclick: openUpgrade }))]));
  for (const k of ["scroll_sensitivity", "move_volume"]) { $(k).value = S.settings[k]; $(k).disabled = !S.pro; }
  $("haptic").checked = S.settings.haptic;
  $("haptic").disabled = !S.pro;
  $("haptic-row").hidden = !S.haptic_supported;
  let mouse = Object.keys(FX_FEEL).filter((id) => S.fx_events.includes(id));
  if (S.platform !== "macos") mouse = mouse.filter((id) => !["tap", "tap2", "swipe", "pinch"].includes(id));
  $("mouse-rows").replaceChildren(...mouse.map((id) => fxTile(id, FX_FEEL[id])));
  $("device-rows").replaceChildren(...Object.entries(DEVICE_LABELS).flatMap(([cat, label]) => [
    fxTile(`plug:${cat}:in`, label, "Plugged in", "in"),
    fxTile(`plug:${cat}:out`, label, "Unplugged", "out"),
  ]));
  if (S.last_event) $("last-event").textContent = S.last_event;
}

// ----- sound & options -----
// How each switch looks and feels (display copy and drawing colors only)
const SWITCH_LOOK = {
  alpaca:    { stem: "#d99ac4", top: "#c9b9d6", feel: "Smooth and muted" },
  blackink:  { stem: "#1f2023", top: "#6b6f78", feel: "Deep, heavy thock" },
  redink:    { stem: "#d7383b", top: "#7a7f88", feel: "Light and smooth" },
  cream:     { stem: "#efe0c2", top: "#efe0c2", feel: "Buttery, rounded" },
  mxblack:   { stem: "#1d1d1f", top: "#2e2f33", feel: "Classic heavy linear" },
  holypanda: { stem: "#f39a6b", top: "#e9e6df", feel: "Big bump, loud thock" },
  mxbrown:   { stem: "#7a4a2a", top: "#2e2f33", feel: "Gentle bump, quiet" },
  turquoise: { stem: "#2bb3a8", top: "#d6ece9", feel: "Snappy bump" },
  topre:     { kind: "dome", stem: "#9a9ca3", top: "#3b3c41", feel: "Cushioned dome" },
  mxblue:    { stem: "#2f6fd6", top: "#2e2f33", feel: "The classic click" },
  boxnavy:   { stem: "#1f2c5c", top: "#2e2f33", feel: "Thick, loud click" },
  bluealps:  { kind: "alps", stem: "#4a86e8", top: "#e8e4da", feel: "Sharp vintage click" },
  buckling:  { kind: "spring", stem: "#b8bcc4", top: "#3b3c41", feel: "Springy IBM clang" },
};
const NS = "http://www.w3.org/2000/svg";
const svg = (tag, attrs) => { const el = document.createElementNS(NS, tag); for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v); return el; };

/** A small front view of the switch: housing plus the moving part in its real color. */
function switchArt(id) {
  const L = SWITCH_LOOK[id] ?? { stem: "#999", top: "#555" };
  const s = svg("svg", { viewBox: "0 0 64 64", "aria-hidden": "true" });
  const stem = svg("g", { class: "stem" });
  if (L.kind === "dome") {
    // rubber dome sits above the housing; it squashes down when pressed
    stem.append(svg("path", { d: "M17 28 C17 9, 47 9, 47 28 Z", fill: L.stem }), svg("rect", { x: 28, y: 6, width: 8, height: 8, rx: 2, fill: L.stem }));
  } else if (L.kind === "spring") {
    stem.append(svg("path", { d: "M32 10 l-9 4 l18 4 l-18 4 l18 4 l-18 4 l18 4 l-9 4", fill: "none", stroke: L.stem, "stroke-width": "3", "stroke-linejoin": "round", "stroke-linecap": "round" }));
  } else if (L.kind === "alps") {
    stem.append(svg("rect", { x: 24, y: 10, width: 16, height: 18, rx: 2, fill: L.stem }));
  } else {
    stem.append(svg("rect", { x: 27, y: 8, width: 10, height: 20, rx: 2, fill: L.stem }), svg("rect", { x: 22, y: 12, width: 20, height: 6, rx: 2, fill: L.stem }));
  }
  s.append(
    stem,
    svg("path", { d: "M12 26 H52 L56 42 H8 Z", fill: L.top, stroke: "rgb(0 0 0 / .25)", "stroke-width": "1" }),
    svg("rect", { x: 8, y: 42, width: 48, height: 12, rx: 2, fill: "#2a2b30" }),
    svg("rect", { x: 22, y: 54, width: 3, height: 6, fill: "#c9a227" }), svg("rect", { x: 39, y: 54, width: 3, height: 6, fill: "#c9a227" }),
  );
  return s;
}

function pressCard(card) {
  card.classList.add("pressing");
  setTimeout(() => card.classList.remove("pressing"), 120);
}

function renderSound() {
  const base = baseSwitch();
  const groups = {};
  for (const sw of S.switches) (groups[sw.group] ??= []).push(sw);
  $("switches").replaceChildren(...Object.entries(groups).flatMap(([g, list]) => [
    h("p", { class: "group-name", text: g }),
    h("div", { class: "cards" }, list.map((sw) => {
      const card = h("button", {
        class: "card" + (sw.locked ? " locked" : ""), role: "radio", "aria-checked": sw.id === base ? "true" : "false",
        "aria-label": `${sw.label}, ${g.toLowerCase()}, ${SWITCH_LOOK[sw.id]?.feel ?? ""}${sw.locked ? ", Pro" : ""}`,
        tabindex: sw.id === base ? "0" : "-1",
        onclick: () => {
          pressCard(card);
          // always let them hear it; a locked switch then opens the upgrade sheet
          if (sw.locked) quiet(call("set_switch", { id: sw.id }));
          else quiet(call("preview", { sound: { builtin: `${sw.id}/press/GENERIC_R0` } }).then(() => sw.id !== base && quiet(call("set_switch", { id: sw.id }))));
        },
      }, switchArt(sw.id),
        h("span", { class: "name" }, sw.label, sw.locked ? h("span", { class: "tag", text: "Pro" }) : null),
        h("span", { class: "feel", text: SWITCH_LOOK[sw.id]?.feel ?? g }));
      return card;
    })),
  ]));
  $("volume").value = S.settings.volume;
  $("volume-out").textContent = `${Math.round(S.settings.volume * 100)}%`;
  $("keyup_sound").checked = S.settings.keyup_sound;
  $("launch_at_login").checked = S.settings.launch_at_login;
  $("shortcut").textContent = S.platform === "macos" ? "⌥ ⌘ K" : "Ctrl + Alt + K";
  $("license").replaceChildren(...(S.pro
    ? [h("p", { text: `Pro · Licensed to ${S.licensed_to}` }),
       h("button", { id: "deactivate", text: "Deactivate this device", onclick: deactivateClick }),
       h("p", { class: "hint", text: "Frees this key so you can use it on another computer." })]
    : [h("p", { class: "hint", text: "Free: 4 switches and the basics." }), h("button", { class: "primary", text: "Upgrade – $5", onclick: openUpgrade })]));
}

// arrow keys move between switch cards (radio group pattern)
$("switches").addEventListener("keydown", (e) => {
  const d = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
  if (!d) return;
  e.preventDefault();
  const cards = [...document.querySelectorAll(".card")];
  const i = cards.indexOf(document.activeElement);
  const next = cards[(i + d + cards.length) % cards.length];
  cards.forEach((c) => (c.tabIndex = c === next ? 0 : -1));
  next.focus();
});

// "Try it": the switch plays through the global listener; here the typed keys appear as caps
$("try").addEventListener("keydown", (e) => {
  if (e.key.length !== 1) return;
  const caps = $("caps");
  caps.querySelector(".placeholder")?.remove();
  const cap = h("span", { class: "cap", text: e.key.toUpperCase() });
  caps.append(cap);
  while (caps.children.length > 7) caps.firstChild.remove();
  for (const c of document.querySelectorAll(".card[aria-checked='true']")) pressCard(c);
});
$("volume").addEventListener("input", (e) => { $("volume-out").textContent = `${Math.round(e.target.value * 100)}%`; });

// ---------- controls ----------
const opt = (key, value) => quiet(call("set_option", { key, value }));
$("enabled").addEventListener("change", (e) => opt("enabled", e.target.checked));
$("keyup_sound").addEventListener("change", (e) => opt("keyup_sound", e.target.checked));
$("launch_at_login").addEventListener("change", (e) => opt("launch_at_login", e.target.checked));
$("haptic").addEventListener("change", (e) => opt("haptic", e.target.checked));
$("volume").addEventListener("change", (e) => opt("volume", +e.target.value).then(() => quiet(invoke("preview", { sound: { builtin: `${baseSwitch()}/press/GENERIC_R0` } }))));
for (const k of ["scroll_sensitivity", "move_volume"]) $(k).addEventListener("change", (e) => opt(k, +e.target.value));
$("preset").addEventListener("change", (e) => quiet(call("preset", { action: "activate", name: e.target.value }).catch(() => refresh())));
$("layout-change").addEventListener("click", () => { $("layout-select").hidden = false; $("layout-select").focus(); });
$("layout-select").addEventListener("change", (e) => { opt("layout_override", e.target.value || null); $("layout-select").hidden = true; });

// tabs (arrow keys move between them)
const tabs = [...document.querySelectorAll('[role="tab"]')];
function showTab(t) {
  for (const x of tabs) {
    const on = x === t;
    x.setAttribute("aria-selected", on);
    x.tabIndex = on ? 0 : -1;
    $(x.getAttribute("aria-controls")).hidden = !on;
  }
  t.focus();
}
tabs.forEach((t, i) => {
  t.addEventListener("click", () => showTab(t));
  t.addEventListener("keydown", (e) => {
    const d = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (d) showTab(tabs[(i + d + tabs.length) % tabs.length]);
  });
});

// preset name dialog
function ask(title, value, action) {
  $("ask-title").textContent = title;
  $("ask-input").value = value;
  $("ask-error").textContent = "";
  const d = $("ask");
  d.onclose = null;
  $("ask-ok").onclick = async (e) => {
    e.preventDefault();
    try { await action($("ask-input").value); d.close(); } catch (err) { $("ask-error").textContent = err; }
  };
  d.showModal();
  $("ask-input").select();
}
$("preset-save").addEventListener("click", () => ask("Save as new preset", "", (name) => invoke("preset", { action: "save_as", name })));
$("preset-rename").addEventListener("click", () => ask("Rename preset", S.settings.active_preset, (to) => invoke("preset", { action: "rename", name: S.settings.active_preset, newName: to })));
$("preset-delete").addEventListener("click", () => quiet(call("preset", { action: "delete", name: S.settings.active_preset })));

// Activation runs on the server (one device per key), so the app shows progress and waits for the
// `license-result` event rather than a direct return. Messages the UI shows for each outcome:
const LICENSE_MSG = {
  in_use: "This key is already active on another device. Open Clacket there and click “Deactivate this device,” then try again.",
  offline: "Clacket couldn’t reach the license server. Connect to the internet and try again.",
  invalid: "This key isn’t valid. Copy the whole key, starting with CLACKET-.",
  revoked: "This key was refunded, so it can’t turn on Pro.",
};

function setActivating(on) {
  const b = $("activate");
  b.disabled = on;
  b.textContent = on ? "Activating…" : "Activate key";
  if (on) $("key-error").textContent = "";
}

function deactivateClick() {
  const b = $("deactivate");
  if (b) { b.disabled = true; b.textContent = "Deactivating…"; }
  quiet(invoke("remove_license"));
}

// upgrade dialog
function openUpgrade() { $("key-error").textContent = ""; if (!$("upgrade").open) $("upgrade").showModal(); }
$("buy").addEventListener("click", () => quiet(invoke("open_link", { which: "buy" })));
$("recover").addEventListener("click", () => quiet(invoke("open_link", { which: "recover" })));
$("activate").addEventListener("click", () => {
  const key = $("key-input").value.trim();
  if (!key) { $("key-error").textContent = "Paste your key, or drop your clacket.license file here."; return; }
  setActivating(true);
  quiet(invoke("activate_license", { key })); // result arrives as a license-result event
});
const ta = $("key-input");
ta.addEventListener("dragover", (e) => { e.preventDefault(); ta.classList.add("drop"); });
ta.addEventListener("dragleave", () => ta.classList.remove("drop"));
ta.addEventListener("drop", async (e) => {
  e.preventDefault();
  ta.classList.remove("drop");
  const f = e.dataTransfer.files[0];
  if (f && f.size < 4096) ta.value = (await f.text()).trim();
});

// ---------- live events ----------
listen("state-changed", refresh);
listen("show-upgrade", openUpgrade);
listen("activating", () => { openUpgrade(); setActivating(true); }); // from a clacket://activate deep link
listen("license-result", (e) => {
  const r = e.payload;
  if (r.action === "deactivate") {
    banner(r.ok
      ? "Pro is off on this device. Your key is free to use on another computer."
      : "Clacket couldn’t reach the license server, so Pro is still on here. Moving a license has to happen online — connect to the internet and try again.", null);
    refresh(); // restore the Deactivate button
    return;
  }
  setActivating(false);
  if (r.ok) { $("upgrade").close(); $("key-input").value = ""; banner(`Clacket Pro is on. Licensed to ${r.name}. Thanks!`, null); }
  else { openUpgrade(); $("key-error").textContent = LICENSE_MSG[r.error] || "Activation didn’t work. Try again."; }
});
listen("device-event", (e) => { $("last-event").textContent = e.payload; });
listen("import-error", (e) => { const m = Array.isArray(e.payload) ? e.payload[1] : String(e.payload); banner(m, null); });
// a key-up can go missing (macOS Secure Input hides it), so a lit key also clears itself
const unlight = {};
listen("key-event", (e) => {
  const { code, down } = e.payload;
  for (const k of document.querySelectorAll(`.key[data-code="${code}"]`)) k.classList.toggle("down", down);
  clearTimeout(unlight[code]);
  if (down) unlight[code] = setTimeout(() => document.querySelectorAll(`.key[data-code="${code}"]`).forEach((k) => k.classList.remove("down")), 1500);
});

// macOS: window buttons appear only while the pointer is near the top edge
let controlsShown = false;
function controls(show) {
  if (show === controlsShown) return;
  controlsShown = show;
  quiet(invoke("window_controls", { visible: show }));
}
document.addEventListener("mousemove", (e) => controls(e.clientY < 36));
document.documentElement.addEventListener("mouseleave", () => controls(false));
window.addEventListener("blur", () => controls(false));

refresh();

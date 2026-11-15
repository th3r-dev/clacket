// Browser demo of Clacket's switches. Same recordings as the app, packed by
// scripts/pack-site.mjs into encrypted blobs; PACK below is filled in at build time with
// { seed, sounds: { id: { url, clips: [[phase/name, bytes]] } }, images: { name: url } }.
const PACK = /*PACK*/null;
const SWITCHES = [
  ["cream", "NK Cream", "#efe0c2", "Buttery, rounded"],
  ["holypanda", "Holy Panda", "#f39a6b", "Big bump, loud thock"],
  ["mxblue", "MX Blue", "#2f6fd6", "The classic click"],
  ["mxbrown", "MX Brown", "#7a4a2a", "Gentle bump, quiet"],
  ["topre", "Topre", "#9a9ca3", "Cushioned dome"],
  ["alpaca", "Alpaca", "#d99ac4", "Smooth and muted"],
  ["blackink", "Black Ink", "#1f2023", "Deep, heavy thock"],
  ["redink", "Red Ink", "#d7383b", "Light and smooth"],
  ["mxblack", "MX Black", "#1d1d1f", "Classic heavy linear"],
  ["turquoise", "Turquoise", "#2bb3a8", "Snappy bump"],
  ["boxnavy", "Box Navy", "#1f2c5c", "Thick, loud click"],
  ["bluealps", "Blue Alps", "#4a86e8", "Sharp vintage click"],
  ["buckling", "Buckling Spring", "#b8bcc4", "Springy IBM clang"],
];
// on-screen rows: [code, label, width]
const ROWS = [
  "Backquote:` Digit1:1 Digit2:2 Digit3:3 Digit4:4 Digit5:5 Digit6:6 Digit7:7 Digit8:8 Digit9:9 Digit0:0 Minus:- Equal:= Backspace:⌫:2",
  "Tab:⇥:1.5 KeyQ:Q KeyW:W KeyE:E KeyR:R KeyT:T KeyY:Y KeyU:U KeyI:I KeyO:O KeyP:P BracketLeft:[ BracketRight:] Backslash:\\:1.5",
  "CapsLock:⇪:1.75 KeyA:A KeyS:S KeyD:D KeyF:F KeyG:G KeyH:H KeyJ:J KeyK:K KeyL:L Semicolon:; Quote:' Enter:return:2.25",
  "ShiftLeft:⇧:2.25 KeyZ:Z KeyX:X KeyC:C KeyV:V KeyB:B KeyN:N KeyM:M Comma:, Period:. Slash:/ ShiftRight:⇧:2.75",
  "Space:space:6.25",
].map((r) => r.split(" ").map((t) => { const p = t.split(":"); return [p[0], p[1], +(p[2] || 1)]; }));

const $ = (id) => document.getElementById(id);
let ctx, master, current = "cream", muted = false;
const banks = {}; // switch id -> { press: {GENERIC:[buffers], SPACE..}, release: {...} }

function audio() {
  if (!ctx) {
    ctx = new (window.AudioContext || window.webkitAudioContext)();
    master = ctx.createGain();
    master.gain.value = +$("vol").value;
    master.connect(ctx.destination);
  }
  if (ctx.state === "suspended") ctx.resume();
  return ctx;
}

// blobs are iv(12) || AES-GCM ciphertext; decrypted in memory only, never given a URL
let keyP;
async function unseal(url) {
  keyP ??= crypto.subtle.digest("SHA-256", new TextEncoder().encode(`clacket:${PACK.seed}`))
    .then((raw) => crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["decrypt"]));
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  const buf = await res.arrayBuffer();
  return crypto.subtle.decrypt({ name: "AES-GCM", iv: buf.slice(0, 12) }, await keyP, buf.slice(12));
}

// decoding needs no running context, so the preload doesn't start audio before a gesture
let decoder;
const decode = (bytes) => (ctx ?? (decoder ??= new (window.OfflineAudioContext || window.webkitOfflineAudioContext)(2, 1, 48000))).decodeAudioData(bytes);

const loading = {};
function loadBank(id) {
  loading[id] ??= (async () => {
    const all = await unseal(PACK.sounds[id].url);
    const bank = { press: {}, release: {} };
    let at = 0;
    await Promise.all(PACK.sounds[id].clips.map(async ([path, len]) => {
      const bytes = all.slice(at, at += len);
      const [phase, name] = path.split("/");
      const kind = name.startsWith("GENERIC") ? "GENERIC" : name;
      const buf = await decode(bytes);
      bank[phase][kind] ??= [];
      bank[phase][kind].push(buf);
    }));
    return (banks[id] = bank);
  })().catch((e) => { delete loading[id]; throw e; });
  return loading[id];
}

const KIND = { Space: "SPACE", Enter: "ENTER", Backspace: "BACKSPACE" };
const kindOf = (code) => KIND[code] ?? "GENERIC";

function play(code, down) {
  if (muted) return;
  const bank = banks[current];
  if (!bank) return;
  const set = bank[down ? "press" : "release"];
  const list = set[kindOf(code)] ?? set.GENERIC;
  if (!list?.length) return;
  const src = ctx.createBufferSource();
  src.buffer = list[Math.floor(Math.random() * list.length)];
  src.playbackRate.value = 1 + (Math.random() - 0.5) * 0.06; // same ±3% as the app
  const g = ctx.createGain();
  g.gain.value = down ? 1 + (Math.random() - 0.5) * 0.16 : 0.6;
  src.connect(g).connect(master);
  src.start();
}

function light(code, down) {
  for (const k of document.querySelectorAll(`.k[data-code="${code}"]`)) k.classList.toggle("down", down);
}

// switch picker
function pick(id) {
  current = id;
  const [, label, , feel] = SWITCHES.find((s) => s[0] === id);
  for (const b of document.querySelectorAll(".sw")) {
    const on = b.dataset.id === id;
    b.setAttribute("aria-checked", on);
    b.tabIndex = on ? 0 : -1;
  }
  $("now").textContent = `${label} · ${feel}`;
  return loadBank(id);
}
$("switches").append(...SWITCHES.map(([id, label, stem]) => {
  const b = document.createElement("button");
  b.className = "sw"; b.dataset.id = id; b.setAttribute("role", "radio");
  b.style.setProperty("--stem", stem);
  b.innerHTML = `<span class="dot" aria-hidden="true"></span>`;
  b.append(label);
  b.addEventListener("click", async () => { audio(); await pick(id); play("KeyA", true); });
  return b;
}));
$("switches").addEventListener("keydown", (e) => {
  const d = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
  if (!d) return;
  e.preventDefault();
  const all = [...document.querySelectorAll(".sw")];
  const next = all[(all.indexOf(document.activeElement) + d + all.length) % all.length];
  next.focus(); next.click();
});

// on-screen keyboard
for (const row of ROWS) {
  const r = document.createElement("div");
  r.className = "row";
  for (const [code, label, w] of row) {
    const k = document.createElement("button");
    k.className = "k"; k.dataset.code = code; k.textContent = label;
    k.setAttribute("aria-label", code === "Space" ? "Space" : label);
    k.style.setProperty("--w", w);
    k.addEventListener("pointerdown", async (e) => { e.preventDefault(); audio(); await loadBank(current); light(code, true); play(code, true); });
    const up = () => { if (k.classList.contains("down")) { light(code, false); play(code, false); } };
    k.addEventListener("pointerup", up);
    k.addEventListener("pointerleave", up);
    r.append(k);
  }
  $("board").append(r);
}

// typing anywhere on the page plays the switch (shortcuts and form fields are left alone)
const held = new Set();
addEventListener("keydown", async (e) => {
  if (e.metaKey || e.ctrlKey || e.altKey || e.target.closest?.("input, textarea, select")) return;
  if (e.target.closest?.(".sw") && e.key.startsWith("Arrow")) return;
  // Space/Enter on a focused link or button should still activate it, silently
  if ((e.code === "Space" || e.code === "Enter") && e.target.closest?.("a, button, summary")) return;
  if (e.code === "Space" || e.code === "Backspace") e.preventDefault(); // don't scroll the page
  if (held.has(e.code)) return; // auto-repeat
  held.add(e.code);
  audio();
  await loadBank(current);
  light(e.code, true);
  play(e.code, true);
});
addEventListener("keyup", (e) => {
  if (!held.delete(e.code)) return;
  light(e.code, false);
  play(e.code, false);
});
addEventListener("blur", () => {
  for (const c of held) light(c, false);
  held.clear();
});

$("vol").addEventListener("input", (e) => { if (master) master.gain.value = +e.target.value; });
$("mute").addEventListener("click", (e) => {
  muted = !muted;
  e.currentTarget.setAttribute("aria-pressed", muted);
  e.currentTarget.textContent = muted ? "Unmute" : "Mute";
});

// lead with the visitor's own platform
const ua = navigator.userAgentData?.platform ?? navigator.userAgent;
const os = [["mac", /Mac/], ["windows", /Win/], ["linux", /Linux|X11/]].find(([, re]) => re.test(ua))?.[0] ?? "";
const mine = document.querySelector(`.dl [data-os="${os}"]`);
if (mine) {
  for (const a of document.querySelectorAll(".dl .btn")) a.classList.add("ghost");
  mine.classList.remove("ghost");
  mine.parentElement.prepend(mine);
}

// screenshots: decrypted and drawn onto their canvas (no image URL to save or open)
for (const c of document.querySelectorAll("canvas[data-img]")) {
  unseal(PACK.images[c.dataset.img])
    .then((bytes) => createImageBitmap(new Blob([bytes], { type: "image/png" })))
    .then((bmp) => { c.getContext("2d").drawImage(bmp, 0, 0, c.width, c.height); bmp.close(); })
    .catch((e) => console.warn("screenshot", c.dataset.img, e));
}

pick("cream").catch(() => {}); // sounds load lazily; a failed preload retries on first key

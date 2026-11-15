// Generates src-tauri/data/keys.toon from Chromium's dom_code_data.inc (BSD-3-Clause), the table
// browsers use to map native key codes to KeyboardEvent.code. No key table is written by hand.
//   node scripts/gen-keys.mjs            (downloads the pinned file)
//   node scripts/gen-keys.mjs <file.inc> (uses a local copy)
import { readFileSync, writeFileSync } from "node:fs";

const REV = "main"; // pin to a commit when refreshing for a release
const SRC_URL = `https://chromium.googlesource.com/chromium/src/+/${REV}/ui/events/keycodes/dom/dom_code_data.inc?format=TEXT`;

const src = process.argv[2]
  ? readFileSync(process.argv[2], "utf8")
  : Buffer.from(await (await fetch(SRC_URL)).text(), "base64").toString("utf8");

// DOM_CODE(usb, evdev, xkb, win, mac, "code", ID), // optional US legend comment like "aA"
const re = /DOM_CODE\((0x[0-9a-f]+),\s*(0x[0-9a-f]+),\s*(0x[0-9a-f]+),\s*(0x[0-9a-f]+),\s*(0x[0-9a-f]+),\s*"(\w+)",\s*\w+\)(?:,\s*\/\/\s*(\S{1,2})\s*$)?/gim;

// Label from the code name: "ShiftLeft" -> "Shift", "NumpadAdd" -> "+", "Digit1" -> "1".
// SHORT holds display abbreviations only (what is printed on a keycap), not key mappings.
const SHORT = {
  Escape: "Esc", BracketLeft: "[", BracketRight: "]", CapsLock: "Caps", Control: "Ctrl", PrintScreen: "PrtSc", ScrollLock: "ScrLk",
  Insert: "Ins", Delete: "Del", PageUp: "PgUp", PageDown: "PgDn", ContextMenu: "Menu", NumLock: "Num",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Add: "+", Subtract: "−", Multiply: "×", Divide: "÷",
  Decimal: ".", Equal: "=", Enter: "Enter", Lang1: "かな", Lang2: "英数", KanaMode: "カナ", Convert: "変換", NonConvert: "無変換",
};
const label = (code, legend) => {
  if (legend) return legend[0].toUpperCase(); // unshifted legend: "aA" -> "A", "1!" -> "1"
  if (SHORT[code]) return SHORT[code];
  const side = code.match(/^(Shift|Control|Alt|Meta)(Left|Right)$/);
  const base = side ? side[1] : code.replace(/^(Key|Digit|Numpad|Intl|Audio|Media|Browser|Launch)(?=.)/, "");
  return SHORT[base] ?? base.replace(/([a-z])([A-Z])/g, "$1 $2");
};

const keys = [];
for (const m of src.matchAll(re)) {
  const [, , evdev, , win, mac, code, legend] = m;
  const n = (h) => parseInt(h, 16);
  keys.push({
    code,
    label: label(code, legend),
    mac: n(mac) !== 0xffff ? n(mac) : null,
    win: n(win) || null,
    evdev: n(evdev) || null,
  });
}
if (keys.length < 100 || !keys.some((k) => k.code === "KeyA" && k.mac === 0)) {
  throw new Error(`parse failed: ${keys.length} keys`);
}
// TOON tabular array: one header, one comma-separated row per key (about half the size of JSON)
const cell = (v) => (v === null ? "null" : typeof v === "number" ? String(v) : /^[\w ]+$/.test(v) && !/^(true|false|null|-?\d)/.test(v) ? v : JSON.stringify(v));
const rows = keys.map((k) => "  " + [k.code, k.label, k.mac, k.win, k.evdev].map(cell).join(","));
const out = new URL("../src-tauri/data/keys.toon", import.meta.url);
writeFileSync(out, `keys[${keys.length}]{code,label,mac,win,evdev}:\n${rows.join("\n")}\n`);
console.log(`src-tauri/data/keys.toon: ${keys.length} keys`);

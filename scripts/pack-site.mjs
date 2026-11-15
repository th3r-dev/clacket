// Packs the site's switch recordings and screenshots into AES-256-GCM blobs under
// site/dist/a/, so dist holds no .mp3 or screenshot .png that opens or downloads by URL.
// Each build draws a fresh seed; demo.js derives the key from it (SHA-256 of "clacket:" + seed)
// and gets the blob map in place of its /*PACK*/null placeholder. The site's worker only
// serves /a/* to same-origin fetches from the page. Run by scripts/build-site.sh.
import { createCipheriv, createHash, randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const OUT = "site/dist";
const SOUNDS = "src-tauri/assets/sounds";
const IMAGES = ["light-sound", "light-keyboard", "dark-mouse"]; // from docs/images

const seed = randomBytes(32).toString("base64url");
const key = createHash("sha256").update(`clacket:${seed}`).digest();
mkdirSync(`${OUT}/a`, { recursive: true });

// writes iv || ciphertext || tag (the layout WebCrypto's AES-GCM decrypt expects after the iv)
function seal(bytes) {
  const iv = randomBytes(12);
  const c = createCipheriv("aes-256-gcm", key, iv);
  const body = Buffer.concat([iv, c.update(bytes), c.final(), c.getAuthTag()]);
  const path = `/a/${randomBytes(12).toString("hex")}.bin`;
  writeFileSync(OUT + path, body);
  return path;
}

// one blob per switch: its clips back to back, with [phase/name, byteLength] to split them
const sounds = {};
let clips = 0;
for (const sw of readdirSync(SOUNDS, { withFileTypes: true }).filter((d) => d.isDirectory() && d.name !== "fx").map((d) => d.name).sort()) {
  const list = [], parts = [];
  for (const phase of ["press", "release"]) {
    for (const f of readdirSync(join(SOUNDS, sw, phase)).filter((f) => f.endsWith(".mp3")).sort()) {
      const b = readFileSync(join(SOUNDS, sw, phase, f));
      list.push([`${phase}/${f.slice(0, -4)}`, b.length]);
      parts.push(b);
    }
  }
  sounds[sw] = { url: seal(Buffer.concat(parts)), clips: list };
  clips += list.length;
}
const images = Object.fromEntries(IMAGES.map((n) => [n, seal(readFileSync(`docs/images/${n}.png`))]));

const js = `${OUT}/demo.js`;
const src = readFileSync(js, "utf8");
if (!src.includes("/*PACK*/null")) throw new Error("demo.js lost its /*PACK*/null placeholder");
writeFileSync(js, src.replace("/*PACK*/null", JSON.stringify({ seed, sounds, images })));
console.log(`packed: ${clips} clips across ${Object.keys(sounds).length} switches, ${IMAGES.length} images`);

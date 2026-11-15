// License keys:      "CLACKET-"  + base32(payload_json ‖ ed25519_signature)  — proof of purchase.
// Activation tokens: "CLACKTOK-" + base32(payload_json ‖ ed25519_signature)  — binds a key to one device.
// Both are signed with LICENSE_SIGNING_KEY and verified offline by the app against its public key.
const A = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

export function base32(bytes) {
  let out = "", bits = 0, value = 0;
  for (const b of bytes) {
    value = (value << 8) | b;
    bits += 8;
    while (bits >= 5) {
      out += A[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) out += A[(value << (5 - bits)) & 31];
  return out;
}

const REV = Object.fromEntries([...A].map((c, i) => [c, i]));
function base32decode(str) {
  let bits = 0, value = 0;
  const out = [];
  for (const ch of str) {
    const v = REV[ch];
    if (v === undefined) return null;
    value = (value << 5) | v;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return new Uint8Array(out);
}

/** Splits "PREFIX" + base32(payload ‖ 64-byte sig) back into its parts. No signature check. */
function decodeEnvelope(text, prefix) {
  const clean = String(text).replace(/\s+/g, "").toUpperCase();
  if (!clean.startsWith(prefix)) return null;
  const raw = base32decode(clean.slice(prefix.length));
  if (!raw || raw.length <= 64) return null;
  const payloadBytes = raw.subarray(0, raw.length - 64);
  let payload;
  try {
    payload = JSON.parse(new TextDecoder().decode(payloadBytes));
  } catch {
    return null;
  }
  return { payload, payloadBytes, sig: raw.subarray(raw.length - 64) };
}

/** The payload of a license key ({ v, lid, name, plan, iat }), or null if it doesn't parse. */
export function parseKey(key) {
  return decodeEnvelope(key, "CLACKET-")?.payload ?? null;
}

/** The raw parts of an activation token, for the test to check its signature. */
export function decodeToken(token) {
  return decodeEnvelope(token, "CLACKTOK-");
}

/** Clean, uppercase form of a key, the way it is stored in D1. */
export function canonicalKey(key) {
  return String(key).replace(/\s+/g, "").toUpperCase();
}

/** jwk: the Ed25519 private key as JWK (the LICENSE_SIGNING_KEY secret, JSON string or object). */
export async function signLicense(jwk, { lid, name, iat = Math.floor(Date.now() / 1000) }) {
  const key = await crypto.subtle.importKey("jwk", typeof jwk === "string" ? JSON.parse(jwk) : jwk, { name: "Ed25519" }, false, ["sign"]);
  // field order matches the Rust struct; serde doesn't care, but stable bytes are easier to debug
  const payload = new TextEncoder().encode(JSON.stringify({ v: 1, lid, name, plan: "pro", iat }));
  const sig = new Uint8Array(await crypto.subtle.sign("Ed25519", key, payload));
  const raw = new Uint8Array(payload.length + sig.length);
  raw.set(payload);
  raw.set(sig, payload.length);
  return "CLACKET-" + base32(raw);
}

/** An activation token: binds a license (lid) to one device (device = the app's device_hash). */
export async function signActivation(jwk, { lid, device, iat = Math.floor(Date.now() / 1000) }) {
  const key = await crypto.subtle.importKey("jwk", typeof jwk === "string" ? JSON.parse(jwk) : jwk, { name: "Ed25519" }, false, ["sign"]);
  const payload = new TextEncoder().encode(JSON.stringify({ v: 1, typ: "act", lid, device, plan: "pro", iat }));
  const sig = new Uint8Array(await crypto.subtle.sign("Ed25519", key, payload));
  const raw = new Uint8Array(payload.length + sig.length);
  raw.set(payload);
  raw.set(sig, payload.length);
  return "CLACKTOK-" + base32(raw);
}

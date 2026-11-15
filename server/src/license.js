// License keys: "CLACKET-" + base32(payload_json ‖ ed25519_signature). Verified offline by the app.
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

import { env, SELF, applyD1Migrations } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";
import Stripe from "stripe";
import { issueForSession } from "../src/index.js";
import { decodeToken } from "../src/license.js";

const stripe = new Stripe("sk_test_x", { httpClient: Stripe.createFetchHttpClient() });
let pubKey; // the public half of LICENSE_SIGNING_KEY, to verify tokens the Worker signs

async function post(event, secret = "whsec_test") {
  const payload = JSON.stringify(event);
  const header = await stripe.webhooks.generateTestHeaderStringAsync({ payload, secret, cryptoProvider: Stripe.createSubtleCryptoProvider() });
  return SELF.fetch("https://x/stripe/webhook", { method: "POST", body: payload, headers: { "stripe-signature": header } });
}
const paid = (id, pi = "pi_" + id) => ({
  id: "evt_" + id, type: "checkout.session.completed",
  data: { object: { id: "cs_test_" + id, payment_status: "paid", payment_intent: pi, customer_details: { email: "jane@example.com", name: "Jane D." } } },
});

beforeAll(async () => {
  await applyD1Migrations(env.DB, env.TEST_MIGRATIONS);
  const kp = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  env.LICENSE_SIGNING_KEY = JSON.stringify(await crypto.subtle.exportKey("jwk", kp.privateKey));
  pubKey = kp.publicKey;
});

describe("webhook", () => {
  it("rejects a bad signature", async () => {
    const r = await post(paid("bad"), "whsec_wrong");
    expect(r.status).toBe(400);
  });

  it("issues one key per session, even when delivered twice", async () => {
    expect((await post(paid("once"))).status).toBe(200);
    expect((await post(paid("once"))).status).toBe(200);
    const { results } = await env.DB.prepare("SELECT key FROM licenses WHERE session_id = 'cs_test_once'").all();
    expect(results).toHaveLength(1);
    expect(results[0].key).toMatch(/^CLACKET-[A-Z2-7]+$/);
  });

  it("concurrent issue for the same session inserts once", async () => {
    const s = paid("race").data.object;
    const [a, b] = await Promise.all([issueForSession(env, s), issueForSession(env, s)]);
    expect(a.key).toBe(b.key);
  });

  it("marks a fully refunded payment revoked", async () => {
    await post(paid("refund", "pi_refund"));
    await post({ id: "evt_r", type: "charge.refunded", data: { object: { payment_intent: "pi_refund", refunded: true } } });
    const row = await env.DB.prepare("SELECT revoked FROM licenses WHERE payment_intent = 'pi_refund'").first();
    expect(row.revoked).toBe(1);
  });
});

describe("activation", () => {
  const A = "a".repeat(64), B = "b".repeat(64); // two 64-hex device hashes

  // Issues a real key through the webhook, then hands back the stored key string.
  async function keyFor(id) {
    await post(paid(id));
    const row = await env.DB.prepare("SELECT key FROM licenses WHERE session_id = ?").bind("cs_test_" + id).first();
    return row.key;
  }
  const activate = (key, device_hash, device_name) =>
    SELF.fetch("https://x/activate", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ key, device_hash, device_name }) });
  const deactivate = (key, device_hash) =>
    SELF.fetch("https://x/deactivate", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ key, device_hash }) });

  it("activates a device and returns a token that verifies for it", async () => {
    const key = await keyFor("act1");
    const r = await activate(key, A, "Jane's Mac");
    expect(r.status).toBe(200);
    const { token } = await r.json();
    expect(token).toMatch(/^CLACKTOK-[A-Z2-7]+$/);
    const t = decodeToken(token);
    expect(t.payload).toMatchObject({ typ: "act", device: A, plan: "pro" });
    expect(await crypto.subtle.verify("Ed25519", pubKey, t.sig, t.payloadBytes)).toBe(true);
  });

  it("re-activating the same device succeeds (idempotent, not 409)", async () => {
    const key = await keyFor("act2");
    expect((await activate(key, A)).status).toBe(200);
    const r = await activate(key, A);
    expect(r.status).toBe(200);
    expect((await r.json()).token).toMatch(/^CLACKTOK-/);
  });

  it("a second, different device is refused with 409 in_use", async () => {
    const key = await keyFor("act3");
    expect((await activate(key, A)).status).toBe(200);
    const r = await activate(key, B);
    expect(r.status).toBe(409);
    expect((await r.json()).error).toBe("in_use");
  });

  it("deactivating frees the seat for another device", async () => {
    const key = await keyFor("act4");
    expect((await activate(key, A)).status).toBe(200);
    expect((await activate(key, B)).status).toBe(409);
    const d = await deactivate(key, A);
    expect(d.status).toBe(200);
    expect((await d.json()).ok).toBe(true);
    expect((await activate(key, B)).status).toBe(200); // the seat is free now
  });

  it("deactivate is a no-op for a device that doesn't hold the seat", async () => {
    const key = await keyFor("act5");
    expect((await activate(key, A)).status).toBe(200);
    expect((await deactivate(key, B)).status).toBe(200); // B never held it
    expect((await activate(key, A)).status).toBe(200); // A still holds it, idempotent
  });

  it("a revoked key can't activate, and a refund frees its seat", async () => {
    const key = await keyFor("act6"); // payment_intent pi_act6
    expect((await activate(key, A)).status).toBe(200);
    await post({ id: "evt_a6", type: "charge.refunded", data: { object: { payment_intent: "pi_act6", refunded: true } } });
    const seat = await env.DB.prepare("SELECT 1 FROM activations WHERE lid IN (SELECT lid FROM licenses WHERE payment_intent = 'pi_act6')").first();
    expect(seat).toBeNull(); // seat freed on refund
    const r = await activate(key, A);
    expect(r.status).toBe(403);
    expect((await r.json()).error).toBe("revoked");
  });

  it("rejects a garbage key and a malformed device hash", async () => {
    expect((await activate("CLACKET-NOPE", A)).status).toBe(403);
    expect((await activate("not-a-key", A)).status).toBe(403);
    const key = await keyFor("act7");
    expect((await activate(key, "short")).status).toBe(400);
  });
});

describe("pages", () => {
  it("thanks shows the key, deep link, and auto-downloads the license", async () => {
    await post(paid("thanks"));
    const html = await (await SELF.fetch("https://x/thanks?session_id=cs_test_thanks")).text();
    expect(html).toContain("clacket://activate?key=CLACKET-");
    expect(html).toContain('download="clacket.license"');
    expect(html).toContain("getElementById('dl').click()"); // auto-download on load
    expect(html).toContain("localStorage.setItem('clacket_key'"); // persisted for reload
  });

  it("thanks reloads itself while a paid session is still settling (paid-but-no-key race)", async () => {
    // session not in D1 and Stripe retrieve fails (test key) -> processing, 202, self-reload
    const r = await SELF.fetch("https://x/thanks?session_id=cs_test_notyet_12345");
    expect(r.status).toBe(202);
    const html = await r.text();
    expect(html).toContain("Payment processing");
    expect(html).toContain("location.replace"); // bounded auto-retry
  });

  it("thanks stops reloading after the attempt cap", async () => {
    const r = await SELF.fetch("https://x/thanks?session_id=cs_test_notyet_12345&t=12");
    expect(r.status).toBe(202);
    const html = await r.text();
    expect(html).not.toContain("location.replace");
    expect(html).toContain("Reload");
  });

  it("an invalid session id is rejected", async () => {
    expect((await SELF.fetch("https://x/thanks?session_id=nope")).status).toBe(404);
    expect((await SELF.fetch("https://x/thanks")).status).toBe(404);
  });

  it("recover is a help page, not an email resend", async () => {
    const r = await SELF.fetch("https://x/recover");
    const html = await r.text();
    expect(r.status).toBe(200);
    expect(html).toContain("clacket.license");
    expect(html).not.toContain("Send my key");
  });
});

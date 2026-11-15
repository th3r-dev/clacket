import { env, SELF, applyD1Migrations } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";
import Stripe from "stripe";
import { issueForSession } from "../src/index.js";

const stripe = new Stripe("sk_test_x", { httpClient: Stripe.createFetchHttpClient() });
const signingKey = async () => {
  const k = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  return crypto.subtle.exportKey("jwk", k.privateKey);
};

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
  env.LICENSE_SIGNING_KEY = JSON.stringify(await signingKey());
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

describe("pages", () => {
  it("thanks shows the key and deep link", async () => {
    await post(paid("thanks"));
    const html = await (await SELF.fetch("https://x/thanks?session_id=cs_test_thanks")).text();
    expect(html).toContain("clacket://activate?key=CLACKET-");
    expect(html).toContain('download="clacket.license"');
  });

  it("recover is a help page, not an email resend", async () => {
    const r = await SELF.fetch("https://x/recover");
    const html = await r.text();
    expect(r.status).toBe(200);
    expect(html).toContain("clacket.license");
    expect(html).not.toContain("Send my key");
  });
});

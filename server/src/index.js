// Sells Clacket Pro. Only the buyer's browser and Stripe talk to this Worker; the app never does.
import Stripe from "stripe";
import { signLicense } from "./license.js";

const stripeFor = (env) => new Stripe(env.STRIPE_SECRET_KEY, { httpClient: Stripe.createFetchHttpClient() });

const esc = (s) => String(s).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
const page = (title, body, status = 200) =>
  new Response(
    `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(title)}</title><style>
:root{--ink:#16171A;--paper:#F4F1EA;--accent:#E8622C;--muted:#5E5A52}
@media (prefers-color-scheme:dark){:root{--ink:#F4F1EA;--paper:#1E1F23;--muted:#B5B0A5}}
body{margin:0;background:var(--paper);color:var(--ink);font:16px/1.5 system-ui,sans-serif}
main{max-width:560px;margin:0 auto;padding:48px 16px}h1{font-size:28px;margin:0 0 12px}
code{display:block;word-break:break-all;background:color-mix(in srgb,var(--ink) 8%,transparent);padding:12px;border-radius:8px;font-size:13px}
.row{display:flex;gap:8px;flex-wrap:wrap;margin-top:16px}
a.b,button{background:var(--accent);color:#16171A;border:0;border-radius:8px;padding:10px 16px;font:600 15px system-ui;text-decoration:none;cursor:pointer}
a.s{background:transparent;color:var(--ink);border:1px solid var(--muted)}input{font:inherit;padding:10px;border-radius:8px;border:1px solid var(--muted);min-width:240px}
p.m{color:var(--muted)}</style><main>${body}</main></html>`,
    { status, headers: { "content-type": "text/html; charset=utf-8", "x-frame-options": "DENY", "referrer-policy": "no-referrer" } },
  );

/** Issues (or returns the existing) key for a paid Checkout Session. Idempotent per session. */
export async function issueForSession(env, session) {
  const found = await env.DB.prepare("SELECT key, email FROM licenses WHERE session_id = ?").bind(session.id).first();
  if (found) return { ...found, created: false };
  const email = session.customer_details?.email;
  if (!email) throw new Error("session has no email");
  const name = session.customer_details?.name || email;
  const lid = "lic_" + crypto.randomUUID().replaceAll("-", "").slice(0, 20);
  const key = await signLicense(env.LICENSE_SIGNING_KEY, { lid, name });
  // UNIQUE(session_id) makes a webhook/thanks-page race insert once; the loser reads the winner's row
  const r = await env.DB.prepare(
    "INSERT OR IGNORE INTO licenses (lid, key, email, name, session_id, payment_intent, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
  ).bind(lid, key, email, name, session.id, session.payment_intent ?? null, Date.now()).run();
  if (!r.meta.changes) return { ...(await env.DB.prepare("SELECT key, email FROM licenses WHERE session_id = ?").bind(session.id).first()), created: false };
  return { key, email, created: true };
}

export async function sendKey(env, email, keys) {
  if (!env.EMAIL) return;
  const list = keys.join("\n\n");
  await env.EMAIL.send({
    to: email,
    from: { email: env.FROM_EMAIL, name: env.APP_NAME },
    subject: keys.length > 1 ? "Your Clacket Pro keys" : "Your Clacket Pro key",
    text: `Thanks for buying Clacket Pro.\n\nYour key:\n\n${list}\n\nOpen Clacket, choose Settings > Upgrade, and paste the key, or open the attached clacket.license file.\nClacket checks the key on your computer and never connects to the internet.\n`,
    html: `<p>Thanks for buying Clacket Pro.</p><p>Your key:</p>${keys.map((k) => `<pre style="white-space:pre-wrap;word-break:break-all">${esc(k)}</pre>`).join("")}<p>Open Clacket, choose Settings &gt; Upgrade, and paste the key, or open the attached <b>clacket.license</b> file. Clacket checks the key on your computer and never connects to the internet.</p>`,
    attachments: [{ content: list + "\n", filename: "clacket.license", type: "text/plain", disposition: "attachment" }],
  });
}

async function webhook(req, env) {
  const body = await req.text();
  let event;
  try {
    event = await stripeFor(env).webhooks.constructEventAsync(
      body, req.headers.get("stripe-signature") ?? "", env.STRIPE_WEBHOOK_SECRET, undefined, Stripe.createSubtleCryptoProvider(),
    );
  } catch {
    return new Response("bad signature", { status: 400 });
  }
  if (event.type === "checkout.session.completed" && event.data.object.payment_status === "paid") {
    const { key, email, created } = await issueForSession(env, event.data.object);
    if (created) await sendKey(env, email, [key]);
  } else if (event.type === "charge.refunded" && event.data.object.refunded) {
    // full refunds only; the lid goes into the app's revoked.txt at the next release
    await env.DB.prepare("UPDATE licenses SET revoked = 1 WHERE payment_intent = ?").bind(event.data.object.payment_intent).run();
  }
  return Response.json({ received: true });
}

async function thanks(url, env) {
  const id = url.searchParams.get("session_id") ?? "";
  if (!/^cs_[A-Za-z0-9_]{8,200}$/.test(id)) return page("Not found", "<h1>That link isn't valid</h1>", 404);
  let row = await env.DB.prepare("SELECT key FROM licenses WHERE session_id = ?").bind(id).first();
  if (!row) {
    // the webhook can arrive after the redirect: confirm with Stripe and issue it here
    const s = await stripeFor(env).checkout.sessions.retrieve(id).catch(() => null);
    if (!s || s.payment_status !== "paid") return page("Payment pending", "<h1>Payment still processing</h1><p>Reload this page in a minute. Your key will also arrive by email.</p>", 202);
    const r = await issueForSession(env, s);
    if (r.created) await sendKey(env, r.email, [r.key]);
    row = r;
  }
  const deep = `clacket://activate?key=${encodeURIComponent(row.key)}`;
  const file = `data:text/plain;charset=utf-8,${encodeURIComponent(row.key + "\n")}`;
  return page("Your Clacket Pro key", `<h1>You have Clacket Pro</h1>
<p>Your key is below. A copy is on its way to your email.</p><code id="k">${esc(row.key)}</code>
<div class="row"><a class="b" href="${esc(deep)}">Open in Clacket</a>
<a class="b s" href="${file}" download="clacket.license">Download clacket.license</a></div>
<p class="m">Clacket checks the key on your computer. It never connects to the internet.</p>`);
}

async function recover(req, env) {
  if (req.method === "GET") {
    return page("Find your key", `<h1>Find your Clacket Pro key</h1><p>Enter the email you paid with. Your keys will be sent there.</p>
<form method="post" class="row"><label for="e" class="m" style="width:100%">Email</label><input id="e" name="email" type="email" required autocomplete="email"><button>Send my key</button></form>`);
  }
  const ip = req.headers.get("cf-connecting-ip") ?? "local";
  if (env.RECOVER_LIMIT && !(await env.RECOVER_LIMIT.limit({ key: ip })).success) {
    return page("Slow down", "<h1>Too many tries</h1><p>Wait a minute, then try again.</p>", 429);
  }
  const email = String((await req.formData()).get("email") ?? "").trim().toLowerCase().slice(0, 254);
  if (email.includes("@")) {
    const { results } = await env.DB.prepare("SELECT key FROM licenses WHERE lower(email) = ? AND revoked = 0").bind(email).all();
    if (results.length) await sendKey(env, email, results.map((r) => r.key));
  }
  // same answer either way, so this can't be used to learn who bought
  return page("Check your email", "<h1>Check your email</h1><p>If that address bought Clacket Pro, the key is on its way.</p>");
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (url.pathname === "/stripe/webhook" && req.method === "POST") return webhook(req, env);
    if (url.pathname === "/thanks" && req.method === "GET") return thanks(url, env);
    if (url.pathname === "/recover" && (req.method === "GET" || req.method === "POST")) return recover(req, env);
    return page("Not found", "<h1>Not found</h1>", 404);
  },
};

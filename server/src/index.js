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
    // record the license so it's in D1 even if the buyer never loads the thanks page
    await issueForSession(env, event.data.object);
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
    if (!s || s.payment_status !== "paid") {
      // keep checking: a paid session usually settles within seconds of the redirect, so the
      // page reloads itself (bounded) rather than leaving the buyer to reload by hand
      const tries = Math.min(parseInt(url.searchParams.get("t") || "0", 10) || 0, 12);
      const retry = tries < 12
        ? `<p class="m">This usually takes a few seconds. Checking again… (${tries + 1})</p>
<script>setTimeout(function(){location.replace(location.pathname+'?session_id='+encodeURIComponent(${JSON.stringify(id)})+'&t='+${tries + 1});},5000);</script>`
        : `<p class="m">Still not confirmed. If you were charged, reload this page, or keep this link — your key appears here once payment settles.</p>
<div class="row"><a class="b" href="?session_id=${esc(id)}">Reload</a></div>`;
      return page("Payment processing", `<h1>Payment processing…</h1>${retry}`, 202);
    }
    row = await issueForSession(env, s);
  }
  const deep = `clacket://activate?key=${encodeURIComponent(row.key)}`;
  const file = `data:text/plain;charset=utf-8,${encodeURIComponent(row.key + "\n")}`;
  const jsKey = JSON.stringify(row.key); // key is [A-Z2-7-] only, safe to inline
  return page("Your Clacket Pro key", `<h1>You have Clacket Pro</h1>
<p>Here is your key. The <b>clacket.license</b> file downloads automatically — opening it activates Clacket Pro. Keep it safe; this page is the only place the key is shown.</p>
<code id="k">${esc(row.key)}</code>
<div class="row"><a class="b" id="open" href="${esc(deep)}">Open in Clacket</a>
<a class="b s" id="dl" href="${file}" download="clacket.license">Download clacket.license</a>
<button class="s" id="copy" type="button">Copy key</button></div>
<p class="m">Clacket checks the key on your computer. It never connects to the internet.</p>
<script>
(function(){
  try{ localStorage.setItem('clacket_key', ${jsKey}); }catch(e){}
  try{ document.getElementById('dl').click(); }catch(e){}           // auto-download the license file
  var c=document.getElementById('copy');
  if(c) c.addEventListener('click', function(){
    try{ navigator.clipboard.writeText(${jsKey}); c.textContent='Copied'; }catch(e){}
  });
})();
</script>`);
}

/** No email is sent (the key is delivered on the thanks page), so this is a help page,
 *  not an automated resend. SUPPORT_EMAIL is optional. */
function recover(env) {
  const contact = env.SUPPORT_EMAIL
    ? `<p>Still lost it? Email <a href="mailto:${esc(env.SUPPORT_EMAIL)}">${esc(env.SUPPORT_EMAIL)}</a> from the address you paid with and we'll send it again.</p>`
    : `<p>Still lost it? Reply to your Stripe payment receipt with the email you paid with and we'll send it again.</p>`;
  return page("Find your key", `<h1>Find your Clacket Pro key</h1>
<p>Your key is shown right after you pay, and in the <b>clacket.license</b> file you downloaded then. Keep that file safe — opening it activates Clacket Pro.</p>
${contact}`);
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (url.pathname === "/stripe/webhook" && req.method === "POST") return webhook(req, env);
    if (url.pathname === "/thanks" && req.method === "GET") return thanks(url, env);
    if (url.pathname === "/recover" && req.method === "GET") return recover(env);
    return page("Not found", "<h1>Not found</h1>", 404);
  },
};

// Runs only for /a/* (run_worker_first in wrangler.jsonc); every other path is served
// straight from ./dist. The encrypted sound and screenshot blobs go out only to fetch()
// calls from this site's own page, so opening a blob URL in a tab, hotlinking it or
// curling it gets a 404. This stops casual downloads, not someone recording the page.
const notFound = () => new Response("Not found", { status: 404, headers: { "Cache-Control": "no-store" } });

export function fromPage(request) {
  const h = request.headers;
  const site = h.get("Sec-Fetch-Site");
  if (site !== null) {
    return site === "same-origin" && h.get("Sec-Fetch-Dest") === "empty" && ["cors", "same-origin"].includes(h.get("Sec-Fetch-Mode"));
  }
  // browsers without Fetch Metadata: the page's own Referer (sent in full same-origin)
  try {
    return new URL(h.get("Referer")).origin === new URL(request.url).origin;
  } catch {
    return false;
  }
}

export default {
  async fetch(request, env) {
    if (request.method !== "GET" || !fromPage(request)) return notFound();
    const res = await env.ASSETS.fetch(request);
    if (!res.ok) return notFound();
    const out = new Response(res.body, res);
    out.headers.set("Content-Type", "application/octet-stream");
    out.headers.set("Content-Disposition", "inline");
    out.headers.set("Cache-Control", "private, max-age=604800, immutable");
    // a cached copy must not answer a navigation to the same URL
    out.headers.set("Vary", "Sec-Fetch-Site, Sec-Fetch-Mode, Sec-Fetch-Dest, Referer");
    return out;
  },
};

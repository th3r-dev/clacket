# Payment flow — test evidence

Run against the **live license Worker** at `pay.th33raphat.dev` wired to a Stripe **sandbox**
(test money only). No real charges. All test rows were deleted from D1 afterwards.

## Automated unit tests (CI, `server/test/index.test.js`, 9 passing)
`npm test` runs these in the Workers runtime on every PR:

| Test | Asserts |
|---|---|
| rejects a bad signature | unsigned/forged webhook → 400 |
| issues one key per session, even when delivered twice | duplicate `checkout.session.completed` → **one** D1 row |
| concurrent issue for the same session inserts once | race on `issueForSession` → same key |
| marks a fully refunded payment revoked | `charge.refunded` → `revoked = 1` |
| thanks shows the key, deep link, and auto-downloads the license | page has the key, `clacket://`, `download="clacket.license"`, auto-click + `localStorage` |
| thanks reloads itself while a paid session is still settling | unsettled session → 202 + bounded self-reload |
| thanks stops reloading after the attempt cap | `t=12` → shows Reload, no further auto-retry |
| an invalid session id is rejected | `?session_id=nope` and empty → 404 |

## End-to-end against the live Worker + Stripe sandbox

| # | Case | Expected | Result |
|---|---|---|---|
| 1 | **Happy path** — pay with test card `4242` on the Payment Link | redirect to `/thanks`, key shown, `clacket.license` **auto-downloads**, D1 row created | ✅ key shown; browser auto-downloaded `clacket.license`; D1 row present (`evidence@example.com`) — see `thanks-page.png` |
| 2 | **Key works offline** — verify the issued key against the app's embedded public key (`src-tauri/license-public.key`) | signature valid → Pro unlocks with no network | ✅ `offline verify … true` |
| 3 | **Paid but thanks page not loaded** — webhook only | license still recorded in D1 for later lookup | ✅ webhook wrote the row independently of the page |
| 4 | **Paid but key not ready yet** (webhook/redirect race) | `/thanks` re-issues from the Stripe session, or shows a self-refreshing "processing" page | ✅ 202 "Payment processing…", bounded auto-reload — see `processing-page.png` |
| 5 | **Processing never settles** | stop auto-reloading after the cap, offer manual Reload | ✅ `t=12` → "Reload", no auto-retry |
| 6 | **Unpaid / abandoned session** opened at `/thanks` | 202 processing, never reveals a key | ✅ 202, no key |
| 7 | **Invalid `session_id`** | 404 | ✅ `nope` → 404, empty → 404 |
| 8 | **Forged webhook** (bad signature) | 400, nothing written | ✅ 400 |
| 9 | **Duplicate webhook** (same session twice) | one key, one row | ✅ one row (unit + live) |
| 10 | **Refund** — refund the test payment | `charge.refunded` webhook → `revoked = 1` in D1 | ✅ `revoked = 1` |

### Known limits (stated, not defects)
- With **no email**, a buyer who loses both the thanks page and the downloaded `clacket.license` recovers only by the seller looking them up in D1 by their payment. The auto-download and `localStorage` copy make this unlikely.
- A **refund** sets `revoked = 1` in D1, but an **already-installed** app only learns a key is revoked when a future release embeds that `lid` in `revoked.txt` (the app never calls the network). New activations of a revoked key fail immediately.

## Screenshots
- `thanks-page.png` — the success page: key, Open in Clacket / Download / Copy, auto-download note.
- `processing-page.png` — the self-refreshing "Payment processing…" state (case 4).

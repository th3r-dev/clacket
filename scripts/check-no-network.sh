#!/usr/bin/env bash
# Clacket now talks to the network for ONE thing only: license activation, from Rust, to one host.
# This guard pins that down. It fails if:
#   (a) any HTTP/WebSocket client crate other than the one approved client (ureq) is in the tree,
#   (b) a src/ file contains an http(s):// literal for any host but the approved ones
#       (pay.th33raphat.dev for activate/deactivate/recover; buy.stripe.com and clacket.th33raphat.dev
#        are opened in the user's browser — Clacket never fetches them),
#   (c) a telemetry/analytics SDK appears in the dependencies, or socket code / networking Tauri
#       plugins show up. The settings webview still can't reach the network (CSP, unchanged).
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
bad=0

# (a) exactly one HTTP/WS client is allowed: ureq. Anything else (incl. a second client) fails.
for t in aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-msvc x86_64-unknown-linux-gnu; do
  extra=$(cargo tree -e normal --target "$t" --prefix none 2>/dev/null \
    | grep -E '^(reqwest|hyper|isahc|attohttpc|curl|tungstenite|tokio-tungstenite|surf|ureq) ' \
    | awk '{print $1}' | sort -u | grep -vx ureq || true)
  if [[ -n "$extra" ]]; then echo "unapproved HTTP/WS client for $t:"; echo "$extra"; bad=1; fi
done

# (b) the only URL hosts the app's own source may name
ALLOWED_HOSTS="pay.th33raphat.dev buy.stripe.com clacket.th33raphat.dev"
allowed() { case " $ALLOWED_HOSTS " in *" $1 "*) return 0;; *) return 1;; esac; }
while IFS= read -r host; do
  [[ -z "$host" ]] && continue
  if ! allowed "$host"; then echo "unapproved URL host in src/: $host"; bad=1; fi
done < <(grep -rhoE 'https?://[A-Za-z0-9.-]+' src | sed -E 's#^https?://##' | sort -u)

# (c) no telemetry/analytics SDKs anywhere in the tree
tel=$(cargo tree -e normal --prefix none 2>/dev/null \
  | grep -iE '^(sentry|segment|posthog|mixpanel|amplitude|datadog|rudder|heap|bugsnag|opentelemetry|telemetry|analytics)[a-z0-9_-]* ' \
  | awk '{print $1}' | sort -u || true)
if [[ -n "$tel" ]]; then echo "telemetry/analytics crate in tree:"; echo "$tel"; bad=1; fi

# no raw sockets in Clacket's own code, no networking Tauri plugins
if grep -rnE 'TcpStream|UdpSocket|TcpListener|std::net::' src; then echo "socket API in src/"; bad=1; fi
for p in http websocket updater upload; do
  if grep -q "tauri-plugin-$p" Cargo.toml; then echo "tauri-plugin-$p is not allowed"; bad=1; fi
done

if [[ "$bad" -eq 0 ]]; then echo "network check passed: one client (ureq), one host (pay.th33raphat.dev)"; fi
exit "$bad"

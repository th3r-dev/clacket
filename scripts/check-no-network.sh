#!/usr/bin/env bash
# Fails if the desktop app could open a network connection: no HTTP/WebSocket client crates
# in the dependency tree for any desktop target, and no socket APIs in Clacket's own code.
# (Tauri pulls reqwest only for Android/iOS, which Clacket doesn't ship.)
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
bad=0
for t in aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-msvc x86_64-unknown-linux-gnu; do
  hits=$(cargo tree -e normal --target "$t" --prefix none 2>/dev/null \
    | grep -E '^(reqwest|hyper|ureq|isahc|attohttpc|curl|tungstenite|tokio-tungstenite|surf) ' | sort -u || true)
  if [ -n "$hits" ]; then echo "network crate for $t:"; echo "$hits"; bad=1; fi
done
if grep -rnE 'TcpStream|UdpSocket|TcpListener|std::net::' src; then echo "socket API in src/"; bad=1; fi
for p in http websocket updater upload; do
  if grep -q "tauri-plugin-$p" Cargo.toml; then echo "tauri-plugin-$p is not allowed"; bad=1; fi
done
[ $bad -eq 0 ] && echo "no-network check passed"
exit $bad

#!/usr/bin/env bash
# Assembles site/dist for clacket.th33raphat.dev from site/src plus the app's own assets:
# switch recordings (for the browser demo), fonts, screenshots, and the latest release's
# installers (the repo is private, so the site serves the downloads). Deploy with:
#   scripts/build-site.sh && (cd site && npx wrangler deploy)
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=site/dist
rm -rf "$OUT" && mkdir -p "$OUT"/{sounds,fonts,img,download}
cp -R site/src/. "$OUT"/

# demo sounds: switch folders only (no fx), plus an index the demo reads
python3 - <<'EOF'
import json, pathlib, shutil
src, out = pathlib.Path("src-tauri/assets/sounds"), pathlib.Path("site/dist/sounds")
index = {}
for sw in sorted(p for p in src.iterdir() if p.is_dir() and p.name != "fx"):
    files = sorted(f for f in sw.rglob("*.mp3"))
    index[sw.name] = [str(f.relative_to(sw).with_suffix("")) for f in files]
    for f in files:
        dst = out / sw.name / f.relative_to(sw)
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(f, dst)
(out / "index.json").write_text(json.dumps(index, separators=(",", ":")))
print(f"sounds: {sum(map(len, index.values()))} clips across {len(index)} switches")
EOF

cp ui/fonts/{mali-latin-400,mali-latin-600,kanit-latin-400,kanit-latin-500}-normal.woff2 "$OUT"/fonts/
cp docs/images/{light-sound,light-keyboard,dark-mouse,social-preview}.png "$OUT"/img/
cp src-tauri/icons/128x128.png "$OUT"/img/icon-128.png

cat > "$OUT"/404.html <<'EOF'
<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Not found · Clacket</title><link rel="stylesheet" href="/style.css">
<section class="hero"><h1>No <em>clack</em> here.</h1><p class="lede">That page doesn't exist.</p><a class="btn" href="/">Back to Clacket</a></section>
EOF

# installers from the latest GitHub release, under stable names the /download/* redirects use
LIMIT=$((25 * 1024 * 1024)) # Workers static assets: 25 MiB per file
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
if tag=$(gh release view --json tagName --jq .tagName 2>/dev/null); then
  gh release download "$tag" --dir "$tmp" --pattern '*.dmg' --pattern '*.msi' --pattern '*.AppImage' --pattern '*.deb' 2>/dev/null || true
  put() { # put <glob> <name>
    local f; f=$(ls "$tmp"/$1 2>/dev/null | head -1) || true
    if [[ -z "$f" ]]; then echo "download: no $1 in $tag"; return; fi
    if (( $(stat -f%z "$f" 2>/dev/null || stat -c%s "$f") > LIMIT )); then echo "download: $(basename "$f") is over 25 MiB, skipped"; return; fi
    cp "$f" "$OUT/download/$2"
    echo "download: $2 <- $(basename "$f")"
  }
  put '*universal.dmg' Clacket-macos-universal.dmg
  put '*.msi' Clacket-windows-x64.msi
  put '*.AppImage' Clacket-linux-amd64.AppImage
  put '*.deb' Clacket-linux-amd64.deb
  sed -i.bak "s|<span id=\"version\">[^<]*</span>|<span id=\"version\">${tag#v}</span>|" "$OUT"/index.html && rm "$OUT"/index.html.bak
else
  echo "download: no release found; download links will 404 until one exists"
fi
du -sh "$OUT"

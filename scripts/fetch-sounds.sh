#!/usr/bin/env bash
# Rebuilds src-tauri/assets/sounds from upstream. The output is committed;
# run this only to refresh it. Needs curl, unzip, ffmpeg, python3.
#   kbsim (MIT)        https://github.com/tplai/kbsim     -> keyboard switches
#   Kenney (CC0)       https://kenney.nl/assets/{interface-sounds,ui-audio} -> mouse, gestures, plugs
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=src-tauri/assets/sounds
KBSIM_SHA=ba103f3b0afa9dab80447aa2e7e2ed80b6bd80e4
SWITCHES="alpaca blackink bluealps boxnavy buckling cream holypanda mxblack mxblue mxbrown redink topre turquoise"
PRESS="GENERIC_R0 GENERIC_R1 GENERIC_R2 GENERIC_R3 GENERIC_R4 SPACE ENTER BACKSPACE"
RELEASE="GENERIC SPACE ENTER BACKSPACE"
UA="Mozilla/5.0"

mkdir -p "$OUT"
base="https://raw.githubusercontent.com/tplai/kbsim/$KBSIM_SHA/src/assets/audio"
for s in $SWITCHES; do
  mkdir -p "$OUT/$s/press" "$OUT/$s/release"
  for c in $PRESS;   do curl -fsSL "$base/$s/press/$c.mp3"   -o "$OUT/$s/press/$c.mp3" || rm -f "$OUT/$s/press/$c.mp3"; done  # some switches only have GENERIC
  for c in $RELEASE; do curl -fsSL "$base/$s/release/$c.mp3" -o "$OUT/$s/release/$c.mp3" || rm -f "$OUT/$s/release/$c.mp3"; done
done
curl -fsSL "https://raw.githubusercontent.com/tplai/kbsim/$KBSIM_SHA/LICENSE.md" -o "$OUT/LICENSE-kbsim.md"

# Kenney hands out a fresh signed link per download.
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
for a in interface-sounds ui-audio; do
  id=$(printf "assets/%s" "$a" | xxd -p | tr -d '\n')
  curl -fsSL -c "$tmp/cj" -b "$tmp/cj" -A "$UA" "https://kenney.nl/assets/$a" -o /dev/null
  url=$(curl -fsSL -c "$tmp/cj" -b "$tmp/cj" -A "$UA" -H "Referer: https://kenney.nl/assets/$a" \
        "https://kenney.nl/download-link/$id" | python3 -c 'import json,sys;print(json.load(sys.stdin)["url"])')
  curl -fsSL -A "$UA" "$url" -o "$tmp/$a.zip"
  unzip -q -o "$tmp/$a.zip" -d "$tmp/$a"
done

# target name : kenney clip
mkdir -p "$OUT/fx"
while read -r name src; do
  f=$(find "$tmp" -name "$src.ogg" | head -1)
  [ -z "$f" ] && { echo "fx: missing $src for $name"; continue; }
  ffmpeg -v error -y -i "$f" -ac 1 -ar 44100 -c:a libmp3lame -q:a 4 "$OUT/fx/$name.mp3"
done <<'EOF'
mouse_down click_004
mouse_up mouserelease1
scroll tick_002
tap click_002
tap2 click_003
swipe scratch_002
pinch pluck_002
usb_storage_in confirmation_001
usb_storage_out drop_003
phone_in glass_006
phone_out glass_003
input_in switch_002
input_out switch_005
audio_in glass_001
audio_out glass_002
usb_other_in select_002
usb_other_out drop_001
charger_in maximize_003
charger_out minimize_003
headphones_in pluck_001
headphones_out pluck_002
display_in open_002
display_out close_002
sd_card_in tick_001
sd_card_out tick_004
EOF

# Movement loop: 2 s of low-passed brown noise, fades so the loop seam is soft.
ffmpeg -v error -y -f lavfi -i "anoisesrc=color=brown:amplitude=0.5:duration=2:sample_rate=44100" \
  -af "lowpass=f=900,afade=t=in:d=0.05,afade=t=out:st=1.95:d=0.05" -ac 1 -c:a libmp3lame -q:a 6 "$OUT/fx/move.mp3"

echo "sounds written to $OUT"

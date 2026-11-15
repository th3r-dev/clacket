<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/banner-dark.png">
    <img src="docs/images/banner-light.png" alt="Clacket: every key, a little clack." width="840">
  </picture>
</p>

<p align="center">
  Mechanical keyboard sounds for any keyboard, on macOS, Windows and Linux.<br>
  Clacket sits in your menu bar and plays a real switch recording on every key press. It connects only to activate or move your license, one device at a time, and never records or sends what you type.<br>
  <a href="https://clacket.th33raphat.dev"><b>clacket.th33raphat.dev</b></a>: try every switch in your browser.
</p>

<p align="center">
  <a href="../../actions/workflows/ci.yml"><img alt="CI" src="../../actions/workflows/ci.yml/badge.svg"></a>
</p>

<p align="center">
  <img src="docs/clacket-demo.gif" alt="Clacket: typing on a laptop keyboard that plays real mechanical switch sounds" width="820">
  <br>
  <em>▶ <a href="https://github.com/th3r-dev/clacket/raw/main/docs/clacket-promo.mp4">Watch the full demo with sound (40s)</a> — or <a href="https://clacket.th33raphat.dev">try every switch in your browser</a>.</em>
</p>

<p align="center">
  <img src="docs/images/light-keyboard.png" alt="Clacket's keyboard tab: an ANSI keyboard drawn on screen, with the space bar selected and set to Holy Panda's space bar sound." width="760">
</p>

## What it does

- **Thirteen recorded switches.** Linear (Alpaca, Black Ink, Red Ink, NK Cream, MX Black), tactile (Holy Panda, MX Brown, Turquoise, Topre) and clicky (MX Blue, Box Navy, Blue Alps, Buckling Spring). Space, Enter and Backspace get their own recordings where the switch has them, plus a release sound when you lift the key.
- **A sound for any key.** Click a key on the on-screen keyboard, or press it, and give it any switch's sound or your own MP3 or WAV. Save the whole mapping as a preset and switch presets from the menu bar.
- **Your keyboard, as you see it.** The drawing follows your keyboard's shape (ANSI, ISO or JIS). On macOS the legends come from your input language, so a Thai or AZERTY layout shows its own letters.
- **Mouse, trackpad and plugs.** Clicks, scroll ticks, a soft sound while the pointer moves, taps, swipes and pinches. Sounds when you plug in or unplug a USB drive, phone, keyboard, charger, display, headphones or SD card, with the port (USB-C, USB-A, Thunderbolt) when the system reports it. Force Touch trackpads can give a haptic tick too.
- **Light on your Mac.** The everyday controls live in the menu bar. The settings window only exists while it's open. An idle release build uses about 23 MB of memory and no CPU, and the program itself is about 5 MB with every sound and font built in.

<p align="center">
  <img src="docs/images/light-sound.png" alt="Sound and options tab: switch cards grouped as linear, tactile and clicky, each with a drawing of the switch in its stem color." width="49%">
  <img src="docs/images/dark-mouse.png" alt="Mouse and devices tab in dark mode: rows for click, scroll, movement and gesture sounds, and the last plugged device." width="49%">
</p>

## Free and Pro

Clacket is free to use. Pro costs **$5 once** and is yours for good.

| | Free | Pro |
|---|:---:|:---:|
| NK Cream, MX Brown, MX Blue, Topre | ✓ | ✓ |
| All 13 switches | | ✓ |
| Volume, key-up sound, mute shortcut (⌥⌘K / Ctrl+Alt+K), open at login | ✓ | ✓ |
| A sound for any key, your own sound files, presets | | ✓ |
| Mouse, trackpad and gesture sounds, haptic tick | | ✓ |
| Plug-in sounds | | ✓ |

You pay in your browser and your key appears right after. Paste it into Clacket, open the `clacket.license` file, or click "Open in Clacket" on the receipt page. Clacket activates the key once against the license server, which ties it to this one device; after that it runs offline. To use the key on another computer, deactivate it first.

<p align="center">
  <img src="docs/images/dark-upgrade.png" alt="The Clacket Pro sheet: price, four things Pro adds, an Upgrade button and a box for pasting a key." width="560">
</p>

## Privacy

Clacket uses the internet for one thing: turning Pro on, or moving it to another computer. When you activate, it sends your license key and a salted device fingerprint (a one-way hash of a machine id, not anything you can read back) to `pay.th33raphat.dev`. The server binds the key to that one device and returns a signed token; from then on Clacket runs offline, because the token alone proves Pro on this device. Switching computers means deactivating first, which frees the key. Clacket never records or stores what you type, and it sends nothing else. No account, no analytics, no update check.

Most of this is enforced in the build. CI (`scripts/check-no-network.sh`) fails if any HTTP client besides the one used for activation appears, if the app's source names a host other than the license server (and the buy and website links it opens in your browser), or if a telemetry SDK slips in. The settings window's content security policy still blocks every network request: only Clacket's Rust backend makes the activation call, never the webview.

## Install

Download the installer for your system from [Releases](../../releases/latest): `.dmg` for macOS, `.msi` or `.exe` for Windows, `.deb` or `.AppImage` for Linux.

- **macOS:** the first time Clacket starts, allow it under System Settings → Privacy & Security → **Input Monitoring**, then reopen it. Clacket shows a button that takes you there.
- **Windows:** no setup.
- **Linux:** Clacket reads `/dev/input`, which works on X11 and Wayland. Add yourself to the `input` group (`sudo usermod -aG input $USER`) and log out and back in.

## Website

[clacket.th33raphat.dev](https://clacket.th33raphat.dev) is a static Cloudflare Worker built from `site/`. It has a live demo that plays the same recordings in the browser, and it hosts the installers, since this repo is private. `scripts/build-site.sh` assembles it from the app's sounds, fonts and screenshots and the latest release's installers. Each release redeploys it.

The demo's recordings and the screenshots ship as AES-GCM blobs under `/a/`, with a fresh key each build (`scripts/pack-site.mjs`). The page decrypts them in memory, plays the sound with Web Audio and draws the screenshots on a canvas. `site/worker.js` serves `/a/*` only to the page's own `fetch()`, so a blob URL opened in a tab, hotlinked or curled gets a 404. This stops casual downloads. Anyone can still record what the browser plays or shows. The Open Graph image and the favicon stay plain files.

```sh
scripts/build-site.sh && (cd site && npx wrangler deploy)
```

## Build from source

You need Rust (stable), the Tauri CLI (`cargo install tauri-cli --version "^2" --locked`), and on Linux the [Tauri system packages](https://v2.tauri.app/start/prerequisites/#linux).

```sh
cd src-tauri
cargo test                        # unit tests
cargo run -- --settings           # run, opening the settings window
cargo tauri build                 # installers in src-tauri/target/release/bundle
```

The license Worker in `server/` has its own tests: `cd server && npm ci && npm test`.

## How it's put together

```
src-tauri/   the app (Rust + Tauri 2)
  src/input_macos.rs, input_windows.rs, input_linux.rs   key and mouse events from each OS
  src/audio.rs      decodes the active preset once; each key press is a lookup and a mix
  src/devices.rs    plug and unplug events (IOKit, Windows messages, Linux uevents)
  src/license.rs    offline Ed25519 key check
  data/             key table and keyboard shapes, in TOON
  assets/sounds/    switch and effect recordings, packed into the binary at build time
ui/          settings window: plain HTML, CSS and JS, no framework
server/      Cloudflare Worker that sells Pro through Stripe and signs keys
site/        clacket.th33raphat.dev: promo page and browser demo
scripts/     regenerate sounds and the key table; the no-network check
```

The key table isn't written by hand. `scripts/gen-keys.mjs` builds it from Chromium's `dom_code_data.inc`, which maps every key's native code on macOS, Windows and Linux to its web name.

## Releases

Pull request titles follow [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `perf:`). PRs are squash-merged, so each title becomes one commit on `main`. From those commits, [release-please](https://github.com/googleapis/release-please) keeps a release PR open with the next version number and a changelog of new features and fixes. Merging that PR tags the version, and CI attaches the installers for all three systems to the release.

### macOS code signing

macOS ties the Input Monitoring permission to an app's code signature. Without a stable signature every build gets a fresh ad-hoc identity, so each update looks like a different app and the permission is dropped — the app launches but hears nothing until you grant it again. A Developer ID signature keeps one identity across updates (it's based on your Team ID and the `app.clacket` bundle id, not the build), so the permission carries over and the release is also notarized, which clears Gatekeeper's "unidentified developer" warning.

The release workflow signs and notarizes when these repository secrets are set (**Settings → Secrets and variables → Actions**); with none set it falls back to an ad-hoc build, which still works.

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | Your **Developer ID Application** certificate, exported from Keychain as a `.p12` and base64-encoded (`base64 -i cert.p12 \| pbcopy`). |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12`. |
| `APPLE_SIGNING_IDENTITY` | The identity name, e.g. `Developer ID Application: Your Name (TEAMID)`. |
| `APPLE_ID` | The Apple ID email for notarization. |
| `APPLE_PASSWORD` | An [app-specific password](https://support.apple.com/en-us/102654) for that Apple ID, not the account password. |
| `APPLE_TEAM_ID` | Your 10-character team id, from [the membership page](https://developer.apple.com/account#MembershipDetailsCard). |

Getting the certificate needs an [Apple Developer Program](https://developer.apple.com/programs/) membership ($99/year). In its portal create a **Developer ID Application** certificate, install it in Keychain, then right-click it → **Export** to get the `.p12`. [`src-tauri/entitlements.plist`](src-tauri/entitlements.plist) carries the hardened-runtime entitlement notarization needs.

## Credits

- Switch recordings: [kbsim](https://github.com/tplai/kbsim) by Thomas Lai, MIT license.
- Mouse, gesture and plug sounds: [Kenney](https://kenney.nl), CC0.
- Key code table: [Chromium](https://chromium.googlesource.com/chromium/src/+/main/ui/events/keycodes/dom/dom_code_data.inc), BSD license.
- Fonts: [Mali](https://github.com/cadsondemak/Mali) and [Kanit](https://github.com/cadsondemak/kanit) by Cadson Demak, SIL Open Font License.

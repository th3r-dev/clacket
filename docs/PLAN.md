# Clacket: mechanical keyboard sounds while you type

## Context
A new project. The working name was "keymec"; the user picked **Clacket**. It's a tray app that plays a mechanical-switch sound on every key press, anywhere in the OS. Two hard goals: **low latency** and **minimal resource use while idle**. User choices:
- Cross-platform Rust + Tauri v2.
- Real recorded switch sounds by default (user pointed to typersguild.com/mechanical-key-sounds).
- The UI shows a keyboard drawn to match the **OS-detected layout type** (ANSI/ISO/JIS). Users click any key to give it a custom sound, and save the whole mapping as a **named preset**.
- UI designed per `th3-skills:design-core`; build lean per `ponytail`.
- **Free + Pro**: Pro is a $5 one-time lifetime purchase. It's sold through **Stripe**, and a small **Cloudflare Worker** issues signed license keys.
- **The app never uses the internet.** No network code, no activation call, no update check, no telemetry. Licenses are checked fully offline. The only outward action is `Upgrade – $5`, which opens the user's own browser; that's the browser going online, not Clacket.

Location: `/Users/th33raphat/Desktop/Project/clacket` (new folder, no existing code to reuse). Toolchain: rustc 1.98, cargo, ffmpeg present. `tauri-cli` is **not installed**; install it with `cargo install tauri-cli --version "^2" --locked`.

## Default sounds (verified source + license)
typersguild's tester loads the exact file set of **github.com/tplai/kbsim** (MIT license):
- 13 switches: alpaca, blackink, bluealps, boxnavy, buckling, cream, holypanda, mxblack, mxblue, mxbrown, redink, topre, turquoise.
- Each switch has `press/GENERIC_R0–R4, SPACE, ENTER, BACKSPACE` and `release/GENERIC, SPACE, ENTER, BACKSPACE`.
- MP3, about 2 KB each, about 400 KB in total.

typersguild states no license, so we **download from kbsim's repo**, not from typersguild. We ship kbsim's LICENSE and a credit in `assets/sounds/CREDITS.md` and in the About section.
- `scripts/fetch-sounds.sh` downloads the files once with `curl` from kbsim at a pinned commit. The files are committed; the script only exists to refresh them.
- `build.rs` scans `assets/sounds/` and generates an `include_bytes!` table, so the sounds are embedded in the binary: no runtime file I/O, and the app works offline.
- Default switch: **NK Cream** (a soft linear, the least tiring to listen to all day).

## Architecture
```
rdev listener thread ──(KeyCode, down/up)──▶ resolver: preset override ▸ switch clip for key kind ▸ GENERIC
                                               └▶ rodio mixer (one OutputStream, opened once), preloaded Arc<[f32]>
tray menu (native, always on) ── on/off · switch · preset · volume
settings window (webview) ── created only on demand, destroyed on close
```
- **Idle cost**: no webview exists unless Settings is open. The window is built on click and destroyed on close, not hidden. The tray menu is native. Targets: under 30 MB RSS, about 0% CPU when idle.
- **Key capture** (`keys.rs`): `rdev::listen` on its own thread. Maps `rdev::Key` to a stable **W3C `code` string** (`KeyA`, `Space`, `ShiftLeft`, …). The same IDs are used by the UI drawing and the presets. Auto-repeat is filtered with a held-key set. `KeyKind` (Normal/Space/Enter/Backspace) comes from the code string.
  - macOS needs the Input Monitoring permission. We detect when it's missing and show it in the UI. Linux works on X11 only (an rdev limitation); this is documented, not worked around.
- **Layout detection** (`layout.rs`, a single `fn detect() -> Layout { Ansi | Iso | Jis }`):
  - macOS: `KBGetLayoutType(LMGetKbdType())` through a small Carbon FFI.
  - Windows: `GetKeyboardType(0) == 7` means JIS. Otherwise the active HKL's language decides: UK, DE, FR, Nordic and similar mean ISO, everything else means ANSI. `ponytail:` this is a locale heuristic, so the UI always offers a manual override.
  - Linux: ANSI, with a manual override.
- **Audio** (`audio.rs`): `rodio` with `default-features=false, features=["mp3","wav"]`. Only the **active preset's** clips are decoded, once, into `Arc<[f32]>`: the base switch plus any per-key overrides, deduplicated by `SoundRef`. A press is one HashMap lookup by key code followed by `mixer.add`. GENERIC picks a random R0–R4 clip with ±3% pitch and ±8% gain jitter. At most 16 voices play at once. Release sounds play on key-up when that option is on.
  - `ponytail:` rodio allocates per press. Switch to a fixed voice-pool cpal callback if measured latency goes above 10 ms.
- **Presets & settings** (`settings.rs`): one `settings.json` in the OS config dir.
  ```
  { enabled, volume, keyup_sound, launch_at_login, layout_override: null|"ansi"|"iso"|"jis",
    active_preset: "Default",
    presets: [{ name, base_switch: "cream",
                keys: { "Space": {"builtin":"holypanda","clip":"SPACE"}, "KeyQ": {"file":"<uuid>.wav"} } }] }
  ```
  - `SoundRef` is either a `builtin` switch plus clip, or a `file`. A custom file is **copied** into `<app data>/sounds/<uuid>.<ext>` so it keeps working if the original moves. Validation: MP3 or WAV, at most 2 s and 5 MB; anything else is rejected with a clear message.
  - The `Default` preset always exists and can't be deleted. Changing the switch in the tray changes the active preset's `base_switch`.
- **Launch at login**: `tauri-plugin-autostart`. **Mute shortcut**: `tauri-plugin-global-shortcut`, ⌥⌘K on macOS, Ctrl+Alt+K elsewhere.

## Mouse, trackpad & plug sounds (Pro)
All sounds go through one map: **sound events are string IDs**. Every override and preset lives in that one map, so the resolver and the presets don't change shape. The preset field `keys` becomes `sounds`. Examples:
- keys: `key:KeyA`
- mouse: `mouse:left|right|middle`, `scroll`, `move`
- gestures: `tap`, `tap2`, `swipe`, `pinch`
- plugs: `plug:<category>:in|out`

**Mouse/trackpad** (`mouse.rs`, fed by the same rdev thread):
- **Click**: rdev `ButtonPress/Release` for left, right and middle.
- **Scroll ticks**: accumulate `|dx|+|dy|` and play one tick per threshold, like a wheel notch. The threshold is the "Sensitivity" slider. Capped at 40 ticks per second, so trackpad momentum scrolling doesn't machine-gun.
- **Movement sound**: one looping brown-noise clip. Its gain sits in an `AtomicU32` and follows pointer speed, updated at most 60 times a second. After 300 ms at 0 the sink pauses, so a still pointer costs no CPU. When the feature is off, the `MouseMove` handler returns on its first line, which matters for 1000 Hz gaming mice.
- **Taps and gestures** (macOS, in `gestures_macos.rs`): a listen-only CGEventTap for gesture events. These are swipe, magnify/pinch, and gesture begin/end, which rdev doesn't expose. Tap-to-click is told apart from a physical click by `kCGMouseEventPressure ≈ 0`. `ponytail:` this pressure test is a heuristic; when in doubt, the event counts as a click. Windows and Linux don't expose touchpad gestures globally, so only clicks and scroll work there, and that's documented.
- **Haptic tick** (macOS, `haptic_macos.rs`): `NSHapticFeedbackManager.defaultPerformer.perform(.generic)` through `objc2`, on scroll ticks and gestures. **First build step is a spike.** If macOS only allows it while Clacket is focused, the feature is dropped and the result reported, rather than shipped half-working.

**Plug sounds** (`devices.rs`, one `cfg` block per OS). Purely event-driven, with no polling:
- **macOS**:
  - USB: IOKit matching and terminated notifications for `IOUSBHostDevice`. The category comes from the class (HID gives input, mass storage gives storage, audio gives audio, PTP/MTP or Apple vendor gives phone, anything else is "other"). The port comes from the parent port's `UsbConnector` (0 = USB-A, 9 = USB-C) or a Thunderbolt path.
  - Charger: `IOPSNotificationCreateRunLoopSource`.
  - Display: `CGDisplayRegisterReconfigurationCallback`.
  - Headphones: a CoreAudio listener on the default output's data source and on default-device changes, which covers both wired and Bluetooth.
  - SD card and drives: DiskArbitration appear/disappear for removable media.
  - All of this runs on one thread with its own CFRunLoop.
- **Windows**: a message-only window receives `WM_DEVICECHANGE` (USB, volumes), `WM_POWERBROADCAST` (charger) and `WM_DISPLAYCHANGE`, plus `IMMNotificationClient` for audio endpoints. The port is reported as "USB". `ponytail:` connector type would need `IOCTL_USB_GET_PORT_CONNECTOR_PROPERTIES`; add it if people ask.
- **Linux**: a `udev` monitor on the `usb`, `power_supply`, `drm`, `sound` and `block` subsystems. USB-C is identified via `/sys/class/typec` when present.
- **Noise control**: ignore all events for the first 2 s (initial enumeration). Deduplicate per device for 500 ms, because composite devices fire several events.
- The UI shows a "Last event" line, for example "USB-C · Storage · Connected". It confirms detection and helps with debugging.

**Extra default sounds**: kbsim only has keys. The mouse, gesture and plug defaults come from **Kenney.nl** packs ("UI Audio" and "Interface Sounds", **CC0**; the license is re-checked when downloading). `fetch-sounds.sh` pulls them and picks clips for click, scroll tick, swipe, pinch, plug-in and unplug. The movement loop is made once at dev time with `ffmpeg -f lavfi anoisesrc=color=brown` plus a low-pass filter, and committed as an asset. No runtime synthesis code.

## Free vs Pro
| | Free | Pro ($5 once, lifetime, personal use) |
|---|---|---|
| Switches | NK Cream, MX Brown, MX Blue, Topre | all 13 |
| On/off, volume, key-up sound, tray, launch at login, mute shortcut | ✓ | ✓ |
| Keyboard tab: see the layout, live key highlight | ✓ (view only) | ✓ |
| Per-key sounds, your own sound files, presets, user packs | — | ✓ |
| Mouse & trackpad: click, scroll ticks, movement sound, tap & gesture sounds, haptic tick (macOS) | — | ✓ |
| Plug sounds: USB device type + port when known, charger, headphones, display, SD card | — | ✓ |

The gate lives in one place: `license::is_pro()`.
- The **commands** (`set_key_sound`, `import_sound`, `preset_*`, and `set_settings` with a locked switch) return a `ProRequired` error.
- The **resolver** ignores per-key overrides and locked switches when not Pro, falling back to NK Cream. Their data is kept, so if Pro is ever removed, nothing is lost and it comes back when a valid key is entered again.

## Licensing design (offline keys; Stripe + Cloudflare Worker on the selling side only)
```
[browser] Stripe Payment Link (hosted checkout) ──webhook checkout.session.completed──▶ Worker
Worker: build payload {v:1, lid, name/email, plan:"pro", iat}, sign with Ed25519, store in D1, email the key
[browser] Stripe success_url ─▶ Worker /thanks?session_id=… : shows the key, "Copy key", "Download clacket.license",
                                 and an "Open in Clacket" link (clacket://activate?key=…, handled locally)
[app, no network] paste key / open .license file / deep link ─▶ verify signature with embedded public key ─▶ Pro
```
- **The license key is self-contained**: base32 of `payload ‖ 64-byte signature`, prefixed `CLACKET-`, about 140 characters. The app embeds only the **public key**; the private key is a Worker secret, so nobody can forge keys. The app accepts the key three ways, all local: paste, open/drag a `clacket.license` file, or the `clacket://` deep link (`tauri-plugin-deep-link`). The key is stored in the app data dir and re-verified at every launch with pure math, no I/O beyond reading the file.
- **Trade-offs of going fully offline**, stated plainly:
  - **No device limit.** The app can't count installs without phoning home. The deterrent is that the buyer's name or email is inside the signed key and shown in About as "Licensed to …". This is the Sublime Text model, and it's fine at $5.
  - **Refunds can't switch Pro off** in installed copies. A refund marks the key revoked in D1, and future app releases embed a small list of revoked `lid`s at build time (`revoked.txt`, read by `build.rs`).
  - Any client-side check can be patched out. There will be no obfuscation and no DRM.
- **Worker endpoints** (`server/src/index.ts`), used only by the browser:
  - `POST /stripe/webhook`: verify the signature with `constructEventAsync`. Handles `checkout.session.completed` (issue the key) and `charge.refunded` (revoke). Idempotent per session ID.
  - `GET /thanks?session_id=`: show the key for a paid session.
  - `POST /recover {email}`: re-sends keys by email. It always answers "sent if it exists", so it can't be used to probe which emails bought. Rate-limited with the Workers rate-limit binding.
- **Storage**: one D1 table, `licenses(lid, key, email, session_id, revoked, created_at)`.
- **Email**: Cloudflare Email Service sends the key (subject "Your Clacket Pro key"), with `clacket.license` attached.
- **Secrets**: `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `LICENSE_SIGNING_KEY`. Nothing secret ships in the app.
- **App side** (`license.rs`, about 60 lines, `ed25519-dalek` + `data-encoding`): `parse_and_verify(key) -> Result<License>`, `is_pro()`, `remove()`.
- **Money**: $5 minus Stripe's 2.9% + 30¢ leaves about $4.55 before tax. Turning on Stripe Tax and registering for it is the owner's decision; it's flagged in the README, not automated.

## No-network guarantee (enforced, not just promised)
- No HTTP client crate in `src-tauri/Cargo.toml` and no Tauri `http`, `updater` or `websocket` plugins. A test fails the build if `cargo tree` shows `reqwest`, `hyper`, `ureq` or `tungstenite`.
- The webview CSP is `default-src 'self'; connect-src 'none'`, and capabilities grant no URL access. The UI is bundled locally, with no CDN and no web fonts.
- `Upgrade – $5` and `Lost your key?` use the opener plugin to hand a fixed `https://` URL to the OS browser. That's the only path out, and it only runs when clicked.
- The README and About state it: "Clacket never connects to the internet."
- At build time, load the `stripe:stripe-best-practices`, `cloudflare`, `wrangler` and `cloudflare-email-service` skills.

## Files
```
clacket/
  src-tauri/
    Cargo.toml, tauri.conf.json, build.rs (sound table), capabilities/default.json, icons/
    assets/sounds/<switch>/{press,release}/*.mp3, CREDITS.md, LICENSE-kbsim.md
    src/main.rs      tray, menu, commands, settings window lifecycle
    src/audio.rs     decode, resolver, play, voice cap, custom-file import + validation
    src/keys.rs      rdev listener, repeat filter, rdev::Key → code string
    src/layout.rs    detect ANSI/ISO/JIS per OS
    src/settings.rs  settings + presets json
    src/license.rs   offline Ed25519 key verify, is_pro(), revoked list from build.rs
    src/mouse.rs     click/scroll accumulator/movement gain
    src/gestures_macos.rs, src/haptic_macos.rs
    src/devices.rs   plug events (cfg macos / windows / linux)
    assets/sounds/fx/  Kenney CC0 clips + movement loop
  server/                      Cloudflare Worker (TypeScript)
    wrangler.toml, schema.sql, src/index.ts, src/index.test.ts
  ui/index.html, ui/style.css, ui/app.js, ui/layouts.js   (no framework, no bundler)
  brand/ logo.svg, tray-template.svg, BRAND.md
  scripts/fetch-sounds.sh
  README.md
```
`ui/layouts.js` holds static data: for ANSI, ISO and JIS, rows of `[code, label, widthUnits]` for the main block plus the nav cluster and arrows (TKL shape). Board-size options (60%, 75%, full) are skipped; add them if users ask.

## Branding (brand/)
- **Name**: Clacket (clack + racket). Tagline: "Every key, a little clack."
- **Mark**: a single keycap at a 3/4 angle with a sound arc. The tray icon is a monochrome template keycap outline (16/32 px).
- **Palette tokens**: ink `#16171A`, paper `#F4F1EA`, accent "switch orange" `#E8622C` (only for on-state, the selected key, the custom-sound marker and focus), muted `#8A8578`, dark surface `#1E1F23`. All pairs checked against WCAG AA.
- **Type**: the system UI font stack, so there's no webfont download. Wordmark in semibold with tight tracking.
- Icons come from the `svg-icon-generator` skill. `BRAND.md` is one page covering name, voice (plain and short, no "we"), palette, and logo do's and don'ts.

## UI (design-core applied)
**Tray menu** (the everyday UI): `Clacket On` checkbox · Preset ▸ · Switch ▸ grouped Linear (Alpaca, Black Ink, Red Ink, NK Cream, MX Black) / Tactile (Holy Panda, MX Brown, Turquoise, Topre) / Clicky (MX Blue, Box Navy, Blue Alps, Buckling Spring) · Volume ▸ 25/50/75/100 · `Open Settings…` · `Quit Clacket`.

**Settings window** (about 820×560). Header: wordmark, preset dropdown, and the On/Off switch (the focal point; accent color when on). Three tabs:

1. **Keyboard** (job: *"Give any key the sound you want."*)
   - The keyboard is drawn to scale from `layouts.js`, using the detected layout. A small label next to it reads "ANSI, detected" and has a `Change` link for the manual override.
   - Every key is a `<button>`. A key with a custom sound shows an accent dot. While the tab is focused, keys you press on your real keyboard light up, so you can find a key by pressing it.
   - Clicking a key selects it; Shift-click or drag selects several. A side panel for the selection shows the current sound, plus a `Sound` select (Switch default · any built-in switch, then a clip: press, space, enter or backspace) · `Choose file…` · `Preview` · `Reset to default`.
   - Preset bar: `Save as new preset…`, `Rename`, `Delete` (disabled on Default). Edits to the active preset save automatically.
   - Empty state when no key is selected: "Select a key to change its sound. Press it on your keyboard or click it here."
2. **Mouse & devices** (Pro; free users see it with the upgrade panel):
   - **Mouse & trackpad**: rows for Click (left/right/middle), Scroll tick (with a Sensitivity slider), Movement (on/off and volume), Tap, Two-finger tap, Swipe, Pinch, and Haptic tick (a macOS-only toggle that's hidden elsewhere).
   - **Devices**: rows for USB storage, Phone, Keyboard/mouse, Audio, Other USB, Charger, Headphones, Display and SD card. Each row has its plug-in and unplug sounds.
   - Every row has an on/off toggle, a `Sound` select (`Choose file…` included) and `Preview`. The "Last event" line sits at the top of Devices.
3. **Sound & options**: switch list grouped Linear/Tactile/Clicky, each row with a `Preview` button; a "Try it" text field; volume slider; key-up sound; launch at login; mute shortcut; About and credits (kbsim, MIT).

**Pro in the UI** (honest, not naggy):
- Locked switches show a small `Pro` tag in the tray and in Settings. Picking one plays its preview, then opens the upgrade sheet.
- Free users can see and explore the Keyboard tab. Clicking a key opens the side panel with "Give each key its own sound with Clacket Pro, $5 once." plus `Upgrade – $5` and `I have a key`.
- The upgrade sheet lists what Pro adds in 4 lines, with `Upgrade – $5` (opens your browser), a key field that accepts paste or a dropped `clacket.license` file, and `Lost your key?` (opens your browser).
- License states, all decided locally and instantly:
  - Pro: "Pro · Licensed to Jane D. · Remove license"
  - Invalid key: "This key isn't valid. Copy the whole key from your email, starting with CLACKET-."
  - Revoked: "This key was refunded, so Pro is off. Your settings are kept."
  - No spinner and no "connecting…" state, because there is no network step.
- No reminders, popups or countdowns in normal use.

**States**: if the permission is missing (macOS), a top banner says "Clacket can't hear your keys. Allow Input Monitoring in System Settings." with an `Open System Settings` button. If there's no output device: "No output device found. Connect speakers or headphones." If an imported file is invalid, the reason appears inline in the side panel, e.g. "This file is 4.1 s long. Use a clip under 2 seconds." If the layout can't be detected, ANSI is drawn and the label reads "ANSI. Change if your keyboard differs."

**A11y**: keys are real buttons with `aria-label` (for example "Q, custom sound") and `aria-pressed` for selection. Arrow keys move focus across the keyboard grid. Targets are at least 32 px; the drawing scales with the window. Visible focus ring, `prefers-color-scheme`, and `prefers-reduced-motion` (the key-light animation is turned off).

**Review**: screenshot both tabs in light and dark mode plus the permission-missing state. Score the 7 design-core dimensions and fix until every one is at least 8, in at most 3 rounds.

## Build order
1. Scaffold the Tauri v2 app: tray only, settings JSON.
2. `fetch-sounds.sh`, then `assets/sounds/` and credits, then `build.rs` table and `audio.rs`. A `--demo` flag plays every switch, so sounds can be checked without key capture.
3. `keys.rs` (code strings, repeat filter) wired to audio; then the resolver with presets.
4. Tray menu, mute shortcut, autostart.
5. `layout.rs` detection plus the `get_layout` command.
6. Brand assets, then `cargo tauri icon`.
7. Settings UI: Keyboard tab (layouts.js, selection, side panel, presets), then the Sound tab. Commands: `get_state`, `set_settings`, `set_key_sound`, `import_sound`, `preview`, `preset_save/rename/delete/activate`, `open_permissions`. A `key-event` emit to the window, only while it's open, drives the live highlight.
8. `license.rs`, the gate in commands and the resolver, and the Pro UI states.
8b. **Haptic spike** first: does it work system-wide? Keep it or drop it. Then `mouse.rs` and `gestures_macos.rs`, then `devices.rs` (macOS first; Windows and Linux written but untested here), then the Kenney assets and the Mouse & devices tab.
9. `server/` Worker + D1 schema in Stripe **test mode**, the `/thanks` page, the `.license` attachment, and the deep link. Generate the keypair with a script; the public key goes into `license.rs`.
10. README with setup steps (Stripe product + Payment Link, wrangler secrets, D1 migrate, keypair generation), with a humanizer pass.

Going live needs the owner's approval. Creating the live Stripe product, deploying the Worker and enabling Stripe Tax all wait for an explicit go-ahead.

## Verification
- `cargo test` in `src-tauri`:
  - every built-in switch decodes all 12 clips (non-empty, peak at most 1.0);
  - resolver order: key override first, then the switch clip for the key kind, then GENERIC;
  - preset serde round-trip;
  - import rejects oversized files and non-audio files;
  - the repeat filter emits once per hold;
  - every rdev key that `layouts.js` uses maps to a code string (a test that parses the JS file's codes).
- License tests:
  - Rust: a key signed by a test keypair passes; a single flipped byte, a truncated key, a key signed by another keypair and a revoked `lid` each fail; `is_pro()=false` makes the resolver ignore overrides and locked switches.
  - Cross-language: a key signed by the Worker code in its tests verifies in Rust. A fixture file is shared by both suites.
  - Worker (`vitest` with the Workers pool): webhook signature rejected when wrong; the same session ID twice creates one key; a refund marks the key revoked; `/recover` gives the same response for known and unknown emails.
- End to end in test mode: run `wrangler dev` and `stripe listen --forward-to localhost:8787/stripe/webhook`, pay with the test card 4242…, then **turn Wi-Fi off** and activate through paste, the `.license` file and the deep link. Confirm Pro turns on.
- No-network checks:
  - the `cargo tree` dependency test;
  - `lsof -i -a -p $(pgrep clacket)` shows **no sockets**, both idle and after using every feature including activation;
  - Little Snitch, if installed, sees no connection attempt.
- Mouse and devices:
  - Unit tests: the scroll accumulator emits the right tick count and respects the 40/s cap; the movement gain decays to pause; the device dedupe and the 2 s startup ignore work; USB class maps to the right category.
  - Manual on this Mac: trackpad click, tap, scroll, swipe and pinch each make their sound. Plug and unplug a USB-C drive, the charger and headphones, and check the sound and the "Last event" text.
  - `ps` while the pointer moves and while it's still; the movement sink must pause.
- `cargo tauri dev`: grant Input Monitoring, type in any app and hear the sound. Assign Space to Holy Panda and Q to a custom WAV, save as preset "Mixed", switch presets from the tray, and confirm the sounds change.
- Detection: the label shows the right layout on this Mac; the override redraws the ISO and JIS shapes.
- Resource check: `ps -o rss,%cpu -p $(pgrep clacket)` with Settings closed, both idle and while typing fast. Targets: under 30 MB, under 2% CPU while typing. Confirm memory drops after closing Settings.
- UI: design-core scoring from screenshots, plus a `th3-skills:qa` pass.
- `cargo tauri build` produces a `.app` and `.dmg` on macOS. Windows and Linux builds are noted as untested on this machine.

Skipped (add when asked): a landing page (the Payment Link is enough to sell), trials, coupons, team licenses, synthesized sounds, 60%/75%/full-size board shapes, per-app enable lists, Wayland, auto-update.

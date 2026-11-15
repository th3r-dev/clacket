# Changelog

## [0.4.0](https://github.com/th3r-dev/clacket/compare/v0.3.0...v0.4.0) (2026-11-15)


### New features

* go live — wire the live Stripe Payment Link into the app and site ([#27](https://github.com/th3r-dev/clacket/issues/27)) ([5bf005f](https://github.com/th3r-dev/clacket/commit/5bf005ff73b20687a810f8025c958d34764860f9))

## [0.3.0](https://github.com/th3r-dev/clacket/compare/v0.2.0...v0.3.0) (2026-11-15)


### New features

* **server:** license Worker on pay.th33raphat.dev, keys delivered on the thanks page (no email cost) ([#24](https://github.com/th3r-dev/clacket/issues/24)) ([9f04ef8](https://github.com/th3r-dev/clacket/commit/9f04ef8f1ee2bf60aecafd9d26253f2e3fa32f4b))
* **site:** clacket.th33raphat.dev with a live switch demo ([#18](https://github.com/th3r-dev/clacket/issues/18)) ([e6df10a](https://github.com/th3r-dev/clacket/commit/e6df10ac89f23fcffc6c8897dcaba5222b7f5d93))
* **site:** encrypt demo sounds and screenshots, play only while focused ([#23](https://github.com/th3r-dev/clacket/issues/23)) ([581ddb5](https://github.com/th3r-dev/clacket/commit/581ddb55a3df012fa71ae4e91cc17ab86e76bab2))
* **ui:** visual icon tiles for Mouse & devices, a distinct sound per device ([#25](https://github.com/th3r-dev/clacket/issues/25)) ([f63488c](https://github.com/th3r-dev/clacket/commit/f63488c0202e152a757304400884ad44971c1630))


### Fixes

* **ci:** bump Cargo.lock in release PRs ([#21](https://github.com/th3r-dev/clacket/issues/21)) ([491ddda](https://github.com/th3r-dev/clacket/commit/491dddafc907a49c195611c0f835ff871f2c2001))
* **ci:** Cargo.lock JSONPath for release-please's tagged TOML ([#22](https://github.com/th3r-dev/clacket/issues/22)) ([0a982b1](https://github.com/th3r-dev/clacket/commit/0a982b181b1c35d6b213effd5069bfcd68cd8c51))
* **ci:** macOS release build without Apple secrets; rebuild a tag on demand ([#19](https://github.com/th3r-dev/clacket/issues/19)) ([07363b3](https://github.com/th3r-dev/clacket/commit/07363b392eb93e9ed6844f259f8b353ebf6d4a56))

## [0.2.0](https://github.com/th3r-dev/clacket/compare/v0.1.0...v0.2.0) (2026-11-15)


### New features

* **core:** Tauri app with offline audio engine, native input, layouts, plug sounds and offline licensing ([ad64493](https://github.com/th3r-dev/clacket/commit/ad64493ee69a1e4a790650e984e08b0bc775b10b))
* **server:** license Worker (Stripe webhook, thanks page, key recovery, D1) ([049ecfe](https://github.com/th3r-dev/clacket/commit/049ecfe0b103f361b2687f401c508906bdaa2af5))
* settings window with keyboard editor, switch cards and Pro upgrade ([#2](https://github.com/th3r-dev/clacket/issues/2)) ([a3101ad](https://github.com/th3r-dev/clacket/commit/a3101ad99188911891cfb1373de8e11f9d7fb9cb))
* **sounds:** bundle kbsim switch sounds (MIT) and Kenney fx (CC0) with fetch script ([e05dddd](https://github.com/th3r-dev/clacket/commit/e05dddd6f0afdfbb64d0f6d9d6f09d1ef8e06280))

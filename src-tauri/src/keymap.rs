//! Native key code -> KeyboardEvent.code, loaded from data/keys.toon (generated from Chromium's
//! table by scripts/gen-keys.mjs). One lookup table per platform, built once.
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Deserialize)]
struct Entry {
    code: String,
    label: String,
    mac: Option<u32>,
    win: Option<u32>,
    evdev: Option<u32>,
}

#[derive(Deserialize)]
struct File {
    keys: Vec<Entry>,
}

pub struct KeyMap {
    native: HashMap<u32, &'static str>,
    pub labels: HashMap<&'static str, &'static str>,
}

/// The native code column used by this platform's input backend.
fn column(e: &Entry) -> Option<u32> {
    if cfg!(target_os = "macos") { e.mac } else if cfg!(target_os = "windows") { e.win } else { e.evdev }
}

pub fn get() -> &'static KeyMap {
    static MAP: OnceLock<KeyMap> = OnceLock::new();
    MAP.get_or_init(|| {
        let entries = toon_format::decode_default::<File>(include_str!("../data/keys.toon")).expect("keys.toon").keys;
        let mut native = HashMap::new();
        let mut labels = HashMap::new();
        for e in entries {
            let col = column(&e);
            // leaked once at startup: ~200 short strings that live for the whole process
            let code: &'static str = Box::leak(e.code.into_boxed_str());
            labels.insert(code, &*Box::leak(e.label.into_boxed_str()));
            if let Some(n) = col {
                native.entry(n).or_insert(code);
            }
        }
        KeyMap { native, labels }
    })
}

pub fn code(native: u32) -> Option<&'static str> {
    get().native.get(&native).copied()
}

/// Legends for printable keys in the user's current keyboard layout (Thai, AZERTY, ...).
/// Keys the layout doesn't produce a visible character for keep the generated label.
#[cfg(target_os = "macos")]
pub fn os_labels() -> HashMap<String, String> {
    use core_foundation::{base::TCFType, data::CFData};
    use std::ffi::c_void;
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
        fn TISGetInputSourceProperty(src: *mut c_void, key: *const c_void) -> *const c_void;
        static kTISPropertyUnicodeKeyLayoutData: *const c_void;
        fn LMGetKbdType() -> u8;
        fn UCKeyTranslate(layout: *const c_void, code: u16, action: u16, mods: u32, kbd: u32, opts: u32,
            dead: *mut u32, max: usize, len: *mut usize, chars: *mut u16) -> i32;
        fn CFRelease(p: *const c_void);
    }
    let mut out = HashMap::new();
    unsafe {
        let src = TISCopyCurrentKeyboardLayoutInputSource();
        if src.is_null() {
            return out;
        }
        let data = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData);
        if !data.is_null() {
            let data = CFData::wrap_under_get_rule(data as _);
            let layout = data.bytes().as_ptr() as *const c_void;
            let kbd = LMGetKbdType() as u32;
            for (native, code) in &get().native {
                let (mut dead, mut len, mut buf) = (0u32, 0usize, [0u16; 4]);
                // action 0 = key down, opts 1 = no dead-key processing
                if UCKeyTranslate(layout, *native as u16, 0, 0, kbd, 1, &mut dead, 4, &mut len, buf.as_mut_ptr()) == 0 && len > 0 {
                    let s = String::from_utf16_lossy(&buf[..len]);
                    if s.chars().all(|c| !c.is_control() && !c.is_whitespace()) {
                        out.insert(code.to_string(), s.to_uppercase());
                    }
                }
            }
        }
        CFRelease(src);
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn os_labels() -> HashMap<String, String> {
    HashMap::new() // ponytail: ToUnicodeEx (Windows) / xkbcommon (Linux) legends when someone asks
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_loads_and_maps() {
        let m = super::get();
        assert!(m.labels.len() > 150);
        #[cfg(target_os = "macos")]
        {
            assert_eq!(super::code(0), Some("KeyA"));
            assert_eq!(super::code(49), Some("Space"));
            assert_eq!(super::code(126), Some("ArrowUp"));
            assert_eq!(super::code(56), Some("ShiftLeft"));
        }
        assert_eq!(m.labels["KeyA"], "A");
    }
}

/// On-screen keyboard geometry from data/layouts.toon (tenkeyless).
/// Each row is a string of tokens: a KeyboardEvent.code with an optional ":width" in key units
/// (default 1), "_" for a gap ("_:0.5" = half a key), and "|" to start the nav cluster.
#[derive(Deserialize, serde::Serialize, Clone)]
pub struct Layouts {
    pub ansi: Vec<String>,
    pub iso: Vec<String>,
    pub jis: Vec<String>,
}

pub fn layouts() -> &'static Layouts {
    static L: OnceLock<Layouts> = OnceLock::new();
    L.get_or_init(|| toon_format::decode_default(include_str!("../data/layouts.toon")).expect("layouts.toon"))
}

#[cfg(test)]
mod layout_tests {
    #[test]
    fn every_layout_code_is_a_known_key() {
        let l = super::layouts();
        for rows in [&l.ansi, &l.iso, &l.jis] {
            assert_eq!(rows.len(), 6);
            for tok in rows.iter().flat_map(|r| r.split_whitespace()) {
                let code = tok.split(':').next().unwrap();
                if code == "_" || code == "|" {
                    continue;
                }
                assert!(super::get().labels.contains_key(code), "unknown code {code}");
            }
        }
    }
}

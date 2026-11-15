//! Physical keyboard shape: ANSI, ISO or JIS. The UI always allows a manual override.

#[cfg(target_os = "macos")]
pub fn detect() -> &'static str {
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn LMGetKbdType() -> u8;
        fn KBGetLayoutType(kbd: i16) -> u32;
    }
    match unsafe { KBGetLayoutType(LMGetKbdType() as i16) } {
        0x4A49_5320 => "jis", // 'JIS '
        0x4953_4F20 => "iso", // 'ISO '
        _ => "ansi",
    }
}

#[cfg(target_os = "windows")]
pub fn detect() -> &'static str {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardType};
    if unsafe { GetKeyboardType(0) } == 7 {
        return "jis";
    }
    // ponytail: locale heuristic — Windows doesn't report the physical shape; the UI has an override
    let lang = (unsafe { GetKeyboardLayout(0) } as usize & 0x3FF) as u16;
    // en-GB is a sublanguage, so check the full low word for it
    let full = (unsafe { GetKeyboardLayout(0) } as usize & 0xFFFF) as u16;
    const ISO_LANGS: &[u16] = &[0x07, 0x0C, 0x0A, 0x10, 0x13, 0x14, 0x1D, 0x06, 0x0B, 0x0F, 0x15, 0x05, 0x0E, 0x16, 0x1A, 0x24, 0x1B, 0x22, 0x19, 0x1F, 0x08];
    if full == 0x0809 || full == 0x1809 || ISO_LANGS.contains(&lang) { "iso" } else { "ansi" }
}

#[cfg(target_os = "linux")]
pub fn detect() -> &'static str {
    "ansi"
}

//! Windows low-level keyboard + mouse hooks. Scan codes go through keymap (Chromium's "win" column).
//! Untested on this Mac; type-checked with `cargo check --target x86_64-pc-windows-msvc`.
use crate::mouse::Input;
use std::{collections::HashSet, sync::{Mutex, OnceLock}};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

type Callback = Box<dyn Fn(Input) + Send + Sync>;
static CB: OnceLock<Callback> = OnceLock::new();
static HELD: Mutex<Option<HashSet<u32>>> = Mutex::new(None);
static LAST_PT: Mutex<Option<(i32, i32)>> = Mutex::new(None);

pub fn permitted() -> bool { true }
pub fn request_permission() {}

pub fn start(cb: impl Fn(Input) + Send + Sync + 'static) -> bool {
    if CB.set(Box::new(cb)).is_err() {
        return false;
    }
    std::thread::Builder::new().name("clacket-input".into()).spawn(|| unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb), std::ptr::null_mut(), 0);
        SetWindowsHookExW(WH_MOUSE_LL, Some(ms), std::ptr::null_mut(), 0);
        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
    }).is_ok()
}

unsafe extern "system" fn kb(n: i32, w: usize, l: isize) -> isize {
    if n >= 0 {
        let k = &*(l as *const KBDLLHOOKSTRUCT);
        let sc = k.scanCode | if k.flags & LLKHF_EXTENDED != 0 { 0xE000 } else { 0 };
        let down = w as u32 == WM_KEYDOWN || w as u32 == WM_SYSKEYDOWN;
        let mut held = HELD.lock().unwrap();
        let held = held.get_or_insert_with(HashSet::new);
        let fresh = if down { held.insert(sc) } else { held.remove(&sc) };
        if fresh {
            if let (Some(code), Some(cb)) = (crate::keymap::code(sc), CB.get()) {
                cb(Input::Key { code, down });
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), n, w, l)
}

unsafe extern "system" fn ms(n: i32, w: usize, l: isize) -> isize {
    if n >= 0 {
        let m = &*(l as *const MSLLHOOKSTRUCT);
        let ev = match w as u32 {
            WM_LBUTTONDOWN => Some(Input::Button { which: "left", down: true, tap: false }),
            WM_LBUTTONUP => Some(Input::Button { which: "left", down: false, tap: false }),
            WM_RBUTTONDOWN => Some(Input::Button { which: "right", down: true, tap: false }),
            WM_RBUTTONUP => Some(Input::Button { which: "right", down: false, tap: false }),
            WM_MBUTTONDOWN => Some(Input::Button { which: "middle", down: true, tap: false }),
            WM_MBUTTONUP => Some(Input::Button { which: "middle", down: false, tap: false }),
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                // one notch = 120 units = 40 points, matching the macOS point deltas
                let d = ((m.mouseData >> 16) as i16) as f64 / 3.0;
                Some(if w as u32 == WM_MOUSEWHEEL { Input::Scroll { dx: 0.0, dy: d } } else { Input::Scroll { dx: d, dy: 0.0 } })
            }
            WM_MOUSEMOVE => {
                let mut last = LAST_PT.lock().unwrap();
                let d = last.map(|(x, y)| ((m.pt.x - x) as f64, (m.pt.y - y) as f64));
                *last = Some((m.pt.x, m.pt.y));
                d.map(|(dx, dy)| Input::Move { dx, dy })
            }
            _ => None,
        };
        if let (Some(ev), Some(cb)) = (ev, CB.get()) {
            cb(ev);
        }
    }
    CallNextHookEx(std::ptr::null_mut(), n, w, l)
}

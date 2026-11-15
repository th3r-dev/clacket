//! One listen-only CGEventTap for keys, mouse, scroll and trackpad gestures.
//! rdev isn't used on macOS: it looks up keyboard layouts off the main thread and crashes
//! on recent macOS releases.
use crate::mouse::Input;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource};
use core_foundation::base::TCFType;
use std::{collections::HashSet, ffi::c_void, sync::Mutex};

type Callback = Box<dyn Fn(Input) + Send + Sync>;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64,
        cb: extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        info: *mut c_void) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetIntegerValueField(e: *mut c_void, field: u32) -> i64;
    fn CGEventGetDoubleValueField(e: *mut c_void, field: u32) -> f64;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CFMachPortCreateRunLoopSource(alloc: *const c_void, port: *mut c_void, order: isize) -> *mut c_void;
}

const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const FLAGS_CHANGED: u32 = 12;
const SCROLL: u32 = 22;
const GESTURE: u32 = 29;
const TAP_DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_USER: u32 = 0xFFFF_FFFF;
// CGEventField
const F_AUTOREPEAT: u32 = 8;
const F_KEYCODE: u32 = 9;
const F_PRESSURE: u32 = 2;
const F_DELTA_X: u32 = 4;
const F_DELTA_Y: u32 = 5;
const F_SCROLL_PT_Y: u32 = 96;
const F_SCROLL_PT_X: u32 = 97;
const F_GESTURE_HID_TYPE: u32 = 110;

struct State {
    cb: Callback,
    held: Mutex<HashSet<i64>>,
    tap: Mutex<usize>,
}

pub fn permitted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

pub fn request_permission() {
    unsafe { CGRequestListenEventAccess(); }
}

/// Starts the tap on its own run-loop thread. Returns false if macOS refused it
/// (Input Monitoring not granted).
pub fn start(cb: impl Fn(Input) + Send + Sync + 'static) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new().name("clacket-input".into()).spawn(move || unsafe {
        let state: &'static State = Box::leak(Box::new(State { cb: Box::new(cb), held: Mutex::default(), tap: Mutex::new(0) }));
        let mut mask: u64 = 0;
        for t in [1u32, 2, 3, 4, 5, 6, 7, KEY_DOWN, KEY_UP, FLAGS_CHANGED, SCROLL, 25, 26, 27, GESTURE, 30, 31] {
            mask |= 1 << t;
        }
        // 1 = kCGSessionEventTap, 0 = head insert, 1 = listen only
        let tap = CGEventTapCreate(1, 0, 1, mask, handler, state as *const State as *mut c_void);
        if tap.is_null() {
            let _ = tx.send(false);
            return;
        }
        *state.tap.lock().unwrap() = tap as usize;
        let src = CFRunLoopSource::wrap_under_create_rule(CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0) as _);
        CFRunLoop::get_current().add_source(&src, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        let _ = tx.send(true);
        CFRunLoop::run_current();
    }).unwrap();
    rx.recv().unwrap_or(false)
}

extern "C" fn handler(_proxy: *mut c_void, ty: u32, e: *mut c_void, info: *mut c_void) -> *mut c_void {
    let st = unsafe { &*(info as *const State) };
    if ty == TAP_DISABLED_TIMEOUT || ty == TAP_DISABLED_USER {
        unsafe { CGEventTapEnable(*st.tap.lock().unwrap() as *mut c_void, true) };
        return e;
    }
    let int = |f| unsafe { CGEventGetIntegerValueField(e, f) };
    let dbl = |f| unsafe { CGEventGetDoubleValueField(e, f) };
    let ev = match ty {
        KEY_DOWN | KEY_UP => {
            if ty == KEY_DOWN && int(F_AUTOREPEAT) != 0 {
                return e;
            }
            let kc = int(F_KEYCODE);
            let mut held = st.held.lock().unwrap();
            let down = ty == KEY_DOWN;
            if down { held.insert(kc); } else { held.remove(&kc); }
            crate::keymap::code(kc as u32).map(|code| Input::Key { code, down })
        }
        FLAGS_CHANGED => {
            // modifiers come as flag changes; toggle on the keycode
            let kc = int(F_KEYCODE);
            let mut held = st.held.lock().unwrap();
            let down = held.insert(kc);
            if !down { held.remove(&kc); }
            crate::keymap::code(kc as u32).map(|code| Input::Key { code, down })
        }
        1..=4 | 25 | 26 => {
            let which = match ty { 1 | 2 => "left", 3 | 4 => "right", _ => "middle" };
            let down = matches!(ty, 1 | 3 | 25);
            // ponytail: tap-to-click heuristic — a tap has no pressure, a physical click does
            let tap = down && dbl(F_PRESSURE) < 0.01;
            Some(Input::Button { which, down, tap })
        }
        5..=7 | 27 => Some(Input::Move { dx: int(F_DELTA_X) as f64, dy: int(F_DELTA_Y) as f64 }),
        SCROLL => Some(Input::Scroll { dx: dbl(F_SCROLL_PT_X), dy: dbl(F_SCROLL_PT_Y) }),
        GESTURE | 30 | 31 => {
            // IOHIDEventType: 5 rotation, 7 scale, 8 zoom, 16 navigation swipe, 23 dock swipe
            // ponytail: undocumented field; burst-debounced by the caller
            match int(F_GESTURE_HID_TYPE) {
                7 | 8 => Some(Input::Gesture("pinch")),
                16 | 23 => Some(Input::Gesture("swipe")),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(ev) = ev {
        (st.cb)(ev);
    }
    e
}

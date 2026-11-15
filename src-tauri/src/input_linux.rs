//! Linux input straight from evdev (/dev/input/event*), so it works on X11 and Wayland.
//! Needs read access to /dev/input: add the user to the `input` group. Untested on this Mac.
use crate::mouse::Input;
use evdev::{Device, EventSummary, KeyCode, RelativeAxisCode};

pub fn permitted() -> bool {
    evdev::enumerate().next().is_some()
}

pub fn request_permission() {}

pub fn start(cb: impl Fn(Input) + Send + Sync + 'static) -> bool {
    let cb = std::sync::Arc::new(cb);
    let mut any = false;
    for (_, dev) in evdev::enumerate() {
        let keys = dev.supported_keys().is_some_and(|k| k.contains(KeyCode::KEY_A) || k.contains(KeyCode::BTN_LEFT));
        if !keys {
            continue;
        }
        any = true;
        let cb = cb.clone();
        std::thread::Builder::new().name("clacket-input".into()).spawn(move || run(dev, &*cb)).ok();
    }
    any
}

fn run(mut dev: Device, cb: &(dyn Fn(Input) + Send + Sync)) {
    while let Ok(events) = dev.fetch_events() {
        for e in events {
            let ev = match e.destructure() {
                // value: 1 press, 0 release, 2 auto-repeat (ignored)
                EventSummary::Key(_, k, v) if v != 2 => {
                    let down = v == 1;
                    match k {
                        KeyCode::BTN_LEFT => Some(Input::Button { which: "left", down, tap: false }),
                        KeyCode::BTN_RIGHT => Some(Input::Button { which: "right", down, tap: false }),
                        KeyCode::BTN_MIDDLE => Some(Input::Button { which: "middle", down, tap: false }),
                        _ => crate::keymap::code(k.code() as u32).map(|code| Input::Key { code, down }),
                    }
                }
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, v) => Some(Input::Move { dx: v as f64, dy: 0.0 }),
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_Y, v) => Some(Input::Move { dx: 0.0, dy: v as f64 }),
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_WHEEL, v) => Some(Input::Scroll { dx: 0.0, dy: v as f64 * 40.0 }),
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_HWHEEL, v) => Some(Input::Scroll { dx: v as f64 * 40.0, dy: 0.0 }),
                _ => None,
            };
            if let Some(ev) = ev {
                cb(ev);
            }
        }
    }
}

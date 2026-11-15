// Clacket: plays switch sounds for keys, mouse, trackpad and plugged devices.
// Never opens a network connection; see README "No network".
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod devices;
mod haptic;
mod keymap;
mod layout;
mod license;
mod mouse;
mod pack;
mod settings;

#[cfg(target_os = "macos")]
#[path = "input_macos.rs"]
mod input;
#[cfg(target_os = "windows")]
#[path = "input_windows.rs"]
mod input;
#[cfg(target_os = "linux")]
#[path = "input_linux.rs"]
mod input;

use audio::{Bank, Engine};
use mouse::{GestureDebounce, Input, MoveSpeed, ScrollTicks};
use serde_json::{json, Value};
use settings::{Settings, SoundRef, FREE_SWITCHES, FX_EVENTS, SWITCHES};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        Arc, Mutex,
    },
    time::Instant,
};
use tauri::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Wry,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

/// Opened in the user's browser, never fetched by Clacket. Set these before release.
const BUY_URL: &str = "https://buy.stripe.com/REPLACE_WITH_PAYMENT_LINK";
const RECOVER_URL: &str = "https://clacket-license.REPLACE.workers.dev/recover";
const PRO_REQUIRED: &str = "PRO_REQUIRED";

struct App {
    s: Mutex<Settings>,
    config_path: PathBuf,
    sounds_dir: PathBuf,
    engine: Arc<Engine>,
    pro: AtomicBool,
    licensed_to: Mutex<Option<String>>,
    license_note: Mutex<Option<String>>,
    last_event: Mutex<String>,
    input_ok: AtomicBool,
    window_open: AtomicBool,
    scroll: Mutex<ScrollTicks>,
    speed: Mutex<MoveSpeed>,
    gesture: Mutex<GestureDebounce>,
    tray: Mutex<Option<TrayIcon>>,
    handle: AppHandle,
}

type AppState<'a> = State<'a, Arc<App>>;

impl App {
    fn pro(&self) -> bool {
        self.pro.load(Relaxed)
    }

    /// Pushes settings into the engine. `rebuild` re-decodes the bank (preset/sound changes only).
    fn apply(&self, rebuild: bool) {
        let s = self.s.lock().unwrap().clone();
        let e = &self.engine;
        e.volume.store(s.volume.to_bits(), Relaxed);
        e.move_volume.store(s.move_volume.to_bits(), Relaxed);
        e.enabled.store(s.enabled, Relaxed);
        e.keyup.store(s.keyup_sound, Relaxed);
        if rebuild {
            *e.bank.write().unwrap() = Bank::build(&s, self.pro(), &self.sounds_dir);
        }
    }

    fn save(&self) -> Result<(), String> {
        self.s.lock().unwrap().save(&self.config_path).map_err(|e| format!("Couldn't save settings: {e}"))
    }

    /// Change settings, save, apply, refresh the tray and the window.
    fn change(&self, rebuild: bool, f: impl FnOnce(&mut Settings) -> Result<(), String>) -> Result<(), String> {
        {
            let mut s = self.s.lock().unwrap();
            f(&mut s)?;
            s.repair();
        }
        self.save()?;
        self.apply(rebuild);
        self.refresh_tray();
        let _ = self.handle.emit("state-changed", ());
        Ok(())
    }

    fn load_license(&self) {
        let key = self.s.lock().unwrap().license_key.clone();
        let (pro, name, note) = match key.as_deref().map(license::verify) {
            Some(Ok(l)) => (true, Some(l.name), None),
            Some(Err(license::LicenseError::Revoked)) => (false, None, Some("This key was refunded, so Pro is off. Your settings are kept.".to_string())),
            Some(Err(_)) => (false, None, Some("The saved license key isn't valid anymore.".to_string())),
            None => (false, None, None),
        };
        self.pro.store(pro, Relaxed);
        *self.licensed_to.lock().unwrap() = name;
        *self.license_note.lock().unwrap() = note;
    }

    fn on_input(&self, ev: Input) {
        let now = Instant::now();
        let pro = self.pro();
        let (id, down) = match ev {
            Input::Key { code, down } => {
                if self.window_open.load(Relaxed) {
                    let _ = self.handle.emit("key-event", json!({ "code": code, "down": down }));
                }
                (format!("key:{code}"), down)
            }
            _ if !pro => return,
            Input::Button { which, down, tap } => {
                if !down {
                    ("mouse:up".into(), true)
                } else if tap {
                    ((if which == "right" { "tap2" } else { "tap" }).into(), true)
                } else {
                    (format!("mouse:{which}"), true)
                }
            }
            Input::Scroll { dx, dy } => {
                let sens = self.s.lock().unwrap().scroll_sensitivity;
                if !self.scroll.lock().unwrap().feed(dx, dy, sens, now) {
                    return;
                }
                self.haptic_tick();
                ("scroll".into(), true)
            }
            Input::Move { dx, dy } => {
                if let Some(v) = self.speed.lock().unwrap().feed(dx, dy, now) {
                    self.engine.movement(v);
                }
                return;
            }
            Input::Gesture(kind) => {
                if !self.gesture.lock().unwrap().feed(kind, now) {
                    return;
                }
                self.haptic_tick();
                (kind.into(), true)
            }
        };
        self.engine.event(&id, down);
    }

    fn haptic_tick(&self) {
        if self.s.lock().unwrap().haptic && self.engine.enabled.load(Relaxed) {
            let _ = self.handle.run_on_main_thread(haptic::tick); // AppKit wants the main thread
        }
    }

    fn on_device(&self, ev: devices::DeviceEvent) {
        if ev.category == "headphones" || ev.category == "audio" {
            self.engine.reopen(); // default output may have moved
        }
        *self.last_event.lock().unwrap() = ev.describe();
        let _ = self.handle.emit("device-event", ev.describe());
        if self.pro() {
            self.engine.event(&format!("plug:{}:{}", ev.category, if ev.connected { "in" } else { "out" }), true);
        }
    }

    fn refresh_tray(&self) {
        if let (Some(tray), Ok(menu)) = (self.tray.lock().unwrap().as_ref(), tray_menu(&self.handle, self)) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn tray_menu(h: &AppHandle, app: &App) -> tauri::Result<Menu<Wry>> {
    let s = app.s.lock().unwrap().clone();
    let pro = app.pro();
    let on = CheckMenuItem::with_id(h, "on", "Clacket On", true, s.enabled, Some("CmdOrCtrl+Alt+K"))?;

    let presets: Vec<CheckMenuItem<Wry>> = s.presets.iter()
        .map(|p| CheckMenuItem::with_id(h, format!("preset:{}", p.name), &p.name, pro || p.name == "Default", p.name == s.active_preset, None::<&str>))
        .collect::<Result<_, _>>()?;
    let presets = Submenu::with_items(h, "Preset", true, &presets.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect::<Vec<_>>())?;

    let mut sw_items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![];
    let mut group = "";
    for (id, label, g) in SWITCHES {
        if *g != group {
            if !group.is_empty() {
                sw_items.push(Box::new(PredefinedMenuItem::separator(h)?));
            }
            sw_items.push(Box::new(MenuItem::with_id(h, format!("group:{g}"), *g, false, None::<&str>)?));
            group = g;
        }
        let locked = !pro && !FREE_SWITCHES.contains(id);
        let text = if locked { format!("{label}  · Pro") } else { label.to_string() };
        sw_items.push(Box::new(CheckMenuItem::with_id(h, format!("switch:{id}"), text, true, s.preset().base_switch == *id, None::<&str>)?));
    }
    let switches = Submenu::with_items(h, "Switch", true, &sw_items.iter().map(|b| b.as_ref()).collect::<Vec<_>>())?;

    let vols: Vec<CheckMenuItem<Wry>> = [25, 50, 75, 100].iter()
        .map(|v| CheckMenuItem::with_id(h, format!("vol:{v}"), format!("{v}%"), true, (s.volume * 100.0).round() as i32 == *v, None::<&str>))
        .collect::<Result<_, _>>()?;
    let volume = Submenu::with_items(h, "Volume", true, &vols.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect::<Vec<_>>())?;

    Menu::with_items(h, &[
        &on,
        &PredefinedMenuItem::separator(h)?,
        &presets,
        &switches,
        &volume,
        &PredefinedMenuItem::separator(h)?,
        &MenuItem::with_id(h, "settings", "Open Settings…", true, Some("CmdOrCtrl+,"))?,
        &MenuItem::with_id(h, "quit", "Quit Clacket", true, Some("CmdOrCtrl+Q"))?,
    ])
}

fn open_settings(h: &AppHandle) {
    if let Some(w) = h.get_webview_window("settings") {
        let _ = w.set_focus();
        return;
    }
    let app = h.state::<Arc<App>>().inner().clone();
    let b = WebviewWindowBuilder::new(h, "settings", WebviewUrl::App("index.html".into()))
        .title("Clacket")
        .inner_size(880.0, 640.0)
        .min_inner_size(720.0, 560.0);
    // macOS: content runs under a transparent title bar; the name is hidden and the window
    // buttons only show while the pointer is near the top (see window_controls)
    #[cfg(target_os = "macos")]
    let b = b.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    if let Ok(w) = b.build() {
        #[cfg(target_os = "macos")]
        set_window_controls(&w, false, false);
        app.window_open.store(true, Relaxed);
        let a = app.clone();
        // closing destroys the webview, so idle Clacket holds no browser engine in memory
        w.on_window_event(move |e| {
            if matches!(e, tauri::WindowEvent::Destroyed) {
                a.window_open.store(false, Relaxed);
            }
        });
    }
}

fn on_menu(h: &AppHandle, id: &str) {
    let app = h.state::<Arc<App>>().inner().clone();
    let r = match id {
        "on" => app.change(false, |s| { s.enabled = !s.enabled; Ok(()) }),
        "settings" => { open_settings(h); Ok(()) }
        "quit" => { h.exit(0); Ok(()) }
        _ if id.starts_with("vol:") => {
            let v: f32 = id[4..].parse().unwrap_or(70.0);
            app.change(false, |s| { s.volume = v / 100.0; Ok(()) })
        }
        _ if id.starts_with("switch:") => set_switch_inner(&app, &id[7..]),
        _ if id.starts_with("preset:") => {
            let name = id[7..].to_string();
            app.change(true, |s| { if name != "Default" && !app.pro() { return Err(PRO_REQUIRED.into()); } s.active_preset = name; Ok(()) })
        }
        _ => Ok(()),
    };
    if matches!(&r, Err(e) if e == PRO_REQUIRED) {
        open_settings(h);
        let _ = h.emit("show-upgrade", ());
        app.refresh_tray(); // undo the checkmark the click toggled
    }
}

fn set_switch_inner(app: &App, id: &str) -> Result<(), String> {
    if !SWITCHES.iter().any(|(s, _, _)| *s == id) {
        return Err("Unknown switch.".into());
    }
    if !app.pro() && !FREE_SWITCHES.contains(&id) {
        // let them hear it before asking
        if let Some(c) = pack::clip(&format!("{id}/press/GENERIC_R0")).and_then(|b| audio::decode(b).ok()) {
            app.engine.play_clip(c, false);
        }
        return Err(PRO_REQUIRED.into());
    }
    app.change(true, |s| { s.preset_mut().base_switch = id.to_string(); Ok(()) })
}

// ---------- commands (sync, so they run on the main thread; TIS layout calls need it) ----------

#[tauri::command]
fn get_state(app: AppState) -> Value {
    let s = app.s.lock().unwrap().clone();
    let pro = app.pro();
    let km = keymap::get();
    let mut labels: std::collections::HashMap<String, String> = km.labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    labels.extend(keymap::os_labels());
    json!({
        "settings": {
            "enabled": s.enabled, "volume": s.volume, "keyup_sound": s.keyup_sound,
            "launch_at_login": s.launch_at_login, "layout_override": s.layout_override,
            "active_preset": s.active_preset, "scroll_sensitivity": s.scroll_sensitivity,
            "move_volume": s.move_volume, "haptic": s.haptic, "off": s.off,
            "presets": s.presets,
        },
        "pro": pro,
        "licensed_to": *app.licensed_to.lock().unwrap(),
        "license_note": *app.license_note.lock().unwrap(),
        "switches": SWITCHES.iter().map(|(id, label, group)| json!({
            "id": id, "label": label, "group": group, "locked": !pro && !FREE_SWITCHES.contains(id)
        })).collect::<Vec<_>>(),
        "fx_events": FX_EVENTS.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        "detected_layout": layout::detect(),
        "layouts": keymap::layouts(),
        "labels": labels,
        "input_ok": app.input_ok.load(Relaxed),
        "has_output": app.engine.has_output(),
        "last_event": *app.last_event.lock().unwrap(),
        "platform": std::env::consts::OS,
        "haptic_supported": haptic::SUPPORTED,
        // built-in clips a key or event can be given: switch press clips and fx
        "builtin": pack::SOUND_INDEX.iter().map(|(n, _, _)| *n)
            .filter(|n| n.contains("/press/") || n.starts_with("fx/")).collect::<Vec<_>>(),
    })
}

#[tauri::command]
fn set_option(app: AppState, h: AppHandle, key: String, value: Value) -> Result<(), String> {
    let pro_only = matches!(key.as_str(), "scroll_sensitivity" | "move_volume" | "haptic");
    if pro_only && !app.pro() {
        return Err(PRO_REQUIRED.into());
    }
    if key == "launch_at_login" {
        let on = value.as_bool().unwrap_or(false);
        let al = h.autolaunch();
        (if on { al.enable() } else { al.disable() }).map_err(|e| format!("Couldn't change launch at login: {e}"))?;
    }
    let f = || value.as_f64().map(|v| v as f32).ok_or("Expected a number.".to_string());
    let b = || value.as_bool().ok_or("Expected on or off.".to_string());
    app.change(false, |s| {
        match key.as_str() {
            "enabled" => s.enabled = b()?,
            "volume" => s.volume = f()?,
            "keyup_sound" => s.keyup_sound = b()?,
            "launch_at_login" => s.launch_at_login = b()?,
            "scroll_sensitivity" => s.scroll_sensitivity = f()?,
            "move_volume" => s.move_volume = f()?,
            "haptic" => s.haptic = b()?,
            "layout_override" => s.layout_override = value.as_str().filter(|v| ["ansi", "iso", "jis"].contains(v)).map(String::from),
            _ => return Err(format!("Unknown option {key}.")),
        }
        Ok(())
    })
}

#[tauri::command]
fn set_switch(app: AppState, id: String) -> Result<(), String> {
    set_switch_inner(&app, &id)
}

/// Assign a sound to events (keys or mouse/plug ids). `sound: null` resets to the default.
#[tauri::command]
fn set_sound(app: AppState, ids: Vec<String>, sound: Option<SoundRef>) -> Result<(), String> {
    if !app.pro() {
        return Err(PRO_REQUIRED.into());
    }
    if let Some(SoundRef::Builtin { builtin }) = &sound {
        if !pack::exists(builtin) {
            return Err("That sound isn't in Clacket.".into());
        }
    }
    app.change(true, |s| {
        let p = s.preset_mut();
        for id in ids {
            match &sound {
                Some(r) => { p.sounds.insert(id, r.clone()); }
                None => { p.sounds.remove(&id); }
            }
        }
        Ok(())
    })
}

#[tauri::command]
fn set_event_on(app: AppState, ids: Vec<String>, on: bool) -> Result<(), String> {
    if !app.pro() {
        return Err(PRO_REQUIRED.into());
    }
    app.change(true, |s| {
        for id in ids {
            if on { s.off.remove(&id); } else { s.off.insert(id); }
        }
        Ok(())
    })
}

/// Opens the picker without blocking: a blocking file dialog freezes the webview/main thread on
/// macOS (the app stops responding after Cancel). The callback does the import off the UI thread,
/// then the result reaches the window as `import-done` / `import-error` events.
#[tauri::command]
fn import_sound(app: AppState, h: AppHandle, ids: Vec<String>) -> Result<(), String> {
    if !app.pro() {
        return Err(PRO_REQUIRED.into());
    }
    let handle = h.clone();
    h.dialog().file().add_filter("Audio", &["mp3", "wav"]).pick_file(move |picked| {
        let Some(fp) = picked else { return }; // cancelled: do nothing, never block
        let app = handle.state::<Arc<App>>().inner().clone();
        let outcome = (|| -> Result<(), String> {
            let path = fp.into_path().map_err(|_| "Can't open that file.".to_string())?;
            audio::validate_file(&path)?;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("wav").to_ascii_lowercase();
            let name = format!("{}.{ext}", uuid::Uuid::new_v4());
            std::fs::create_dir_all(&app.sounds_dir).map_err(|e| e.to_string())?;
            std::fs::copy(&path, app.sounds_dir.join(&name)).map_err(|e| format!("Couldn't copy the file: {e}"))?;
            app.change(true, |s| {
                let p = s.preset_mut();
                for id in &ids {
                    p.sounds.insert(id.clone(), SoundRef::File { file: name.clone() });
                }
                Ok(())
            })
        })();
        match outcome {
            Ok(()) => {
                app.engine.event(ids[0].as_str(), true); // play the new sound once
                let _ = handle.emit("import-done", &ids);
            }
            Err(e) => {
                let _ = handle.emit("import-error", (&ids, e));
            }
        }
    });
    Ok(())
}

/// Plays a sound once: a SoundRef, or the current sound of an event id.
#[tauri::command]
fn preview(app: AppState, sound: Option<SoundRef>, event: Option<String>) -> Result<(), String> {
    let clip = match (sound, event) {
        (Some(SoundRef::Builtin { builtin }), _) => pack::clip(&builtin).ok_or("Unknown sound.")?.pipe(audio::decode)?,
        (Some(SoundRef::File { file }), _) => {
            if file.contains('/') || file.contains('\\') || file.contains("..") {
                return Err("Unknown sound.".into());
            }
            audio::decode(std::fs::read(app.sounds_dir.join(file)).map_err(|_| "That sound file is missing.".to_string())?)?
        }
        (None, Some(id)) => {
            let fx = FX_EVENTS.iter().find(|(e, _)| *e == id).map(|(_, d)| *d);
            match app.engine.bank.read().unwrap().pick(&id, true) {
                Some(c) => c,
                None => pack::clip(fx.ok_or("Nothing to play.")?).ok_or("Unknown sound.")?.pipe(audio::decode)?,
            }
        }
        _ => return Err("Nothing to play.".into()),
    };
    app.engine.play_clip(clip, false);
    Ok(())
}

trait Pipe: Sized { fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T { f(self) } }
impl<T> Pipe for T {}

#[tauri::command]
fn preset(app: AppState, action: String, name: String, new_name: Option<String>) -> Result<(), String> {
    if !app.pro() {
        return Err(PRO_REQUIRED.into());
    }
    let name = name.trim().to_string();
    app.change(true, |s| {
        let exists = |s: &Settings, n: &str| s.presets.iter().any(|p| p.name.eq_ignore_ascii_case(n));
        match action.as_str() {
            "activate" => s.active_preset = name,
            "save_as" => {
                if name.is_empty() || name.len() > 40 { return Err("Use a name of 1 to 40 characters.".into()); }
                if exists(s, &name) { return Err(format!("A preset called \"{name}\" already exists.")); }
                let mut p = s.preset().clone();
                p.name = name.clone();
                s.presets.push(p);
                s.active_preset = name;
            }
            "rename" => {
                let to = new_name.unwrap_or_default().trim().to_string();
                if name == "Default" { return Err("The Default preset can't be renamed.".into()); }
                if to.is_empty() || to.len() > 40 { return Err("Use a name of 1 to 40 characters.".into()); }
                if exists(s, &to) { return Err(format!("A preset called \"{to}\" already exists.")); }
                if let Some(p) = s.presets.iter_mut().find(|p| p.name == name) { p.name = to.clone(); }
                if s.active_preset == name { s.active_preset = to; }
            }
            "delete" => {
                if name == "Default" { return Err("The Default preset can't be deleted.".into()); }
                s.presets.retain(|p| p.name != name);
            }
            _ => return Err("Unknown preset action.".into()),
        }
        Ok(())
    })
}

#[tauri::command]
fn activate_license(app: AppState, key: String) -> Result<String, String> {
    let key = license::extract(&key).unwrap_or(key);
    match license::verify(&key) {
        Ok(l) => {
            app.s.lock().unwrap().license_key = Some(key);
            app.load_license();
            app.change(true, |_| Ok(()))?;
            Ok(l.name)
        }
        Err(license::LicenseError::Revoked) => Err("This key was refunded, so it can't turn on Pro.".into()),
        Err(_) => Err("This key isn't valid. Copy the whole key, starting with CLACKET-.".into()),
    }
}

#[tauri::command]
fn remove_license(app: AppState) -> Result<(), String> {
    app.s.lock().unwrap().license_key = None;
    app.load_license();
    app.change(true, |_| Ok(()))
}

/// Hands a fixed URL to the OS. Clacket itself makes no request.
#[tauri::command]
fn open_link(which: String) -> Result<(), String> {
    let url = match which.as_str() {
        "buy" => BUY_URL,
        "recover" => RECOVER_URL,
        #[cfg(target_os = "macos")]
        "permissions" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
        _ => return Err("Unknown link.".into()),
    };
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Shows or hides the macOS traffic-light buttons with a short fade.
#[cfg(target_os = "macos")]
fn set_window_controls(w: &tauri::WebviewWindow, visible: bool, animate: bool) {
    use objc2_app_kit::{NSAnimatablePropertyContainer, NSWindow, NSWindowButton};
    let Ok(ptr) = w.ns_window() else { return };
    let win: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
    for kind in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        if let Some(b) = win.standardWindowButton(kind) {
            let alpha = if visible { 1.0 } else { 0.0 };
            if animate { b.animator().setAlphaValue(alpha) } else { b.setAlphaValue(alpha) }
        }
    }
}

#[tauri::command]
fn window_controls(window: tauri::WebviewWindow, visible: bool) {
    #[cfg(target_os = "macos")]
    set_window_controls(&window, visible, true);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, visible);
}

#[tauri::command]
fn request_permission() {
    input::request_permission();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["CmdOrCtrl+Alt+K"]).expect("shortcut")
                .with_handler(|h, _, e| {
                    if e.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        on_menu(h, "on");
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_state, set_option, set_switch, set_sound, set_event_on, import_sound, preview,
            preset, activate_license, remove_license, open_link, request_permission, window_controls
        ])
        .setup(|tapp| {
            #[cfg(target_os = "macos")]
            tapp.set_activation_policy(tauri::ActivationPolicy::Accessory); // tray only, no Dock icon

            let h = tapp.handle().clone();
            let config_path = settings::file_path(&h.path().app_config_dir()?);
            let sounds_dir = audio::import_dir(&h.path().app_data_dir()?);
            let app = Arc::new(App {
                s: Mutex::new(Settings::load(&config_path)),
                config_path,
                sounds_dir,
                engine: Engine::start(),
                pro: AtomicBool::new(false),
                licensed_to: Mutex::new(None),
                license_note: Mutex::new(None),
                last_event: Mutex::new(String::new()),
                input_ok: AtomicBool::new(false),
                window_open: AtomicBool::new(false),
                scroll: Mutex::new(ScrollTicks::new()),
                speed: Mutex::new(MoveSpeed::new()),
                gesture: Mutex::new(GestureDebounce::new()),
                tray: Mutex::new(None),
                handle: h.clone(),
            });
            app.load_license();
            app.apply(true);
            tapp.manage(app.clone());

            let tray = TrayIconBuilder::with_id("clacket")
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Clacket")
                .menu(&tray_menu(&h, &app)?)
                .show_menu_on_left_click(true)
                .on_menu_event(|h, e| on_menu(h, e.id().as_ref()))
                .build(tapp)?;
            *app.tray.lock().unwrap() = Some(tray);

            let a = app.clone();
            app.input_ok.store(input::permitted() && input::start(move |ev| a.on_input(ev)), Relaxed);
            if !app.input_ok.load(Relaxed) || std::env::args().any(|a| a == "--settings") {
                open_settings(&h); // permission banner, or `clacket --settings` from a terminal
            }
            let a = app.clone();
            devices::start(move |ev| a.on_device(ev));

            // clacket://activate?key=... from the /thanks page
            use tauri_plugin_deep_link::DeepLinkExt;
            let a = app.clone();
            h.deep_link().on_open_url(move |e| {
                for url in e.urls() {
                    if let Some(key) = license::extract(url.as_str()) {
                        let r = activate_license(a.handle.state::<Arc<App>>(), key);
                        open_settings(&a.handle);
                        let _ = a.handle.emit("license-result", r.map_err(|e| e.to_string()));
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Clacket")
        .run(|_, e| {
            // closing Settings must not quit; only "Quit Clacket" (exit code set) does
            if let RunEvent::ExitRequested { api, code: None, .. } = e {
                api.prevent_exit();
            }
        });
}

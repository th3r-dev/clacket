//! Plug / unplug events, purely notification driven (no polling).
//! macOS: IOKit (USB interfaces), IOPS (charger), CoreGraphics (displays), CoreAudio (headphones),
//! DiskArbitration (SD cards). Windows/Linux: smaller sets, untested on this Mac.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceEvent {
    /// usb_storage | phone | input | audio | usb_other | charger | headphones | display | sd_card
    pub category: &'static str,
    /// "USB-C" | "USB-A" | "Thunderbolt" | "USB" | "" (not a port)
    pub port: &'static str,
    pub connected: bool,
    pub name: String,
}

impl DeviceEvent {
    pub fn describe(&self) -> String {
        let cat = match self.category {
            "usb_storage" => "Storage", "phone" => "Phone", "input" => "Keyboard/mouse", "audio" => "Audio",
            "usb_other" => "USB device", "charger" => "Charger", "headphones" => "Headphones",
            "display" => "Display", "sd_card" => "SD card", c => c,
        };
        let mut parts = vec![];
        if !self.port.is_empty() { parts.push(self.port.to_string()); }
        parts.push(cat.to_string());
        if !self.name.is_empty() { parts.push(self.name.clone()); }
        parts.push((if self.connected { "Connected" } else { "Disconnected" }).to_string());
        parts.join(" · ")
    }
}

/// USB interface class (and vendor/name) -> category.
pub fn usb_category(iface_class: i64, vendor: i64, name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    match iface_class {
        8 => "usb_storage",
        3 => "input",
        1 => "audio",
        6 => "phone", // still image / PTP: phones and cameras
        _ if vendor == 0x05ac && (n.contains("iphone") || n.contains("ipad")) => "phone",
        _ if n.contains("android") || n.contains("pixel") || n.contains("galaxy") => "phone",
        _ => "usb_other",
    }
}

/// Port from what the OS reports: a connector type if present, else ancestor names in the registry.
pub fn port_from(connector: Option<i64>, ancestors: &[String]) -> &'static str {
    match connector {
        Some(0) => return "USB-A",
        Some(9) | Some(10) => return "USB-C",
        _ => {}
    }
    let has = |s: &str| ancestors.iter().any(|a| a.contains(s));
    if has("Thunderbolt") {
        "Thunderbolt"
    } else if has("TypeC") || ancestors.iter().any(|a| a.starts_with("usb-drd")) {
        "USB-C" // Apple Silicon dual-role ports are USB-C
    } else {
        "USB"
    }
}

/// Drops the startup enumeration burst and repeats from composite devices.
pub struct Gate {
    start: Instant,
    seen: HashMap<String, Instant>,
}

impl Gate {
    pub fn new(now: Instant) -> Self {
        Self { start: now, seen: HashMap::new() }
    }

    /// key: a stable id for the physical device + direction.
    pub fn pass(&mut self, key: &str, now: Instant) -> bool {
        if now.duration_since(self.start) < Duration::from_secs(2) {
            return false;
        }
        let fresh = self.seen.get(key).is_none_or(|t| now.duration_since(*t) >= Duration::from_millis(500));
        self.seen.insert(key.to_string(), now);
        self.seen.retain(|_, t| now.duration_since(*t) < Duration::from_secs(10));
        fresh
    }
}

#[cfg(target_os = "macos")]
pub use mac::start;
#[cfg(target_os = "windows")]
pub use win::start;
#[cfg(target_os = "linux")]
pub use linux::start;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use core_foundation::{
        base::{CFType, TCFType},
        boolean::CFBoolean,
        dictionary::CFDictionary,
        number::CFNumber,
        runloop::{kCFRunLoopDefaultMode, CFRunLoop, CFRunLoopSource},
        string::CFString,
    };
    use std::{
        ffi::{c_char, c_void, CStr},
        sync::Mutex,
    };

    type Cb = Box<dyn Fn(DeviceEvent) + Send + Sync>;
    struct Ctx {
        cb: Cb,
        gate: Mutex<Gate>,
        /// interface registry id -> (device id, category, port, name), remembered for the unplug
        known: Mutex<HashMap<u64, (u64, &'static str, &'static str, String)>>,
        on_ac: Mutex<Option<bool>>,
        output_external: Mutex<Option<bool>>,
        headphones: Mutex<Option<bool>>,
    }

    impl Ctx {
        fn emit(&self, key: String, ev: DeviceEvent) {
            if self.gate.lock().unwrap().pass(&key, Instant::now()) {
                (self.cb)(ev);
            }
        }
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IONotificationPortCreate(main_port: u32) -> *mut c_void;
        fn IONotificationPortGetRunLoopSource(p: *mut c_void) -> *mut c_void;
        fn IOServiceMatching(name: *const c_char) -> *mut c_void;
        fn IOServiceAddMatchingNotification(p: *mut c_void, ty: *const c_char, matching: *mut c_void,
            cb: extern "C" fn(*mut c_void, u32), refcon: *mut c_void, it: *mut u32) -> i32;
        fn IOIteratorNext(it: u32) -> u32;
        fn IOObjectRelease(o: u32) -> i32;
        fn IORegistryEntryGetRegistryEntryID(e: u32, id: *mut u64) -> i32;
        fn IORegistryEntryGetParentEntry(e: u32, plane: *const c_char, parent: *mut u32) -> i32;
        fn IORegistryEntryGetName(e: u32, name: *mut c_char) -> i32;
        fn IOObjectGetClass(e: u32, name: *mut c_char) -> i32;
        fn IORegistryEntrySearchCFProperty(e: u32, plane: *const c_char, key: *const c_void, alloc: *const c_void, opts: u32) -> *const c_void;
        fn IOPSNotificationCreateRunLoopSource(cb: extern "C" fn(*mut c_void), ctx: *mut c_void) -> *mut c_void;
        fn IOPSCopyPowerSourcesInfo() -> *const c_void;
        fn IOPSGetProvidingPowerSourceType(snapshot: *const c_void) -> *const c_void;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGDisplayRegisterReconfigurationCallback(cb: extern "C" fn(u32, u32, *mut c_void), ctx: *mut c_void) -> i32;
    }
    #[repr(C)]
    struct Addr { selector: u32, scope: u32, element: u32 }
    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectAddPropertyListener(obj: u32, a: *const Addr,
            cb: extern "C" fn(u32, u32, *const Addr, *mut c_void) -> i32, ctx: *mut c_void) -> i32;
        fn AudioObjectGetPropertyData(obj: u32, a: *const Addr, qsize: u32, q: *const c_void, size: *mut u32, out: *mut c_void) -> i32;
    }
    #[link(name = "DiskArbitration", kind = "framework")]
    extern "C" {
        fn DASessionCreate(alloc: *const c_void) -> *mut c_void;
        fn DASessionScheduleWithRunLoop(s: *mut c_void, rl: *const c_void, mode: *const c_void);
        fn DARegisterDiskAppearedCallback(s: *mut c_void, m: *const c_void, cb: extern "C" fn(*mut c_void, *mut c_void), ctx: *mut c_void);
        fn DARegisterDiskDisappearedCallback(s: *mut c_void, m: *const c_void, cb: extern "C" fn(*mut c_void, *mut c_void), ctx: *mut c_void);
        fn DADiskCopyDescription(d: *mut c_void) -> *const c_void;
    }

    const PLANE: &CStr = c"IOService";
    const RECURSIVE_PARENTS: u32 = 1 | 2;
    const fn cc(s: &[u8; 4]) -> u32 { u32::from_be_bytes(*s) }

    fn prop(e: u32, key: &str) -> Option<CFType> {
        let k = CFString::new(key);
        let p = unsafe { IORegistryEntrySearchCFProperty(e, PLANE.as_ptr(), k.as_concrete_TypeRef() as _, std::ptr::null(), 0) };
        (!p.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(p as _) })
    }
    fn prop_up(e: u32, key: &str) -> Option<CFType> {
        let k = CFString::new(key);
        let p = unsafe { IORegistryEntrySearchCFProperty(e, PLANE.as_ptr(), k.as_concrete_TypeRef() as _, std::ptr::null(), RECURSIVE_PARENTS) };
        (!p.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(p as _) })
    }
    fn num(t: Option<CFType>) -> Option<i64> {
        t?.downcast::<CFNumber>()?.to_i64()
    }
    fn string(t: Option<CFType>) -> String {
        t.and_then(|t| t.downcast::<CFString>()).map(|s| s.to_string()).unwrap_or_default()
    }
    fn entry_id(e: u32) -> u64 {
        let mut id = 0;
        unsafe { IORegistryEntryGetRegistryEntryID(e, &mut id) };
        id
    }
    /// Class and entry names of every ancestor, nearest first.
    fn ancestors(e: u32) -> (Vec<String>, u64) {
        let (mut out, mut cur, mut device) = (vec![], e, 0u64);
        let mut buf = [0 as c_char; 128];
        for depth in 0..24 {
            let mut parent = 0;
            if unsafe { IORegistryEntryGetParentEntry(cur, PLANE.as_ptr(), &mut parent) } != 0 {
                break;
            }
            if depth == 0 {
                device = entry_id(parent); // the USB device that owns this interface
            }
            unsafe {
                IOObjectGetClass(parent, buf.as_mut_ptr());
                out.push(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned());
                IORegistryEntryGetName(parent, buf.as_mut_ptr());
                out.push(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned());
            }
            if cur != e {
                unsafe { IOObjectRelease(cur) };
            }
            cur = parent;
        }
        if cur != e {
            unsafe { IOObjectRelease(cur) };
        }
        (out, device)
    }

    extern "C" fn usb_added(refcon: *mut c_void, it: u32) {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        loop {
            let e = unsafe { IOIteratorNext(it) };
            if e == 0 { break; }
            let class = num(prop(e, "bInterfaceClass")).unwrap_or(-1);
            let vendor = num(prop_up(e, "idVendor")).unwrap_or(0);
            let name = string(prop_up(e, "USB Product Name"));
            let (anc, device) = ancestors(e);
            let port = port_from(num(prop_up(e, "UsbConnector")), &anc);
            let cat = usb_category(class, vendor, &name);
            ctx.known.lock().unwrap().insert(entry_id(e), (device, cat, port, name.clone()));
            // one sound per device, not per interface: the gate dedupes on the device id
            ctx.emit(format!("usb:{device}:in"), DeviceEvent { category: cat, port, connected: true, name });
            unsafe { IOObjectRelease(e) };
        }
    }

    extern "C" fn usb_removed(refcon: *mut c_void, it: u32) {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        loop {
            let e = unsafe { IOIteratorNext(it) };
            if e == 0 { break; }
            if let Some((device, category, port, name)) = ctx.known.lock().unwrap().remove(&entry_id(e)) {
                ctx.emit(format!("usb:{device}:out"), DeviceEvent { category, port, connected: false, name });
            }
            unsafe { IOObjectRelease(e) };
        }
    }

    extern "C" fn power_changed(refcon: *mut c_void) {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        let ac = unsafe {
            let info = IOPSCopyPowerSourcesInfo();
            if info.is_null() { return; }
            let ty = IOPSGetProvidingPowerSourceType(info);
            let ac = !ty.is_null() && CFString::wrap_under_get_rule(ty as _).to_string() == "AC Power";
            core_foundation::base::CFRelease(info);
            ac
        };
        let mut last = ctx.on_ac.lock().unwrap();
        if last.replace(ac).is_some_and(|was| was != ac) {
            ctx.emit(format!("charger:{ac}"), DeviceEvent { category: "charger", port: "", connected: ac, name: String::new() });
        }
    }

    extern "C" fn display_changed(_display: u32, flags: u32, refcon: *mut c_void) {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        const ADD: u32 = 1 << 4;
        const REMOVE: u32 = 1 << 5;
        if flags & (ADD | REMOVE) != 0 {
            let connected = flags & ADD != 0;
            ctx.emit(format!("display:{connected}"), DeviceEvent { category: "display", port: "", connected, name: String::new() });
        }
    }

    fn get_u32(obj: u32, selector: u32, scope: u32) -> Option<u32> {
        let a = Addr { selector, scope, element: 0 };
        let (mut v, mut size) = (0u32, 4u32);
        (unsafe { AudioObjectGetPropertyData(obj, &a, 0, std::ptr::null(), &mut size, &mut v as *mut u32 as _) } == 0).then_some(v)
    }

    const SYSTEM: u32 = 1;
    const GLOBAL: u32 = cc(b"glob");
    const OUTPUT: u32 = cc(b"outp");

    /// Default output moved to a non-built-in device (BT/USB headphones), or the built-in
    /// output switched to its headphone jack.
    fn audio_check(ctx: &Ctx) {
        let Some(dev) = get_u32(SYSTEM, cc(b"dOut"), GLOBAL) else { return };
        let external = get_u32(dev, cc(b"tran"), GLOBAL).is_some_and(|t| t != cc(b"bltn"));
        let jack = !external && get_u32(dev, cc(b"ssrc"), OUTPUT) == Some(cc(b"hdpn"));
        let on = external || jack;
        let mut last = ctx.headphones.lock().unwrap();
        if last.replace(on).is_some_and(|was| was != on) {
            ctx.emit(format!("headphones:{on}"), DeviceEvent { category: "headphones", port: "", connected: on, name: String::new() });
        }
        *ctx.output_external.lock().unwrap() = Some(external);
    }

    extern "C" fn audio_changed(_obj: u32, _n: u32, _a: *const Addr, refcon: *mut c_void) -> i32 {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        audio_check(ctx);
        // the built-in device's jack listener is added once; re-adding on every change is harmless
        if let Some(dev) = get_u32(SYSTEM, cc(b"dOut"), GLOBAL) {
            let a = Addr { selector: cc(b"ssrc"), scope: OUTPUT, element: 0 };
            unsafe { AudioObjectAddPropertyListener(dev, &a, audio_changed, refcon) };
        }
        0
    }

    fn sd_event(disk: *mut c_void, refcon: *mut c_void, connected: bool) {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        let desc = unsafe { DADiskCopyDescription(disk) };
        if desc.is_null() { return; }
        let d: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(desc as _) };
        let get = |k: &str| d.find(CFString::new(k)).map(|v| v.clone());
        let whole = get("DAMediaWhole").and_then(|v| v.downcast::<CFBoolean>()).is_some_and(bool::from);
        let proto = string(get("DADeviceProtocol"));
        let name = string(get("DAMediaName"));
        // USB drives already sound through IOKit; only the SD slot is handled here
        if whole && proto == "Secure Digital" {
            ctx.emit(format!("sd:{name}:{connected}"), DeviceEvent { category: "sd_card", port: "", connected, name: String::new() });
        }
    }
    extern "C" fn disk_appeared(disk: *mut c_void, refcon: *mut c_void) { sd_event(disk, refcon, true) }
    extern "C" fn disk_gone(disk: *mut c_void, refcon: *mut c_void) { sd_event(disk, refcon, false) }

    pub fn start(cb: impl Fn(DeviceEvent) + Send + Sync + 'static) {
        std::thread::Builder::new().name("clacket-devices".into()).spawn(move || unsafe {
            let ctx: &'static Ctx = Box::leak(Box::new(Ctx {
                cb: Box::new(cb),
                gate: Mutex::new(Gate::new(Instant::now())),
                known: Mutex::default(),
                on_ac: Mutex::new(None),
                output_external: Mutex::new(None),
                headphones: Mutex::new(None),
            }));
            let refcon = ctx as *const Ctx as *mut c_void;
            let rl = CFRunLoop::get_current();

            let port = IONotificationPortCreate(0);
            let src = CFRunLoopSource::wrap_under_get_rule(IONotificationPortGetRunLoopSource(port) as _);
            rl.add_source(&src, kCFRunLoopDefaultMode);
            let (mut it_add, mut it_rm) = (0u32, 0u32);
            // each call consumes one reference to its matching dictionary
            IOServiceAddMatchingNotification(port, c"IOServiceFirstMatch".as_ptr(), IOServiceMatching(c"IOUSBHostInterface".as_ptr()), usb_added, refcon, &mut it_add);
            IOServiceAddMatchingNotification(port, c"IOServiceTerminate".as_ptr(), IOServiceMatching(c"IOUSBHostInterface".as_ptr()), usb_removed, refcon, &mut it_rm);
            // drain once to arm; existing devices are recorded, and the gate silences startup
            usb_added(refcon, it_add);
            usb_removed(refcon, it_rm);

            let ps = IOPSNotificationCreateRunLoopSource(power_changed, refcon);
            if !ps.is_null() {
                rl.add_source(&CFRunLoopSource::wrap_under_create_rule(ps as _), kCFRunLoopDefaultMode);
            }
            power_changed(refcon);

            CGDisplayRegisterReconfigurationCallback(display_changed, refcon);

            let dout = Addr { selector: cc(b"dOut"), scope: GLOBAL, element: 0 };
            AudioObjectAddPropertyListener(SYSTEM, &dout, audio_changed, refcon);
            audio_changed(0, 0, std::ptr::null(), refcon);

            let da = DASessionCreate(std::ptr::null());
            if !da.is_null() {
                DARegisterDiskAppearedCallback(da, std::ptr::null(), disk_appeared, refcon);
                DARegisterDiskDisappearedCallback(da, std::ptr::null(), disk_gone, refcon);
                DASessionScheduleWithRunLoop(da, rl.as_concrete_TypeRef() as _, kCFRunLoopDefaultMode as _);
            }
            CFRunLoop::run_current();
        }).ok();
    }
}

#[cfg(target_os = "linux")]
mod linux {
    //! Kernel uevents over a netlink socket: no libudev needed.
    use super::*;

    pub fn start(cb: impl Fn(DeviceEvent) + Send + Sync + 'static) {
        std::thread::Builder::new().name("clacket-devices".into()).spawn(move || unsafe {
            let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_DGRAM, libc::NETLINK_KOBJECT_UEVENT);
            if fd < 0 { return; }
            let mut addr: libc::sockaddr_nl = std::mem::zeroed();
            addr.nl_family = libc::AF_NETLINK as u16;
            addr.nl_groups = 1;
            if libc::bind(fd, &addr as *const _ as *const libc::sockaddr, std::mem::size_of_val(&addr) as u32) < 0 { return; }
            let mut gate = Gate::new(Instant::now());
            let mut buf = vec![0u8; 8192];
            loop {
                let n = libc::recv(fd, buf.as_mut_ptr() as _, buf.len(), 0);
                if n <= 0 { continue; }
                let kv: HashMap<&str, &str> = buf[..n as usize].split(|b| *b == 0)
                    .filter_map(|l| std::str::from_utf8(l).ok()?.split_once('='))
                    .collect();
                let action = kv.get("ACTION").copied().unwrap_or("");
                let connected = match action { "add" => true, "remove" => false, "change" => true, _ => continue };
                let sub = kv.get("SUBSYSTEM").copied().unwrap_or("");
                let devpath = kv.get("DEVPATH").copied().unwrap_or("");
                let ev = match sub {
                    "usb" if kv.get("DEVTYPE") == Some(&"usb_interface") && action != "change" => {
                        let class = kv.get("INTERFACE").and_then(|i| i.split('/').next()?.parse().ok()).unwrap_or(-1);
                        // ponytail: generic "USB"; map ports via /sys/class/typec/*/ partner links when asked
                        let port = "USB";
                        let device = devpath.rsplit_once(':').map(|(d, _)| d).unwrap_or(devpath);
                        Some((format!("usb:{device}:{connected}"), DeviceEvent { category: usb_category(class, 0, ""), port, connected, name: String::new() }))
                    }
                    "power_supply" if action == "change" && kv.get("POWER_SUPPLY_TYPE") == Some(&"Mains") => {
                        let on = kv.get("POWER_SUPPLY_ONLINE") == Some(&"1");
                        Some((format!("charger:{on}"), DeviceEvent { category: "charger", port: "", connected: on, name: String::new() }))
                    }
                    "drm" if kv.get("HOTPLUG") == Some(&"1") => {
                        Some(("display".into(), DeviceEvent { category: "display", port: "", connected: true, name: String::new() }))
                    }
                    "block" if kv.get("DEVTYPE") == Some(&"disk") && devpath.contains("mmc") && action != "change" => {
                        Some((format!("sd:{connected}"), DeviceEvent { category: "sd_card", port: "", connected, name: String::new() }))
                    }
                    _ => None,
                };
                if let Some((key, ev)) = ev {
                    if gate.pass(&key, Instant::now()) { cb(ev); }
                }
            }
        }).ok();
    }
}

#[cfg(target_os = "windows")]
mod win {
    //! Message-only window: WM_DEVICECHANGE (USB), WM_POWERBROADCAST (charger), WM_DISPLAYCHANGE.
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use windows_sys::Win32::{System::Power::*, UI::WindowsAndMessaging::*};

    type Cb = Box<dyn Fn(DeviceEvent) + Send + Sync>;
    static CB: OnceLock<Cb> = OnceLock::new();
    static GATE: Mutex<Option<Gate>> = Mutex::new(None);
    static AC: Mutex<Option<bool>> = Mutex::new(None);

    fn emit(key: String, ev: DeviceEvent) {
        let mut g = GATE.lock().unwrap();
        if g.get_or_insert_with(|| Gate::new(Instant::now())).pass(&key, Instant::now()) {
            if let Some(cb) = CB.get() { cb(ev); }
        }
    }

    unsafe extern "system" fn proc_(h: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> isize {
        const DBT_DEVICEARRIVAL: usize = 0x8000;
        const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;
        match msg {
            WM_DEVICECHANGE if w == DBT_DEVICEARRIVAL || w == DBT_DEVICEREMOVECOMPLETE => {
                let connected = w == DBT_DEVICEARRIVAL;
                // ponytail: no device class or connector on Windows yet; IOCTL_USB_GET_PORT_CONNECTOR_PROPERTIES when asked
                emit(format!("usb:{connected}"), DeviceEvent { category: "usb_other", port: "USB", connected, name: String::new() });
            }
            WM_POWERBROADCAST => {
                let mut s = std::mem::zeroed::<SYSTEM_POWER_STATUS>();
                if GetSystemPowerStatus(&mut s) != 0 {
                    let on = s.ACLineStatus == 1;
                    let mut last = AC.lock().unwrap();
                    if last.replace(on).is_some_and(|was| was != on) {
                        emit(format!("charger:{on}"), DeviceEvent { category: "charger", port: "", connected: on, name: String::new() });
                    }
                }
            }
            WM_DISPLAYCHANGE => emit("display".into(), DeviceEvent { category: "display", port: "", connected: true, name: String::new() }),
            _ => {}
        }
        DefWindowProcW(h, msg, w, l)
    }

    pub fn start(cb: impl Fn(DeviceEvent) + Send + Sync + 'static) {
        if CB.set(Box::new(cb)).is_err() { return; }
        std::thread::Builder::new().name("clacket-devices".into()).spawn(|| unsafe {
            let class: Vec<u16> = "ClacketDevices\0".encode_utf16().collect();
            let wc = WNDCLASSW { lpfnWndProc: Some(proc_), lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
            RegisterClassW(&wc);
            // a hidden top-level window: WM_DEVICECHANGE and WM_DISPLAYCHANGE are broadcast to these
            CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
            let mut msg = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                DispatchMessageW(&msg);
            }
        }).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(usb_category(8, 0, "SanDisk"), "usb_storage");
        assert_eq!(usb_category(3, 0, "Magic Keyboard"), "input");
        assert_eq!(usb_category(1, 0, "DAC"), "audio");
        assert_eq!(usb_category(6, 0, ""), "phone");
        assert_eq!(usb_category(255, 0x05ac, "iPhone"), "phone");
        assert_eq!(usb_category(9, 0, "Hub"), "usb_other");
    }

    #[test]
    fn ports() {
        assert_eq!(port_from(Some(0), &[]), "USB-A");
        assert_eq!(port_from(Some(9), &[]), "USB-C");
        assert_eq!(port_from(None, &["IOThunderboltPort".into()]), "Thunderbolt");
        assert_eq!(port_from(None, &["AppleUSB20XHCIARMPort".into(), "usb-drd0-port-hs".into()]), "USB-C");
        assert_eq!(port_from(None, &["AppleUSB20HubPort".into()]), "USB");
    }

    #[test]
    fn gate_ignores_startup_and_dedupes() {
        let t0 = Instant::now();
        let mut g = Gate::new(t0);
        assert!(!g.pass("usb:1:in", t0 + Duration::from_millis(500))); // startup burst
        let t = t0 + Duration::from_secs(3);
        assert!(g.pass("usb:1:in", t));
        assert!(!g.pass("usb:1:in", t + Duration::from_millis(100))); // second interface, same device
        assert!(g.pass("usb:1:out", t + Duration::from_millis(200)));
        assert!(g.pass("usb:1:in", t + Duration::from_secs(1)));
    }

    #[test]
    fn describe_reads_well() {
        let e = DeviceEvent { category: "usb_storage", port: "USB-C", connected: true, name: "SanDisk".into() };
        assert_eq!(e.describe(), "USB-C · Storage · SanDisk · Connected");
    }
}

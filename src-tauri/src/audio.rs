//! Decodes the active preset once, then each event is a map lookup + mixer.add.
use crate::{
    pack,
    settings::{Settings, SoundRef, FREE_SWITCHES, FX_EVENTS, DEFAULT_SWITCH},
};
use rodio::{mixer::Mixer, Decoder, Source};
use rodio::buffer::SamplesBuffer;
use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

const MAX_VOICES: usize = 16;
pub const MAX_CLIP_SECS: f32 = 2.0;
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

pub fn decode(bytes: Vec<u8>) -> Result<SamplesBuffer, String> {
    let d = Decoder::new(Cursor::new(bytes)).map_err(|_| "This isn't an MP3 or WAV file.".to_string())?;
    let (ch, sr) = (d.channels(), d.sample_rate());
    let samples: Vec<f32> = d.collect();
    if samples.is_empty() {
        return Err("This file has no audio in it.".into());
    }
    Ok(SamplesBuffer::new(ch, sr, samples))
}

fn secs(b: &SamplesBuffer) -> f32 {
    b.total_duration().map(|d| d.as_secs_f32()).unwrap_or(0.0)
}

/// Checks a user file before it's copied in. Returns the decoded clip.
pub fn validate_file(path: &Path) -> Result<SamplesBuffer, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext != "mp3" && ext != "wav" {
        return Err("Use an MP3 or WAV file.".into());
    }
    let size = std::fs::metadata(path).map_err(|e| format!("Can't read this file: {e}"))?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!("This file is {:.1} MB. Use a file under 5 MB.", size as f64 / 1_048_576.0));
    }
    let clip = decode(std::fs::read(path).map_err(|e| format!("Can't read this file: {e}"))?)?;
    let s = secs(&clip);
    if s > MAX_CLIP_SECS {
        return Err(format!("This file is {s:.1} s long. Use a clip under 2 seconds."));
    }
    Ok(clip)
}

/// Clips resolved for the active preset.
#[derive(Default)]
pub struct Bank {
    /// "press:GENERIC" holds R0..R4; "press:SPACE", "release:GENERIC", ...
    switch: HashMap<String, Vec<SamplesBuffer>>,
    /// event id -> clip (per-key overrides + mouse/plug sounds)
    events: HashMap<String, SamplesBuffer>,
    pub move_loop: Option<SamplesBuffer>,
}

pub fn key_kind(code: &str) -> &'static str {
    match code {
        "Space" => "SPACE",
        "Enter" | "NumpadEnter" => "ENTER",
        "Backspace" => "BACKSPACE",
        _ => "GENERIC",
    }
}

impl Bank {
    pub fn build(s: &Settings, pro: bool, sounds_dir: &Path) -> Self {
        let p = s.preset();
        let sw = if pro || FREE_SWITCHES.contains(&p.base_switch.as_str()) { p.base_switch.as_str() } else { DEFAULT_SWITCH };
        let mut b = Bank::default();
        let load = |name: &str| pack::clip(name).and_then(|x| decode(x).ok());
        let generic: Vec<_> = (0..5).filter_map(|i| load(&format!("{sw}/press/GENERIC_R{i}"))).collect();
        b.switch.insert("press:GENERIC".into(), generic);
        for k in ["SPACE", "ENTER", "BACKSPACE"] {
            if let Some(c) = load(&format!("{sw}/press/{k}")) {
                b.switch.insert(format!("press:{k}"), vec![c]);
            }
        }
        for k in ["GENERIC", "SPACE", "ENTER", "BACKSPACE"] {
            if let Some(c) = load(&format!("{sw}/release/{k}")) {
                b.switch.insert(format!("release:{k}"), vec![c]);
            }
        }
        if !pro {
            return b;
        }
        // dedupe: the same SoundRef used on many keys is decoded once
        let mut cache: HashMap<SoundRef, Option<SamplesBuffer>> = HashMap::new();
        let mut resolve = |r: &SoundRef| {
            cache
                .entry(r.clone())
                .or_insert_with(|| match r {
                    SoundRef::Builtin { builtin } => load(builtin),
                    SoundRef::File { file } => std::fs::read(sounds_dir.join(file)).ok().and_then(|x| decode(x).ok()),
                })
                .clone()
        };
        for (id, r) in &p.sounds {
            if let Some(c) = resolve(r) {
                b.events.insert(id.clone(), c);
            }
        }
        for (id, default) in FX_EVENTS {
            if s.off.contains(*id) {
                b.events.remove(*id);
                continue;
            }
            if !b.events.contains_key(*id) {
                if let Some(c) = resolve(&SoundRef::Builtin { builtin: default.to_string() }) {
                    b.events.insert(id.to_string(), c);
                }
            }
        }
        b.move_loop = b.events.remove("move");
        b
    }

    /// The clip for an event. Key events: per-key override > switch clip for the key kind > GENERIC.
    pub fn pick(&self, id: &str, down: bool) -> Option<SamplesBuffer> {
        if let Some(code) = id.strip_prefix("key:") {
            if down {
                if let Some(c) = self.events.get(id) {
                    return Some(c.clone());
                }
            }
            let phase = if down { "press" } else { "release" };
            let v = self.switch.get(&format!("{phase}:{}", key_kind(code)))
                .or_else(|| self.switch.get(&format!("{phase}:GENERIC")))?;
            return (!v.is_empty()).then(|| v[fastrand::usize(..v.len())].clone());
        }
        self.events.get(id).cloned()
    }
}

/// Owns the output device and the decoded bank.
pub struct Engine {
    mixer: Mutex<Option<Mixer>>,
    pub bank: RwLock<Bank>,
    pub volume: AtomicU32,
    pub enabled: AtomicBool,
    pub keyup: AtomicBool,
    voices: Arc<AtomicUsize>,
    reopen: std::sync::mpsc::Sender<()>,
    move_gain: Arc<AtomicU32>,
    move_on: Arc<AtomicBool>,
    pub move_volume: AtomicU32,
}

impl Engine {
    /// Opens the default output on its own thread (the stream handle isn't Send on every
    /// platform). Sending on `reopen` reopens it, e.g. after the default device changes.
    pub fn start() -> Arc<Engine> {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let e = Arc::new(Engine {
            mixer: Mutex::new(None),
            bank: RwLock::new(Bank::default()),
            volume: AtomicU32::new(0.7f32.to_bits()),
            enabled: AtomicBool::new(true),
            keyup: AtomicBool::new(true),
            voices: Arc::new(AtomicUsize::new(0)),
            reopen: tx,
            move_gain: Arc::new(AtomicU32::new(0)),
            move_on: Arc::new(AtomicBool::new(false)),
            move_volume: AtomicU32::new(0.3f32.to_bits()),
        });
        let weak = Arc::downgrade(&e);
        std::thread::Builder::new()
            .name("clacket-audio".into())
            .spawn(move || {
                let mut _stream = None;
                loop {
                    let Some(e) = weak.upgrade() else { return };
                    let tx = e.reopen.clone();
                    // ponytail: the stream runs (silent) while idle; drop it after idle if CPU shows up
                    match rodio::OutputStreamBuilder::from_default_device()
                        .and_then(|b| b.with_error_callback(move |_| { let _ = tx.send(()); }).open_stream_or_fallback())
                    {
                        Ok(mut s) => {
                            s.log_on_drop(false);
                            *e.mixer.lock().unwrap() = Some(s.mixer().clone());
                            _stream = Some(s);
                        }
                        Err(_) => *e.mixer.lock().unwrap() = None,
                    }
                    drop(e);
                    // wait for a reopen request; coalesce bursts
                    if rx.recv().is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(300));
                    while rx.try_recv().is_ok() {}
                    _stream = None;
                }
            })
            .unwrap();
        e
    }

    pub fn reopen(&self) {
        let _ = self.reopen.send(());
    }

    pub fn has_output(&self) -> bool {
        self.mixer.lock().unwrap().is_some()
    }

    fn vol(&self) -> f32 {
        f32::from_bits(self.volume.load(Relaxed))
    }

    pub fn play_clip(&self, clip: SamplesBuffer, jitter: bool) {
        if self.voices.load(Relaxed) >= MAX_VOICES {
            return; // ponytail: drop new voices over the cap instead of stealing the oldest
        }
        let Some(m) = self.mixer.lock().unwrap().clone() else { return };
        let (speed, gain) = if jitter {
            (1.0 + (fastrand::f32() - 0.5) * 0.06, 1.0 + (fastrand::f32() - 0.5) * 0.16)
        } else {
            (1.0, 1.0)
        };
        self.voices.fetch_add(1, Relaxed);
        let src = clip.speed(speed).amplify(self.vol() * gain);
        m.add(rodio::source::Done::new(src, self.voices.clone()));
    }

    /// Plays the sound for an event id, if there is one and Clacket is on.
    pub fn event(&self, id: &str, down: bool) {
        if !self.enabled.load(Relaxed) || (!down && !self.keyup.load(Relaxed)) {
            return;
        }
        let clip = self.bank.read().unwrap().pick(id, down);
        if let Some(c) = clip {
            self.play_clip(c, id.starts_with("key:"));
        }
    }

    /// Pointer movement: `speed` in points/second. Starts a quiet loop whose gain follows speed;
    /// the loop stops itself after 300 ms of stillness so a still pointer costs nothing.
    pub fn movement(&self, speed: f32) {
        if !self.enabled.load(Relaxed) {
            return;
        }
        let target = (speed / 3000.0).min(1.0) * f32::from_bits(self.move_volume.load(Relaxed)) * self.vol();
        self.move_gain.fetch_max(target.to_bits(), Relaxed); // positive f32 bits order like the values
        if self.move_on.swap(true, Relaxed) {
            return;
        }
        let Some(clip) = self.bank.read().unwrap().move_loop.clone() else {
            self.move_on.store(false, Relaxed);
            return;
        };
        let Some(m) = self.mixer.lock().unwrap().clone() else {
            self.move_on.store(false, Relaxed);
            return;
        };
        let (gain_in, on) = (self.move_gain.clone(), self.move_on.clone());
        let mut current = 0f32;
        let mut still_ms = 0u32;
        let src = clip.repeat_infinite().amplify(0.0).stoppable().periodic_access(Duration::from_millis(16), move |s| {
            let target = f32::from_bits(gain_in.swap(0, Relaxed));
            current = target.max(current * 0.85);
            s.inner_mut().set_factor(current);
            still_ms = if current < 0.002 { still_ms + 16 } else { 0 };
            if still_ms > 300 {
                s.stop();
                on.store(false, Relaxed);
            }
        });
        m.add(src);
    }
}

pub fn import_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sounds")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Preset, SWITCHES};

    #[test]
    fn every_switch_decodes() {
        for (sw, _, _) in SWITCHES {
            let n = pack::SOUND_INDEX.iter().filter(|(n, _, _)| n.starts_with(&format!("{sw}/"))).count();
            assert!(n >= 6, "{sw} has {n} clips");
            for (name, _, _) in pack::SOUND_INDEX.iter().filter(|(n, _, _)| n.starts_with(&format!("{sw}/"))) {
                let c = decode(pack::clip(name).unwrap()).unwrap();
                let peak = c.clone().fold(0f32, |m, x| m.max(x.abs()));
                assert!(peak > 0.0 && peak <= 1.0, "{name} peak {peak}");
            }
        }
    }

    #[test]
    fn resolver_order() {
        let mut s = Settings::default();
        s.presets[0] = Preset {
            name: "Default".into(),
            base_switch: "holypanda".into(),
            sounds: [("key:KeyQ".to_string(), SoundRef::Builtin { builtin: "fx/tap".into() })].into(),
        };
        let dir = std::env::temp_dir();
        let pro = Bank::build(&s, true, &dir);
        let tap = decode(pack::clip("fx/tap").unwrap()).unwrap();
        let q = pro.pick("key:KeyQ", true).unwrap();
        assert_eq!(q.total_duration(), tap.total_duration()); // override wins
        let space = decode(pack::clip("holypanda/press/SPACE").unwrap()).unwrap();
        assert_eq!(pro.pick("key:Space", true).unwrap().total_duration(), space.total_duration());
        assert!(pro.pick("key:KeyA", true).is_some()); // GENERIC
        assert!(pro.pick("key:KeyA", false).is_some()); // release

        // free: override ignored, locked switch falls back to cream
        let free = Bank::build(&s, false, &dir);
        let cream_space = decode(pack::clip("cream/press/SPACE").unwrap()).unwrap();
        assert_eq!(free.pick("key:Space", true).unwrap().total_duration(), cream_space.total_duration());
        assert_ne!(free.pick("key:KeyQ", true).unwrap().total_duration(), tap.total_duration());
        assert!(free.pick("mouse:left", true).is_none());
    }

    #[test]
    fn mx_blue_without_space_clip_falls_back_to_generic() {
        let mut s = Settings::default();
        s.presets[0].base_switch = "mxblue".into();
        let b = Bank::build(&s, false, &std::env::temp_dir());
        assert!(b.pick("key:Space", true).is_some());
        assert!(b.pick("key:Enter", false).is_some());
    }

    #[test]
    fn rejects_bad_files() {
        let dir = std::env::temp_dir().join("clacket-test");
        std::fs::create_dir_all(&dir).unwrap();
        let txt = dir.join("a.txt");
        std::fs::write(&txt, "hi").unwrap();
        assert!(validate_file(&txt).unwrap_err().contains("MP3 or WAV"));
        let fake = dir.join("a.mp3");
        std::fs::write(&fake, "not audio").unwrap();
        assert!(validate_file(&fake).is_err());
        let big = dir.join("big.wav");
        std::fs::write(&big, vec![0u8; (MAX_FILE_BYTES + 1) as usize]).unwrap();
        assert!(validate_file(&big).unwrap_err().contains("under 5 MB"));
        // the 2 s movement loop is at the limit; a 3 s wav must be refused
        let long = dir.join("long.wav");
        write_wav(&long, 44100 * 3);
        assert!(validate_file(&long).unwrap_err().contains("under 2 seconds"));
        let ok = dir.join("ok.wav");
        write_wav(&ok, 4410);
        assert!(validate_file(&ok).is_ok());
    }

    fn write_wav(p: &Path, n: u32) {
        let mut b = Vec::new();
        let data = n * 2;
        b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(1u16.to_le_bytes());
        b.extend(44100u32.to_le_bytes()); b.extend(88200u32.to_le_bytes()); b.extend(2u16.to_le_bytes()); b.extend(16u16.to_le_bytes());
        b.extend(b"data"); b.extend(data.to_le_bytes());
        for i in 0..n { b.extend((((i % 100) as i16 - 50) * 300).to_le_bytes()); }
        std::fs::write(p, b).unwrap();
    }
}

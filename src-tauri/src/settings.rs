use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Where a sound comes from: a clip in the built-in pack, or a file the user imported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(untagged)]
pub enum SoundRef {
    /// e.g. { "builtin": "holypanda/press/SPACE" } or { "builtin": "fx/tap" }
    Builtin { builtin: String },
    /// file name inside <app data>/sounds/
    File { file: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub base_switch: String,
    /// event id ("key:Space", "mouse:left", "scroll", "plug:usb_storage:in", ...) -> sound
    #[serde(default)]
    pub sounds: BTreeMap<String, SoundRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub volume: f32,
    pub keyup_sound: bool,
    pub launch_at_login: bool,
    pub layout_override: Option<String>,
    pub active_preset: String,
    pub presets: Vec<Preset>,
    /// event ids switched off; mouse/plug ids are off until the user turns them on
    pub off: BTreeSet<String>,
    pub scroll_sensitivity: f32,
    pub move_volume: f32,
    pub haptic: bool,
    pub license_key: Option<String>,
}

pub const DEFAULT_SWITCH: &str = "cream";
pub const FREE_SWITCHES: &[&str] = &["cream", "mxbrown", "mxblue", "topre"];

/// (id, label, group). Order is the order shown in menus.
pub const SWITCHES: &[(&str, &str, &str)] = &[
    ("alpaca", "Alpaca", "Linear"),
    ("blackink", "Black Ink", "Linear"),
    ("redink", "Red Ink", "Linear"),
    ("cream", "NK Cream", "Linear"),
    ("mxblack", "MX Black", "Linear"),
    ("holypanda", "Holy Panda", "Tactile"),
    ("mxbrown", "MX Brown", "Tactile"),
    ("turquoise", "Turquoise", "Tactile"),
    ("topre", "Topre", "Tactile"),
    ("mxblue", "MX Blue", "Clicky"),
    ("boxnavy", "Box Navy", "Clicky"),
    ("bluealps", "Blue Alps", "Clicky"),
    ("buckling", "Buckling Spring", "Clicky"),
];

/// Mouse, gesture and plug events with their default built-in sound.
pub const FX_EVENTS: &[(&str, &str)] = &[
    ("mouse:left", "fx/mouse_down"),
    ("mouse:right", "fx/mouse_down"),
    ("mouse:middle", "fx/mouse_down"),
    ("mouse:up", "fx/mouse_up"),
    ("scroll", "fx/scroll"),
    ("move", "fx/move"),
    ("tap", "fx/tap"),
    ("tap2", "fx/tap2"),
    ("swipe", "fx/swipe"),
    ("pinch", "fx/pinch"),
    ("plug:usb_storage:in", "fx/usb_storage_in"),
    ("plug:usb_storage:out", "fx/usb_storage_out"),
    ("plug:phone:in", "fx/phone_in"),
    ("plug:phone:out", "fx/phone_out"),
    ("plug:input:in", "fx/input_in"),
    ("plug:input:out", "fx/input_out"),
    ("plug:audio:in", "fx/audio_in"),
    ("plug:audio:out", "fx/audio_out"),
    ("plug:usb_other:in", "fx/usb_other_in"),
    ("plug:usb_other:out", "fx/usb_other_out"),
    ("plug:charger:in", "fx/charger_in"),
    ("plug:charger:out", "fx/charger_out"),
    ("plug:headphones:in", "fx/headphones_in"),
    ("plug:headphones:out", "fx/headphones_out"),
    ("plug:display:in", "fx/display_in"),
    ("plug:display:out", "fx/display_out"),
    ("plug:sd_card:in", "fx/sd_card_in"),
    ("plug:sd_card:out", "fx/sd_card_out"),
];

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.7,
            keyup_sound: true,
            launch_at_login: false,
            layout_override: None,
            active_preset: "Default".into(),
            presets: vec![Preset { name: "Default".into(), base_switch: DEFAULT_SWITCH.into(), sounds: BTreeMap::new() }],
            off: FX_EVENTS.iter().map(|(id, _)| id.to_string()).collect(),
            scroll_sensitivity: 0.5,
            move_volume: 0.3,
            haptic: false,
            license_key: None,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut s: Settings = std::fs::read(path)
            .ok()
            .and_then(|b| toon_format::decode_default(&String::from_utf8_lossy(&b)).ok())
            .unwrap_or_default();
        s.repair();
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // write then rename, so a crash mid-write never leaves a half file
        let tmp = path.with_extension("toon.tmp");
        let text = toon_format::encode_default(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    /// Keeps the invariants: a "Default" preset exists, the active preset exists, values in range.
    pub fn repair(&mut self) {
        if !self.presets.iter().any(|p| p.name == "Default") {
            self.presets.insert(0, Preset { name: "Default".into(), base_switch: DEFAULT_SWITCH.into(), sounds: BTreeMap::new() });
        }
        if !self.presets.iter().any(|p| p.name == self.active_preset) {
            self.active_preset = "Default".into();
        }
        for p in &mut self.presets {
            if !SWITCHES.iter().any(|(id, _, _)| *id == p.base_switch) {
                p.base_switch = DEFAULT_SWITCH.into();
            }
        }
        self.volume = self.volume.clamp(0.0, 1.0);
        self.move_volume = self.move_volume.clamp(0.0, 1.0);
        self.scroll_sensitivity = self.scroll_sensitivity.clamp(0.0, 1.0);
    }

    pub fn preset(&self) -> &Preset {
        self.presets.iter().find(|p| p.name == self.active_preset).unwrap()
    }

    pub fn preset_mut(&mut self) -> &mut Preset {
        let name = self.active_preset.clone();
        self.presets.iter_mut().find(|p| p.name == name).unwrap()
    }
}

pub fn file_path(dir: &Path) -> PathBuf {
    dir.join("settings.toon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_repair() {
        let mut s = Settings::default();
        s.presets.push(Preset {
            name: "Mixed".into(),
            base_switch: "holypanda".into(),
            sounds: [
                ("key:Space".to_string(), SoundRef::Builtin { builtin: "holypanda/press/SPACE".into() }),
                ("key:KeyQ".to_string(), SoundRef::File { file: "abc.wav".into() }),
            ]
            .into(),
        });
        s.active_preset = "Mixed".into();
        let text = toon_format::encode_default(&s).unwrap();
        let back: Settings = toon_format::decode_default(&text).unwrap();
        assert_eq!(back, s);
        // the saved file survives a disk round trip
        let p = std::env::temp_dir().join("clacket-settings-test.toon");
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);

        let mut broken: Settings = serde_json::from_str(r#"{"active_preset":"gone","presets":[],"volume":9}"#).unwrap();
        broken.repair();
        assert_eq!(broken.active_preset, "Default");
        assert_eq!(broken.preset().base_switch, DEFAULT_SWITCH);
        assert_eq!(broken.volume, 1.0);
    }
}

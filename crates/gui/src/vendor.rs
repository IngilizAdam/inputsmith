//! Read-only access to data from a local Options+ installation: localized
//! strings, the action-card catalog, per-control tags and recommendations,
//! device renders and button hotspot coordinates.
//!
//! Nothing here is bundled with inputsmith; without an installation the UI
//! falls back to firmware names and a built-in action list.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use iced::widget::image::Handle;
use inputsmith_ipc::model::Card;
use serde::Deserialize;

/// The default ("All applications") profile id the vendor data is keyed by.
const GLOBAL_KEY: &str = "420fd454-0c36-499d-bde4-146823b16147";

#[derive(Debug, Clone, Default)]
pub struct Hotspot {
    pub slot_id: String,
    pub name_key: String,
    /// Marker position in percent of the image size.
    pub x: f32,
    pub y: f32,
    /// Label direction: -1 left, 1 right, 0 centred; `dy` likewise vertically.
    pub dx: f32,
    pub dy: f32,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceArt {
    pub front: Option<PathBuf>,
    pub buttons: Option<PathBuf>,
    pub point_scroll: Option<PathBuf>,
    pub button_hotspots: Vec<Hotspot>,
    pub point_scroll_hotspots: Vec<Hotspot>,
}

#[derive(Default)]
pub struct Vendor {
    strings: HashMap<String, String>,
    pub cards: Vec<Card>,
    control_tags: HashMap<u16, Vec<String>>,
    recommendations: HashMap<u16, Vec<String>>,
    /// modelId -> depot name.
    depots: HashMap<String, String>,
    depot_root: Option<PathBuf>,
}

fn install_data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let pf = std::env::var_os("ProgramFiles").map(PathBuf::from)?;
        Some(pf.join("LogiOptionsPlus").join("data"))
    } else {
        None
    }
}

fn program_data() -> Option<PathBuf> {
    if cfg!(windows) { std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join("LogiOptionsPlus")) } else { None }
}

fn read_json(p: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Minimal `KEY: value` YAML reader for the vendor string tables.
fn parse_strings(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with(' ') {
            continue;
        }
        let Some((k, v)) = line.split_once(": ") else { continue };
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        out.insert(k.trim().to_string(), v.replace("\\n", "\n").replace("''", "'"));
    }
    out
}

fn info_lists(v: &serde_json::Value) -> HashMap<u16, Vec<String>> {
    let mut out = HashMap::new();
    if let Some(list) = v.pointer(&format!("/data/{GLOBAL_KEY}/infoList")).and_then(|l| l.as_array()) {
        for e in list {
            let (Some(id), Some(infos)) = (e["id"].as_str().and_then(|s| s.parse().ok()), e["infos"].as_array()) else {
                continue;
            };
            out.insert(id, infos.iter().filter_map(|i| i.as_str().map(str::to_string)).collect());
        }
    }
    out
}

impl Vendor {
    pub fn get() -> &'static Vendor {
        static V: OnceLock<Vendor> = OnceLock::new();
        V.get_or_init(Vendor::load)
    }

    fn load() -> Self {
        let mut v = Vendor::default();
        if let Some(data) = install_data_dir() {
            if let Ok(t) = std::fs::read_to_string(data.join("strings").join("en-US.yaml")) {
                v.strings = parse_strings(&t);
            }
            for f in ["card_presets/card_presets_win.json", "macros/common_application_cards_win.json"] {
                if let Some(cards) = read_json(&data.join(f)).and_then(|j| j.get("cards").cloned()) {
                    for c in cards.as_array().into_iter().flatten() {
                        match serde_json::from_value::<Card>(c.clone()) {
                            Ok(card) => v.cards.push(card),
                            Err(e) => log::debug!("skipping card {}: {e}", c["id"]),
                        }
                    }
                }
            }
            if let Some(j) = read_json(&data.join("rap/available_card_tags_control.json")) {
                v.control_tags = info_lists(&j);
            }
            if let Some(j) = read_json(&data.join("rap/recommendations_control_win.json")) {
                v.recommendations = info_lists(&j);
            }
        }
        if let Some(pd) = program_data() {
            if let Some(db) = read_json(&pd.join("devices.json")) {
                for d in db["devices"].as_array().into_iter().flatten() {
                    if let (Some(m), Some(dep)) = (d["modelId"].as_str(), d["depot"].as_str()) {
                        v.depots.insert(m.to_string(), dep.to_string());
                    }
                }
            }
            // Newest build directory under depots/.
            v.depot_root = std::fs::read_dir(pd.join("depots"))
                .ok()
                .and_then(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).max());
        }
        if v.cards.is_empty() {
            v.cards = builtin_cards();
        }
        log::info!("vendor data: {} strings, {} cards, {} depots", v.strings.len(), v.cards.len(), v.depots.len());
        v
    }

    /// Localized string for a vendor key, or a humanized key.
    pub fn text(&self, key: &str) -> String {
        if let Some(s) = self.strings.get(key) {
            return s.clone();
        }
        let base = key
            .trim_start_matches("ASSIGNMENT_NAME_")
            .trim_start_matches("SLOT_NAME_")
            .trim_end_matches("_BUTTON")
            .replace('_', " ")
            .to_lowercase();
        let mut c = base.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }

    pub fn card(&self, id: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.id == id)
    }

    pub fn recommended(&self, cid: u16) -> Vec<&Card> {
        self.recommendations.get(&cid).into_iter().flatten().filter_map(|id| self.card(id)).collect()
    }

    /// Cards allowed on a control, grouped into the picker's categories.
    pub fn categories(&self, cid: u16) -> Vec<(&'static str, Vec<&Card>)> {
        let allowed: Vec<&str> =
            self.control_tags.get(&cid).map(|v| v.iter().map(String::as_str).collect()).unwrap_or_default();
        let ok = |c: &&Card| {
            !c.tags.iter().any(|t| t == "PRESET_TAG_MACROS_UNSUPPORTED")
                && (allowed.is_empty() || c.tags.iter().any(|t| allowed.contains(&t.as_str())))
        };
        let groups: [(&str, &[&str]); 5] = [
            ("Mouse", &["PRESET_TAG_MOUSE", "PRESET_MOUSE_FUNCTIONS", "PRESET_TAG_MOUSE_SMARTSHIFT"]),
            ("Computer", &["PRESET_COMPUTER_FUNCTIONS"]),
            ("Media", &["PRESET_TAG_MEDIA"]),
            ("Keyboard", &["PRESET_KEYBOARD_FUNCTIONS", "PRESET_TAG_KEY_OR_BUTTON"]),
            ("Gestures", &["PRESET_TAG_GESTURE"]),
        ];
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (name, tags) in groups {
            let cards: Vec<&Card> = self
                .cards
                .iter()
                .filter(ok)
                .filter(|c| c.tags.iter().any(|t| tags.contains(&t.as_str())))
                .filter(|c| seen.insert(c.id.clone()))
                .collect();
            if !cards.is_empty() {
                out.push((name, cards));
            }
        }
        out
    }

    /// Renders and hotspots for a model, preferring the colour variant.
    pub fn art(&self, model_id: &str, extended: u8) -> DeviceArt {
        let (Some(root), Some(depot)) = (&self.depot_root, self.depots.get(model_id)) else {
            return DeviceArt::default();
        };
        let dir = root.join(depot);
        let Some(manifest) = read_json(&dir.join("manifest.json")) else { return DeviceArt::default() };
        let wanted = if extended > 0 { format!("{model_id}_ext{extended}") } else { model_id.to_string() };
        let entries = manifest["devices"].as_array().cloned().unwrap_or_default();
        let entry = entries
            .iter()
            .find(|d| d["modelId"] == wanted.as_str())
            .or_else(|| entries.iter().find(|d| d["modelId"] == model_id));
        let Some(entry) = entry else { return DeviceArt::default() };
        let res = |key: &str| {
            entry["resources"]
                .as_array()?
                .iter()
                .find(|r| r["key"] == key)
                .and_then(|r| r["src"].as_str())
                .map(|s| dir.join(s))
        };
        let mut art = DeviceArt {
            front: res("device_image"),
            buttons: res("device_buttons_image"),
            point_scroll: res("device_point_scroll_image"),
            ..Default::default()
        };
        if let Some(meta) = res("image_metadata").and_then(|p| read_json(&p)) {
            for img in meta["images"].as_array().into_iter().flatten() {
                let spots: Vec<Hotspot> = img["assignments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| Hotspot {
                        slot_id: a["slotId"].as_str().unwrap_or_default().to_string(),
                        name_key: a["slotName"].as_str().unwrap_or_default().to_string(),
                        x: a["marker"]["x"].as_f64().unwrap_or(0.0) as f32,
                        y: a["marker"]["y"].as_f64().unwrap_or(0.0) as f32,
                        dx: a["label"]["x"].as_f64().unwrap_or(1.0) as f32,
                        dy: a["label"]["y"].as_f64().unwrap_or(0.0) as f32,
                    })
                    .collect();
                match img["key"].as_str() {
                    Some("device_buttons_image") => art.button_hotspots = spots,
                    Some("device_point_scroll_image") => art.point_scroll_hotspots = spots,
                    _ => {}
                }
            }
        }
        art
    }
}

/// Decodes an image file (WebP/PNG) for iced, cached by path.
pub fn image(path: &Path) -> Option<(Handle, u32, u32)> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<PathBuf, (Handle, u32, u32)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(h) = cache.lock().unwrap().get(path) {
        return Some(h.clone());
    }
    let img = ::image::open(path).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    let v = (Handle::from_rgba(w, h, img.into_raw()), w, h);
    cache.lock().unwrap().insert(path.to_path_buf(), v.clone());
    Some(v)
}

/// A small action list used when no Options+ installation is present.
fn builtin_cards() -> Vec<Card> {
    #[derive(Deserialize)]
    struct Wrap {
        cards: Vec<Card>,
    }
    let json = serde_json::json!({ "cards": [
        {"id": "builtin_middle", "name": "Middle button", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MOUSE"],
         "macro": {"type": "MOUSE", "mouse": {"action": "BUTTON", "hidUsage": 3}}},
        {"id": "builtin_back", "name": "Back", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MOUSE"],
         "macro": {"type": "MOUSE", "mouse": {"action": "WIN_BACK"}}},
        {"id": "builtin_forward", "name": "Forward", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MOUSE"],
         "macro": {"type": "MOUSE", "mouse": {"action": "WIN_FORWARD"}}},
        {"id": "builtin_copy", "name": "Copy", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_COMPUTER_FUNCTIONS"],
         "macro": {"type": "KEYSTROKE", "keystroke": {"code": 6, "modifiers": [224]}}},
        {"id": "builtin_paste", "name": "Paste", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_COMPUTER_FUNCTIONS"],
         "macro": {"type": "KEYSTROKE", "keystroke": {"code": 25, "modifiers": [224]}}},
        {"id": "builtin_undo", "name": "Undo", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_COMPUTER_FUNCTIONS"],
         "macro": {"type": "KEYSTROKE", "keystroke": {"code": 29, "modifiers": [224]}}},
        {"id": "builtin_play_pause", "name": "Play / pause", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MEDIA"],
         "macro": {"type": "MEDIA", "media": {"usage": "PLAY_PAUSE"}}},
        {"id": "builtin_volume_up", "name": "Volume up", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MEDIA"],
         "macro": {"type": "MEDIA", "media": {"usage": "VOLUME_UP"}}},
        {"id": "builtin_volume_down", "name": "Volume down", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MEDIA"],
         "macro": {"type": "MEDIA", "media": {"usage": "VOLUME_DOWN"}}},
        {"id": "builtin_do_nothing", "name": "Do nothing", "attribute": "MACRO_PLAYBACK", "tags": ["PRESET_TAG_MOUSE"],
         "macro": {"type": "DO_NOTHING", "doNothing": {"action": "NONE"}}}
    ]});
    serde_json::from_value::<Wrap>(json).map(|w| w.cards).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_vendor_strings() {
        let s = super::parse_strings("# c\nA_B: Hello\nC: \"Quoted: yes\"\n  nested: no\n");
        assert_eq!(s["A_B"], "Hello");
        assert_eq!(s["C"], "Quoted: yes");
        assert!(!s.contains_key("nested"));
    }

    #[test]
    fn builtin_cards_parse() {
        assert_eq!(super::builtin_cards().len(), 10);
    }
}

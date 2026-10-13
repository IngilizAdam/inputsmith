//! Inputsmith's own data, compiled into the programs: device descriptions
//! (`devices/*.json`), device art (`art/*.svg`), the action catalog
//! (`actions.json`), per-control picker data (`controls.json`) and English UI
//! text (`strings.json`). Values that Logi Options+ also has (slot prefixes,
//! scroll defaults, card definitions, recommendations) match it exactly so
//! settings stay interchangeable. See `README.md` in this crate.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use inputsmith_ipc::model::{Card, ScrollWheelSettings, ThumbWheelSettings};
use serde::Deserialize;

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// One device model.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub model_id: String,
    pub display_name: String,
    /// Names of colour/region variants, keyed by extended model id.
    #[serde(default)]
    pub extended_display_names: BTreeMap<u8, String>,
    /// Prefix of the device's settings slots (`<prefix>_c82`, `<prefix>_mouse_settings`).
    pub slot_prefix: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Scroll wheel settings used until the user changes them.
    #[serde(default)]
    pub scroll_defaults: Option<ScrollWheelSettings>,
    /// Thumb wheel settings used until the user changes them.
    #[serde(default)]
    pub thumb_wheel_defaults: Option<ThumbWheelSettings>,
    /// DPI ranges of a high-resolution sensor (normal and high-resolution mode).
    #[serde(default)]
    pub high_resolution_sensor: Option<HighResolutionSensor>,
    /// Control ids that can be re-assigned.
    #[serde(default)]
    pub controls: Vec<u16>,
    /// Card id per control id for controls whose default is not their own function (e.g. the
    /// gesture button, which does nothing by itself).
    #[serde(default)]
    pub defaults: BTreeMap<u16, String>,
    /// File name in `art/`.
    #[serde(default)]
    pub art: Option<String>,
    /// Hotspots on the "Buttons" page.
    #[serde(default)]
    pub buttons: Vec<Hotspot>,
    /// Hotspots on the "Point & scroll" page.
    #[serde(default)]
    pub point_scroll: Vec<Hotspot>,
}

/// DPI ranges of a high-resolution sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighResolutionSensor {
    /// Range while the high-resolution mode is off.
    pub off: DpiRange,
    /// Range while it is on.
    pub on: DpiRange,
    #[serde(default)]
    pub default_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct DpiRange {
    pub min: u16,
    pub max: u16,
    pub default: u16,
    pub step: u16,
}

/// A marker on the device art.
#[derive(Debug, Clone, Deserialize)]
pub struct Hotspot {
    /// Slot suffix: `c82` becomes `<slot prefix>_c82`.
    pub slot: String,
    /// UI text key of the slot.
    pub name: String,
    /// Marker position in percent of the art's width and height.
    pub x: f32,
    pub y: f32,
    /// Callout direction: -1 left, 1 right, 0 centred; `dy` likewise vertically.
    #[serde(default)]
    pub dx: f32,
    #[serde(default)]
    pub dy: f32,
}

/// Action picker data for one control id.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlData {
    /// Card ids shown first, in order.
    #[serde(default)]
    pub recommended: Vec<String>,
    /// Card tags allowed on the control.
    #[serde(default)]
    pub allowed_tags: Vec<String>,
}

pub fn devices() -> &'static [DeviceData] {
    static D: OnceLock<Vec<DeviceData>> = OnceLock::new();
    D.get_or_init(|| {
        DEVICE_FILES
            .iter()
            .filter_map(|(name, json)| {
                serde_json::from_str(json).map_err(|e| log::error!("devices/{name}: {e}")).ok()
            })
            .collect()
    })
}

pub fn device(model_id: &str) -> Option<&'static DeviceData> {
    devices().iter().find(|d| d.model_id == model_id)
}

/// The card a control uses until the user assigns one, if it differs from the control's own function.
pub fn default_card(model_id: &str, cid: u16) -> Option<&'static Card> {
    let id = device(model_id)?.defaults.get(&cid)?;
    cards().iter().find(|c| &c.id == id)
}

/// An SVG from `art/`.
pub fn art(name: &str) -> Option<&'static [u8]> {
    ART_FILES.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// The action catalog.
pub fn cards() -> &'static [Card] {
    #[derive(Deserialize)]
    struct File {
        cards: Vec<Card>,
    }
    static C: OnceLock<Vec<Card>> = OnceLock::new();
    C.get_or_init(|| {
        serde_json::from_str::<File>(include_str!("../actions.json"))
            .map(|f| f.cards)
            .map_err(|e| log::error!("actions.json: {e}"))
            .unwrap_or_default()
    })
}

pub fn control(cid: u16) -> Option<&'static ControlData> {
    #[derive(Deserialize)]
    struct File {
        controls: HashMap<u16, ControlData>,
    }
    static C: OnceLock<HashMap<u16, ControlData>> = OnceLock::new();
    C.get_or_init(|| {
        serde_json::from_str::<File>(include_str!("../controls.json"))
            .map(|f| f.controls)
            .map_err(|e| log::error!("controls.json: {e}"))
            .unwrap_or_default()
    })
    .get(&cid)
}

/// English text for a UI key such as `SLOT_NAME_BACK_BUTTON`.
pub fn text(key: &str) -> Option<&'static str> {
    static S: OnceLock<HashMap<String, String>> = OnceLock::new();
    S.get_or_init(|| {
        serde_json::from_str(include_str!("../strings.json"))
            .map_err(|e| log::error!("strings.json: {e}"))
            .unwrap_or_default()
    })
    .get(key)
    .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn names(card: &Card, out: &mut Vec<String>) {
        out.push(card.name.clone());
        for n in card.nested_cards.values() {
            names(n, out);
        }
    }

    #[test]
    fn every_file_parses() {
        assert_eq!(devices().len(), DEVICE_FILES.len());
        assert!(!cards().is_empty());
        assert!(control(82).is_some());
        assert!(text("SLOT_NAME_BACK_BUTTON").is_some());
    }

    #[test]
    fn devices_are_consistent() {
        let mut ids = HashSet::new();
        for d in devices() {
            assert!(ids.insert(&d.model_id), "duplicate model {}", d.model_id);
            assert!(d.slot_prefix.ends_with(&d.model_id), "{}: slot prefix", d.model_id);
            if let Some(a) = &d.art {
                assert!(art(a).is_some(), "{}: missing art/{a}", d.model_id);
                // A picture must show every control, or some would not be reachable.
                for cid in &d.controls {
                    let slot = format!("c{cid}");
                    assert!(d.buttons.iter().any(|h| h.slot == slot), "{}: no hotspot for {cid}", d.model_id);
                }
            }
            for (cid, card) in &d.defaults {
                assert!(d.controls.contains(cid), "{}: default for unknown control {cid}", d.model_id);
                assert!(cards().iter().any(|c| &c.id == card), "{}: unknown default card {card}", d.model_id);
            }
            for h in d.buttons.iter().chain(&d.point_scroll) {
                assert!(text(&h.name).is_some(), "{}: no text for {}", d.model_id, h.name);
                assert!((0.0..=100.0).contains(&h.x) && (0.0..=100.0).contains(&h.y));
            }
        }
    }

    #[test]
    fn catalog_is_consistent() {
        let mut ids = HashSet::new();
        for c in cards() {
            assert!(ids.insert(&c.id), "duplicate card {}", c.id);
            let mut n = Vec::new();
            names(c, &mut n);
            for key in n.iter().filter(|k| !k.is_empty()) {
                assert!(text(key).is_some(), "{}: no text for {key}", c.id);
            }
        }
    }

    #[test]
    fn art_has_a_view_box() {
        for (name, svg) in ART_FILES {
            assert!(std::str::from_utf8(svg).unwrap().contains("viewBox="), "{name}");
        }
    }
}

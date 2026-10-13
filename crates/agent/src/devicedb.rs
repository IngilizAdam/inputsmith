//! Device metadata from Inputsmith's own device descriptions
//! (`crates/data/devices`): display names, the slot prefixes profiles are
//! keyed by and default scroll settings. Devices without a description get
//! their name from firmware and a slot prefix from [`fallback_slot_prefix`].

use std::collections::HashMap;

use inputsmith_data::DeviceData;
use inputsmith_ipc::model::{Card, ScrollWheelSettings};

#[derive(Debug, Default)]
pub struct DeviceDb;

impl DeviceDb {
    pub fn load() -> Self {
        log::info!("{} device descriptions", inputsmith_data::devices().len());
        Self
    }

    pub fn get(&self, model_id: &str) -> Option<&'static DeviceData> {
        inputsmith_data::device(model_id)
    }

    /// Slot prefix for a device: the described one when known, else derived.
    pub fn slot_prefix(&self, model_id: &str, firmware_name: &str) -> String {
        self.get(model_id)
            .map(|d| d.slot_prefix.clone())
            .unwrap_or_else(|| fallback_slot_prefix(firmware_name, model_id))
    }

    /// Cards for controls whose default is not their own function, by control id.
    pub fn default_cards(&self, model_id: &str) -> HashMap<u16, Card> {
        let Some(d) = self.get(model_id) else { return HashMap::new() };
        d.defaults.keys().filter_map(|&cid| Some((cid, inputsmith_data::default_card(model_id, cid)?.clone()))).collect()
    }

    /// The model's default scroll wheel settings.
    pub fn scroll_defaults(&self, model_id: &str) -> Option<ScrollWheelSettings> {
        self.get(model_id)?.scroll_defaults.clone()
    }

    /// The variant's name (e.g. "… for Mac") if it has one, else the model's, else the firmware's.
    pub fn display_name(&self, model_id: &str, extended_model_id: u8, firmware_name: &str) -> String {
        self.get(model_id)
            .map(|d| d.extended_display_names.get(&extended_model_id).unwrap_or(&d.display_name).clone())
            .unwrap_or_else(|| firmware_name.to_string())
    }
}

/// `MX Anywhere 3` + `6b025` -> `mx-anywhere-3-6b025` (matches Options+'s
/// prefix for about half of all models).
pub fn fallback_slot_prefix(name: &str, model_id: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    format!("{out}-{model_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mx_anywhere_3_matches_options_plus() {
        let db = DeviceDb;
        assert_eq!(db.slot_prefix("6b025", "ignored"), "mx-anywhere-3-6b025");
        assert_eq!(db.display_name("6b025", 1, "ignored"), "MX Anywhere 3");
        assert_eq!(db.display_name("6b025", 4, "ignored"), "MX Anywhere 3 for Mac");
        assert_eq!(db.display_name("4082", 0, "MX Master 3"), "MX Master 3");
        let s = db.scroll_defaults("6b025").unwrap();
        let ss = s.smartshift.unwrap();
        assert!(ss.is_enabled && ss.is_scroll_force_enabled);
        assert_eq!((ss.sensitivity, ss.scroll_force), (83, 50));
        let keys = s.virtual_thumbwheel.unwrap().keys;
        assert_eq!(keys.iter().filter(|k| k.enabled).map(|k| k.control_id).collect::<Vec<_>>(), [83, 86]);
        assert!(db.scroll_defaults("4082").is_none());
        assert_eq!(db.slot_prefix("4082", "MX Master 3"), "mx-master-3-4082");
        assert!(db.default_cards("6b025").is_empty());
    }

    #[test]
    fn gesture_button_defaults_to_gestures() {
        let db = DeviceDb;
        assert_eq!(db.slot_prefix("6b023", "ignored"), "mx-master-3-6b023");
        let cards = db.default_cards("6b023");
        assert_eq!(cards[&195].id, "card_global_presets_one_of_gesture_button");
        assert_eq!(cards.len(), 1);
    }

    #[test]
    fn fallback_prefix() {
        assert_eq!(super::fallback_slot_prefix("MX Anywhere 3", "6b025"), "mx-anywhere-3-6b025");
        assert_eq!(super::fallback_slot_prefix("M720 Triathlon", "4055"), "m720-triathlon-4055");
    }
}

//! Optional device metadata from a local Options+ installation.
//!
//! The vendor's merged device database (`%PROGRAMDATA%\LogiOptionsPlus\devices.json`)
//! provides display names and the curated slot prefixes profiles are keyed by.
//! It is read at runtime only; without it names come from firmware and slot
//! prefixes from [`fallback_slot_prefix`].

use std::collections::HashMap;
use std::path::PathBuf;

use inputsmith_ipc::model::ScrollWheelSettings;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbDevice {
    pub model_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub slot_prefix: String,
    #[serde(default)]
    pub depot: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

#[derive(Debug, Default)]
pub struct DeviceDb {
    by_model: HashMap<String, DbDevice>,
}

#[derive(Deserialize)]
struct DbFile {
    devices: Vec<DbDevice>,
}

impl DeviceDb {
    pub fn vendor_path() -> Option<PathBuf> {
        if cfg!(windows) {
            std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join("LogiOptionsPlus").join("devices.json"))
        } else if cfg!(target_os = "macos") {
            Some(PathBuf::from("/Library/Application Support/Logitech.localized/LogiOptionsPlus/devices.json"))
        } else {
            None
        }
    }

    /// Loads the vendor database if present; empty otherwise.
    pub fn load() -> Self {
        let Some(path) = Self::vendor_path() else { return Self::default() };
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<DbFile>(&b).map_err(|e| e.to_string()))
        {
            Ok(f) => {
                log::info!("loaded {} device definitions from {}", f.devices.len(), path.display());
                Self { by_model: f.devices.into_iter().map(|d| (d.model_id.clone(), d)).collect() }
            }
            Err(e) => {
                log::debug!("no vendor device database ({}): {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn get(&self, model_id: &str) -> Option<&DbDevice> {
        self.by_model.get(model_id)
    }

    /// Slot prefix for a device: the vendor's when known, else derived.
    pub fn slot_prefix(&self, model_id: &str, firmware_name: &str) -> String {
        self.get(model_id)
            .map(|d| d.slot_prefix.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| fallback_slot_prefix(firmware_name, model_id))
    }

    /// The model's default scroll wheel settings (`capabilities.mouseScrollWheelOverride`).
    pub fn scroll_defaults(&self, model_id: &str) -> Option<ScrollWheelSettings> {
        let v = self.get(model_id)?.capabilities.get("mouseScrollWheelOverride")?;
        serde_json::from_value(v.clone()).map_err(|e| log::debug!("{model_id}: bad mouseScrollWheelOverride: {e}")).ok()
    }

    pub fn display_name(&self, model_id: &str, firmware_name: &str) -> String {
        self.get(model_id)
            .map(|d| d.display_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| firmware_name.to_string())
    }
}

/// `MX Anywhere 3` + `6b025` -> `mx-anywhere-3-6b025` (matches the vendor's
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
    fn reads_scroll_defaults() {
        // As found in the vendor database for the MX Anywhere 3 (note the snake_case inner keys).
        let caps = serde_json::json!({"mouseScrollWheelOverride": {"dir": "STANDARD", "isSmooth": false,
            "smartshift": {"is_enabled": true, "is_scroll_force_enabled": true, "mode": "RATCHET",
                "scroll_force": 50, "sensitivity": 83},
            "speed": 0, "virtualThumbwheel": {"keys": [{"controlId": 83, "enabled": true, "stateChange": "DEFAULT"},
                {"controlId": 86, "enabled": true, "stateChange": "DEFAULT"}]}}});
        let dev = DbDevice {
            model_id: "6b025".into(),
            display_name: String::new(),
            slot_prefix: String::new(),
            depot: String::new(),
            kind: String::new(),
            capabilities: caps,
        };
        let db = DeviceDb { by_model: [("6b025".to_string(), dev)].into() };
        let s = db.scroll_defaults("6b025").unwrap();
        let ss = s.smartshift.unwrap();
        assert!(ss.is_enabled && ss.is_scroll_force_enabled);
        assert_eq!((ss.sensitivity, ss.scroll_force), (83, 50));
        let keys = s.virtual_thumbwheel.unwrap().keys;
        assert_eq!(keys.iter().filter(|k| k.enabled).map(|k| k.control_id).collect::<Vec<_>>(), [83, 86]);
        assert!(db.scroll_defaults("4082").is_none());
    }

    #[test]
    fn fallback_prefix() {
        assert_eq!(super::fallback_slot_prefix("MX Anywhere 3", "6b025"), "mx-anywhere-3-6b025");
        assert_eq!(super::fallback_slot_prefix("M720 Triathlon", "4055"), "m720-triathlon-4055");
    }
}

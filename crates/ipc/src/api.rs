//! The agent API: routes and their payloads, shared by the agent, the GUI and the CLI.

use serde::{Deserialize, Serialize};

use crate::model::{Application, Assignment, Profile, ScrollWheelSettings, ThumbWheelSettings};

pub mod routes {
    pub const AGENT_INFO: &str = "/agent/info";
    /// `DeviceList`; event when devices change.
    pub const DEVICES: &str = "/devices";
    pub const DEVICES_CHANGED: &str = "/devices/changed";
    /// Removes a device that is not connected from the device list (`IdArg`).
    pub const DEVICE_FORGET: &str = "/devices/forget";
    /// Drops all assignments of a device so its defaults apply (`IdArg`).
    pub const DEVICE_RESET: &str = "/devices/reset";
    /// `ProfileList`; event with the new list when profiles change.
    pub const PROFILES: &str = "/profiles";
    pub const PROFILES_CHANGED: &str = "/profiles/changed";
    /// Set or remove one assignment (`ProfileAssignment`).
    pub const ASSIGNMENT: &str = "/profiles/assignment";
    /// Create (set, `AppProfile`) or delete (remove, `IdArg`) an application profile.
    pub const APP_PROFILE: &str = "/profiles/application";
    /// `{"applications": [Application]}`
    pub const APPLICATIONS: &str = "/applications";
    pub const BACKUPS: &str = "/backups";
    pub const BACKUP_CREATE: &str = "/backups/create";
    pub const BACKUP_RESTORE: &str = "/backups/restore";
    pub const SETTINGS_EXPORT: &str = "/settings/export";
    pub const SETTINGS_IMPORT: &str = "/settings/import";
    /// Imports the settings of a local Logi Options+ installation (no payload).
    pub const SETTINGS_IMPORT_OPTIONS_PLUS: &str = "/settings/import-options-plus";
    /// System-wide primary/secondary button swap (`ButtonSwap`).
    pub const BUTTON_SWAP: &str = "/system/button-swap";
}

/// One assignment in a profile; an empty `profile_id` means the global profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileAssignment {
    pub profile_id: String,
    pub assignment: Option<Assignment>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
}

/// A new application profile and, optionally, the application it is for.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppProfile {
    pub profile: Profile,
    pub application: Option<Application>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub version: String,
    pub settings_dir: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: Option<u8>,
    /// `critical`, `low`, `good`, `full` or `unknown`.
    pub level: String,
    pub charging: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub cid: u16,
    pub slot_id: String,
    pub reprogrammable: bool,
    pub divertable: bool,
    pub gesture_capable: bool,
    /// Can report wheel turns while held ("hold + wheel" horizontal scrolling).
    #[serde(default)]
    pub horizontal_scroll: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dpi {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub default: u16,
    pub current: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub smartshift: bool,
    pub smartshift_torque: bool,
    pub hires_wheel: bool,
    pub thumbwheel: bool,
    /// Pointer speed through 0x2205 pointer motion scaling (mice without adjustable DPI).
    #[serde(default)]
    pub pointer_scaling: bool,
    /// A high-resolution sensor with a separate high-resolution DPI range.
    #[serde(default)]
    pub high_resolution_sensor: bool,
    /// Haptic feedback (0x19B0).
    #[serde(default)]
    pub haptics: bool,
    pub fn_inversion: bool,
    pub backlight: bool,
    pub easy_switch_hosts: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Stable id (serial number when available).
    pub id: String,
    pub name: String,
    pub model_id: String,
    pub extended_model_id: u8,
    pub slot_prefix: String,
    /// `mouse`, `keyboard`, ...
    pub kind: String,
    pub online: bool,
    pub serial: Option<String>,
    pub firmware: Vec<String>,
    pub connection: String,
    pub battery: Option<Battery>,
    pub dpi: Option<Dpi>,
    pub controls: Vec<Control>,
    pub capabilities: Capabilities,
    /// The model's default scroll wheel settings (vendor `mouseScrollWheelOverride`), used while the
    /// profile has no scroll wheel card.
    #[serde(default)]
    pub scroll_defaults: Option<ScrollWheelSettings>,
    /// The model's default thumb wheel settings, used while the profile has no thumb wheel card.
    #[serde(default)]
    pub thumb_wheel_defaults: Option<ThumbWheelSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub path: String,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupList {
    pub backups: Vec<Backup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PathArg {
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelArg {
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IdArg {
    pub id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ButtonSwap {
    /// Primary and secondary buttons are swapped (left-handed).
    pub swapped: bool,
}

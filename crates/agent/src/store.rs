//! Persistent settings: profiles and application definitions.
//!
//! Profiles use the same JSON layout as Logi Options+ ([`inputsmith_ipc::model`]),
//! so settings imported from Options+ load unchanged.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use inputsmith_ipc::api;
use inputsmith_ipc::model::{Application, Assignment, Card, Profile};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_PROFILE_NAME: &str = "PROFILE_NAME_DEFAULT";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub applications: Vec<Application>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O: {0}")]
    Io(#[from] io::Error),
    #[error("settings JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not an Options+ settings document")]
    NotOptionsPlus,
    #[error("Options+ database: {0}")]
    Sqlite(#[from] crate::sqlite::Error),
}

impl Settings {
    /// The global ("All applications") profile, created on demand.
    pub fn default_profile_mut(&mut self) -> &mut Profile {
        if let Some(i) = self.profiles.iter().position(is_default_profile) {
            return &mut self.profiles[i];
        }
        let id = new_id();
        self.profiles.push(Profile {
            id: id.clone(),
            name: DEFAULT_PROFILE_NAME.into(),
            application_id: id,
            active_for_application: true,
            ..Default::default()
        });
        self.profiles.last_mut().unwrap()
    }

    pub fn default_profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| is_default_profile(p))
    }

    /// The profile bound to the application whose executable is `exe_path`.
    pub fn profile_for_exe(&self, exe_path: &str) -> Option<&Profile> {
        let exe = exe_path.to_lowercase();
        let app = self.applications.iter().find(|a| {
            !a.is_disabled
                && (a.application_path.eq_ignore_ascii_case(&exe)
                    || a.application_paths_list.iter().any(|p| p.eq_ignore_ascii_case(&exe)))
        })?;
        self.profiles.iter().find(|p| !is_default_profile(p) && p.application_id == app.application_id)
    }

    /// Effective assignment for `slot_id`: the application profile's own
    /// assignment, falling back to the global profile.
    pub fn resolve<'a>(&'a self, app_profile: Option<&'a Profile>, slot_id: &str) -> Option<&'a Assignment> {
        let find = |p: &'a Profile| p.assignments.iter().find(|a| a.slot_id == slot_id && !a.is_disabled);
        app_profile.and_then(find).or_else(|| self.default_profile().and_then(find))
    }

    /// Inserts or replaces an assignment in `profile_id`.
    pub fn set_assignment(&mut self, profile_id: &str, assignment: Assignment) -> bool {
        let Some(p) = self.profiles.iter_mut().find(|p| p.id == profile_id) else { return false };
        match p.assignments.iter_mut().find(|a| a.slot_id == assignment.slot_id) {
            Some(a) => *a = assignment,
            None => p.assignments.push(assignment),
        }
        true
    }

    /// Every card assigned to `slot_id` in any profile.
    pub fn cards_for_slot<'a>(&'a self, slot_id: &'a str) -> impl Iterator<Item = &'a Card> + 'a {
        self.profiles
            .iter()
            .flat_map(|p| p.assignments.iter())
            .filter(move |a| a.slot_id == slot_id && !a.is_disabled)
            .filter_map(|a| a.card.as_ref())
    }

    /// Imports profiles and applications from the JSON document Options+
    /// stores in `settings.db` (keys `profile_keys`, `profile-<uuid>`,
    /// `applications.applications`). Existing entries with the same IDs are
    /// replaced.
    pub fn import_options_plus(&mut self, doc: &serde_json::Value) -> Result<usize, StoreError> {
        let keys = doc.get("profile_keys").and_then(|k| k.as_array()).ok_or(StoreError::NotOptionsPlus)?;
        let mut n = 0;
        for key in keys.iter().filter_map(|k| k.as_str()) {
            let Some(v) = doc.get(key) else { continue };
            let profile: Profile = serde_json::from_value(v.clone())?;
            self.profiles.retain(|p| p.id != profile.id);
            self.profiles.push(profile);
            n += 1;
        }
        if let Some(apps) = doc.pointer("/applications/applications").and_then(|a| a.as_array()) {
            for a in apps {
                let app: Application = serde_json::from_value(a.clone())?;
                self.applications.retain(|x| x.application_id != app.application_id);
                self.applications.push(app);
            }
        }
        Ok(n)
    }
}

pub fn is_default_profile(p: &Profile) -> bool {
    p.application_id == p.id || p.name == DEFAULT_PROFILE_NAME
}

/// Random RFC 4122 v4-shaped id from OS-seeded hashing (no extra dependency).
pub fn new_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// File-backed settings with export/import and local backups.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Per-user configuration directory: `%APPDATA%\inputsmith`,
    /// `$XDG_CONFIG_HOME/inputsmith` or `~/.config/inputsmith`,
    /// `~/Library/Application Support/inputsmith`.
    pub fn default_dir() -> PathBuf {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        };
        base.unwrap_or_else(|| PathBuf::from(".")).join("inputsmith")
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    pub fn load(&self) -> Result<Settings, StoreError> {
        match fs::read(self.settings_path()) {
            Ok(b) => Ok(serde_json::from_slice(&b)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Ok(Settings { version: SCHEMA_VERSION, ..Default::default() })
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, s: &Settings) -> Result<(), StoreError> {
        write_atomic(&self.settings_path(), s)
    }

    /// Writes the settings to an arbitrary file.
    pub fn export(&self, s: &Settings, to: &Path) -> Result<(), StoreError> {
        write_atomic(to, s)
    }

    /// Reads settings previously exported (or an Options+ settings document).
    pub fn import(&self, from: &Path) -> Result<Settings, StoreError> {
        let v: serde_json::Value = serde_json::from_slice(&fs::read(from)?)?;
        Self::from_document(v)
    }

    /// Settings from the local Logi Options+ installation's database
    /// (`%LOCALAPPDATA%\LogiOptionsPlus\settings.db`, including its write-ahead log).
    pub fn import_options_plus_db(&self, db: Option<&Path>) -> Result<Settings, StoreError> {
        let path = match db {
            Some(p) => p.to_path_buf(),
            None => options_plus_db_path().ok_or(StoreError::NotOptionsPlus)?,
        };
        Self::from_document(read_options_plus_db(&path)?)
    }

    fn from_document(v: serde_json::Value) -> Result<Settings, StoreError> {
        if v.get("profile_keys").is_some() {
            let mut s = Settings { version: SCHEMA_VERSION, ..Default::default() };
            s.import_options_plus(&v)?;
            return Ok(s);
        }
        Ok(serde_json::from_value(v)?)
    }

    /// Creates a timestamped backup and returns its path.
    pub fn create_backup(&self, s: &Settings, label: Option<&str>) -> Result<PathBuf, StoreError> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let label: String = label
            .unwrap_or("manual")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = self.backups_dir().join(format!("{ts}-{label}.json"));
        write_atomic(&path, s)?;
        Ok(path)
    }

    fn known_devices_path(&self) -> PathBuf {
        self.settings_path().with_file_name("devices.json")
    }

    /// Devices seen before, shown as inactive while not connected.
    pub fn load_known_devices(&self) -> Vec<api::Device> {
        match fs::read(self.known_devices_path()) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                log::warn!("ignoring unreadable known devices: {e}");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        }
    }

    pub fn save_known_devices(&self, devices: &[api::Device]) -> Result<(), StoreError> {
        let path = self.known_devices_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(devices)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Lists backups, newest first.
    pub fn list_backups(&self) -> Result<Vec<PathBuf>, StoreError> {
        let mut v: Vec<PathBuf> = match fs::read_dir(self.backups_dir()) {
            Ok(d) => d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        v.sort();
        v.reverse();
        Ok(v)
    }

    /// Replaces current settings with a backup, keeping a safety backup of
    /// the settings being replaced.
    pub fn restore_backup(&self, backup: &Path) -> Result<Settings, StoreError> {
        let restored = self.import(backup)?;
        let current = self.load()?;
        self.create_backup(&current, Some("before-restore"))?;
        self.save(&restored)?;
        Ok(restored)
    }
}

/// Where Options+ keeps its settings database, if it exists.
pub fn options_plus_db_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("LogiOptionsPlus")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/LogiOptionsPlus")
    } else {
        return None;
    };
    Some(base.join("settings.db")).filter(|p| p.exists())
}

/// The newest settings document in an Options+ `settings.db` (`data(_id, _date_created, file)`).
pub fn read_options_plus_db(path: &Path) -> Result<serde_json::Value, StoreError> {
    let db = crate::sqlite::Database::open(path)?;
    let rows = db.table("data")?;
    let row = rows.iter().max_by_key(|r| r.rowid).ok_or(StoreError::NotOptionsPlus)?;
    for v in &row.values {
        let bytes = match v {
            crate::sqlite::Value::Blob(b) => b.as_slice(),
            crate::sqlite::Value::Text(t) => t.as_bytes(),
            _ => continue,
        };
        if bytes.first() == Some(&b'{') {
            return Ok(serde_json::from_slice(bytes)?);
        }
    }
    Err(StoreError::NotOptionsPlus)
}

fn write_atomic(path: &Path, s: &Settings) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(slot: &str, card_id: &str) -> Assignment {
        Assignment {
            slot_id: slot.into(),
            card_id: card_id.into(),
            card: Some(Card { id: card_id.into(), ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn app_profile_overrides_and_falls_back() {
        let mut s = Settings::default();
        let gid = s.default_profile_mut().id.clone();
        s.set_assignment(&gid, assignment("m_c82", "global-middle"));
        s.set_assignment(&gid, assignment("m_c83", "global-back"));
        s.applications.push(Application {
            application_id: "app1".into(),
            application_path: r"c:\apps\foo.exe".into(),
            ..Default::default()
        });
        s.profiles.push(Profile { id: "p1".into(), application_id: "app1".into(), ..Default::default() });
        s.set_assignment("p1", assignment("m_c82", "app-middle"));

        let app = s.profile_for_exe(r"C:\Apps\FOO.exe");
        assert_eq!(app.map(|p| p.id.as_str()), Some("p1"));
        assert_eq!(s.resolve(app, "m_c82").unwrap().card_id, "app-middle");
        assert_eq!(s.resolve(app, "m_c83").unwrap().card_id, "global-back");
        assert_eq!(s.resolve(None, "m_c82").unwrap().card_id, "global-middle");
    }

    #[test]
    fn imports_options_plus_document() {
        let doc = serde_json::json!({
            "profile_keys": ["profile-abc"],
            "profile-abc": {
                "id": "abc", "name": "PROFILE_NAME_DEFAULT", "applicationId": "abc", "activeForApplication": true,
                "assignments": [{"slotId": "mx-anywhere-3-6b025_c82", "cardId": "card_global_presets_middle_button",
                    "card": {"id": "card_global_presets_middle_button", "attribute": "MACRO_PLAYBACK",
                        "macro": {"type": "MOUSE", "mouse": {"action": "BUTTON", "hidUsage": 3}}}}]
            },
            "applications": {"applications": [{"applicationId": "application_id_x", "applicationPath": "c:\\x.exe"}]}
        });
        let mut s = Settings::default();
        assert_eq!(s.import_options_plus(&doc).unwrap(), 1);
        let a = s.resolve(None, "mx-anywhere-3-6b025_c82").unwrap();
        assert_eq!(a.card.as_ref().unwrap().action.as_ref().unwrap().mouse.as_ref().unwrap().hid_usage, 3);
        assert_eq!(s.applications.len(), 1);
    }

    #[test]
    fn backup_roundtrip() {
        let dir = std::env::temp_dir().join(format!("inputsmith-test-{}", new_id()));
        let store = Store::new(&dir);
        let mut s = Settings::default();
        s.default_profile_mut();
        store.save(&s).unwrap();
        let b = store.create_backup(&s, Some("test")).unwrap();
        store.save(&Settings::default()).unwrap();
        let restored = store.restore_backup(&b).unwrap();
        assert_eq!(restored, s);
        assert_eq!(store.load().unwrap(), s);
        assert_eq!(store.list_backups().unwrap().len(), 2); // test + before-restore
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod options_plus_db_tests {
    use super::*;

    /// Reads the local Options+ settings database when present (developer machines only).
    #[test]
    fn imports_local_options_plus_if_installed() {
        let Some(path) = options_plus_db_path() else { return };
        let store = Store::new(std::env::temp_dir().join("inputsmith-unused"));
        let s = store.import_options_plus_db(Some(&path)).unwrap();
        let global = s.default_profile().expect("global profile");
        eprintln!(
            "Options+ import: {} profiles, {} applications, {} global assignments",
            s.profiles.len(),
            s.applications.len(),
            global.assignments.len()
        );
        for a in &global.assignments {
            let c = a.card.as_ref();
            eprintln!("  {} -> {}", a.slot_id, c.map(|c| c.id.as_str()).unwrap_or("-"));
            if let Some(w) = c.and_then(|c| c.mouse_scroll_wheel_settings.as_ref()) {
                eprintln!("     scroll: {}", serde_json::to_string(w).unwrap());
            }
        }
    }
}

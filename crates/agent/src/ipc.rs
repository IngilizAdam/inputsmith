//! Agent IPC server for the GUI, the CLI and scripts ([`inputsmith_ipc::api`] routes on
//! [`inputsmith_ipc::transport::agent_path`]).

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use inputsmith_ipc::api::{self, AppProfile, ProfileAssignment, ProfileList, routes};
use inputsmith_ipc::transport::{Conn, Listener};
use inputsmith_ipc::{Message, Status, Verb};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::devices::{AgentEvent, Shared};
use crate::store::{Settings, is_default_profile, new_id};

struct Client {
    conn: Conn,
    subs: Mutex<HashSet<String>>,
}

pub struct Server {
    shared: Arc<Shared>,
    clients: Mutex<Vec<Arc<Client>>>,
}

type HandlerResult = Result<Option<serde_json::Value>, (Status, String)>;

fn json<T: Serialize>(v: &T) -> HandlerResult {
    Ok(Some(serde_json::to_value(v).map_err(|e| (Status::Failed, e.to_string()))?))
}

fn arg<T: DeserializeOwned>(m: &Message) -> Result<T, (Status, String)> {
    let v = m.payload.clone().ok_or((Status::InvalidArgument, "missing payload".into()))?;
    serde_json::from_value(v).map_err(|e| (Status::InvalidArgument, e.to_string()))
}

fn store_err(e: crate::store::StoreError) -> (Status, String) {
    (Status::Failed, e.to_string())
}

impl Server {
    pub fn new(shared: Arc<Shared>) -> Arc<Self> {
        Arc::new(Self { shared, clients: Mutex::new(Vec::new()) })
    }

    /// Accepts clients forever on `path`; also forwards agent events as broadcasts.
    pub fn start(self: &Arc<Self>, path: &str) -> std::io::Result<()> {
        let mut listener = Listener::bind(path)?;
        log::info!("IPC listening on {path}");
        let me = self.clone();
        thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok(conn) => {
                        let client = Arc::new(Client { conn, subs: Mutex::new(HashSet::new()) });
                        me.clients.lock().unwrap().push(client.clone());
                        let me = me.clone();
                        thread::spawn(move || me.serve(client));
                    }
                    Err(e) => {
                        log::warn!("IPC accept failed: {e}");
                        thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            }
        });
        let me = self.clone();
        let events = self.shared.subscribe();
        thread::spawn(move || {
            for ev in events {
                match ev {
                    AgentEvent::DevicesChanged => {
                        let list = me.shared.device_list();
                        me.broadcast(routes::DEVICES_CHANGED, serde_json::to_value(list).ok());
                    }
                    AgentEvent::ProfilesChanged => {
                        let profiles = me.shared.settings.read().unwrap().profiles.clone();
                        me.broadcast(routes::PROFILES_CHANGED, serde_json::to_value(ProfileList { profiles }).ok());
                    }
                }
            }
        });
        Ok(())
    }

    fn serve(&self, client: Arc<Client>) {
        log::debug!("IPC client connected");
        for m in client.conn.messages() {
            let m = match m {
                Ok(m) => m,
                Err(e) => {
                    log::debug!("IPC client error: {e}");
                    break;
                }
            };
            let reply = match m.verb {
                Verb::Subscribe => {
                    client.subs.lock().unwrap().insert(m.path.clone());
                    m.reply(Status::Ok)
                }
                Verb::Event => continue,
                _ => match self.handle(&m) {
                    Ok(payload) => {
                        let mut r = m.reply(Status::Ok);
                        r.payload = payload;
                        r
                    }
                    Err((code, what)) => {
                        log::debug!("{} {} -> {code:?}: {what}", m.verb.as_str(), m.path);
                        m.reply(code).with_error(what)
                    }
                },
            };
            if client.conn.send(&reply).is_err() {
                break;
            }
        }
        self.clients.lock().unwrap().retain(|c| !Arc::ptr_eq(c, &client));
        log::debug!("IPC client disconnected");
    }

    fn broadcast(&self, path: &str, payload: Option<serde_json::Value>) {
        let mut msg = Message::new(Verb::Event, path);
        msg.payload = payload;
        let clients = self.clients.lock().unwrap().clone();
        for c in clients {
            if c.subs.lock().unwrap().contains(path) {
                let _ = c.conn.send(&msg);
            }
        }
    }

    fn handle(&self, m: &Message) -> HandlerResult {
        let sh = &self.shared;
        match (m.verb, m.path.as_str()) {
            (Verb::Get, routes::AGENT_INFO) => json(&api::AgentInfo {
                version: env!("CARGO_PKG_VERSION").into(),
                settings_dir: sh.store.settings_path().parent().map(|p| p.display().to_string()).unwrap_or_default(),
            }),
            (Verb::Get, routes::DEVICES) => json(&sh.device_list()),
            (Verb::Get, routes::PROFILES) => {
                json(&ProfileList { profiles: sh.settings.read().unwrap().profiles.clone() })
            }
            (Verb::Get, routes::APPLICATIONS) => json(&serde_json::json!({
                "applications": sh.settings.read().unwrap().applications
            })),
            (Verb::Set, routes::ASSIGNMENT) => {
                let pa: ProfileAssignment = arg(m)?;
                let assignment = pa.assignment.ok_or((Status::InvalidArgument, "missing assignment".into()))?;
                let found = sh
                    .update_settings(|s| {
                        let profile_id = if pa.profile_id.is_empty() {
                            s.default_profile_mut().id.clone()
                        } else {
                            pa.profile_id.clone()
                        };
                        s.set_assignment(&profile_id, assignment)
                    })
                    .map_err(store_err)?;
                if found { Ok(None) } else { Err((Status::NotFound, "no such profile".into())) }
            }
            (Verb::Remove, routes::ASSIGNMENT) => {
                // Removing an app-profile assignment restores the global one.
                let pa: ProfileAssignment = arg(m)?;
                let slot = pa.assignment.map(|a| a.slot_id).unwrap_or_default();
                sh.update_settings(|s| {
                    if let Some(p) = s.profiles.iter_mut().find(|p| p.id == pa.profile_id) {
                        p.assignments.retain(|a| a.slot_id != slot);
                    }
                })
                .map_err(store_err)?;
                Ok(None)
            }
            (Verb::Set, routes::APP_PROFILE) => {
                // Creates (or replaces) an application profile; the payload
                // carries the profile and optionally its application.
                let mut np: AppProfile = arg(m)?;
                if np.profile.id.is_empty() {
                    np.profile.id = new_id();
                }
                let id = np.profile.id.clone();
                sh.update_settings(|s| {
                    if let Some(app) = np.application {
                        s.applications.retain(|a| a.application_id != app.application_id);
                        s.applications.push(app);
                    }
                    s.profiles.retain(|p| p.id != np.profile.id);
                    s.profiles.push(np.profile);
                })
                .map_err(store_err)?;
                json(&api::IdArg { id })
            }
            (Verb::Remove, routes::APP_PROFILE) => {
                let a: api::IdArg = arg(m)?;
                sh.update_settings(|s| {
                    let app = s
                        .profiles
                        .iter()
                        .find(|p| p.id == a.id && !is_default_profile(p))
                        .map(|p| p.application_id.clone());
                    s.profiles.retain(|p| !(p.id == a.id && !is_default_profile(p)));
                    if let Some(app) = app {
                        s.applications.retain(|x| x.application_id != app);
                    }
                })
                .map_err(store_err)?;
                Ok(None)
            }
            (Verb::Get, routes::BACKUPS) => {
                let list = sh.store.list_backups().map_err(store_err)?;
                json(&api::BackupList { backups: list.iter().map(|p| backup_info(p)).collect() })
            }
            (Verb::Set, routes::BACKUP_CREATE) => {
                let a: api::LabelArg = arg(m).unwrap_or_default();
                let s = sh.settings.read().unwrap().clone();
                let label = (!a.label.is_empty()).then_some(a.label.as_str());
                let p = sh.store.create_backup(&s, label).map_err(store_err)?;
                json(&backup_info(&p))
            }
            (Verb::Set, routes::BACKUP_RESTORE) => {
                let a: api::PathArg = arg(m)?;
                let restored = sh.store.restore_backup(Path::new(&a.path)).map_err(store_err)?;
                sh.replace_settings(restored).map_err(store_err)?;
                Ok(None)
            }
            (Verb::Set, routes::SETTINGS_EXPORT) => {
                let a: api::PathArg = arg(m)?;
                let s = sh.settings.read().unwrap().clone();
                sh.store.export(&s, Path::new(&a.path)).map_err(store_err)?;
                Ok(None)
            }
            (Verb::Set, routes::SETTINGS_IMPORT) => {
                let a: api::PathArg = arg(m)?;
                let incoming: Settings = sh.store.import(Path::new(&a.path)).map_err(store_err)?;
                let current = sh.settings.read().unwrap().clone();
                sh.store.create_backup(&current, Some("before-import")).map_err(store_err)?;
                sh.replace_settings(incoming).map_err(store_err)?;
                Ok(None)
            }
            (Verb::Set, routes::SETTINGS_IMPORT_OPTIONS_PLUS) => {
                let incoming = sh.store.import_options_plus_db(None).map_err(store_err)?;
                let current = sh.settings.read().unwrap().clone();
                sh.store.create_backup(&current, Some("before-optionsplus-import")).map_err(store_err)?;
                sh.replace_settings(incoming).map_err(store_err)?;
                Ok(None)
            }
            (Verb::Set, routes::DEVICE_RESET) => {
                // Drops every assignment for the device's slots (native behaviour).
                let a: api::IdArg = arg(m)?;
                let prefix = sh
                    .device_list()
                    .devices
                    .into_iter()
                    .find(|d| d.id == a.id)
                    .map(|d| d.slot_prefix)
                    .ok_or((Status::NotFound, "unknown device".into()))?;
                sh.update_settings(|s| {
                    for p in &mut s.profiles {
                        p.assignments.retain(|x| !x.slot_id.starts_with(&format!("{prefix}_")));
                    }
                })
                .map_err(store_err)?;
                Ok(None)
            }
            (Verb::Set, routes::DEVICE_FORGET) => {
                let a: api::IdArg = arg(m)?;
                match sh.forget_device(&a.id).map_err(store_err)? {
                    true => Ok(None),
                    false => Err((Status::InvalidArgument, "device is connected".into())),
                }
            }
            (Verb::Get, routes::BUTTON_SWAP) => {
                let swapped = inputsmith_input::buttons_swapped().map_err(|e| (Status::Failed, e.to_string()))?;
                json(&api::ButtonSwap { swapped })
            }
            (Verb::Set, routes::BUTTON_SWAP) => {
                let a: api::ButtonSwap = arg(m)?;
                inputsmith_input::set_buttons_swapped(a.swapped).map_err(|e| (Status::Failed, e.to_string()))?;
                log::info!("primary/secondary buttons swapped: {}", a.swapped);
                Ok(None)
            }
            _ => Err((Status::UnknownRoute, format!("{} {}", m.verb.as_str(), m.path))),
        }
    }
}

fn backup_info(p: &Path) -> api::Backup {
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (ts, label) = stem.split_once('-').unwrap_or((&stem, ""));
    api::Backup { path: p.display().to_string(), created: ts.parse().unwrap_or(0), label: label.into() }
}

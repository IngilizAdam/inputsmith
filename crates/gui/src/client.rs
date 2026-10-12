//! Connection to the inputsmith agent, exposed to iced as a subscription.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use futures::SinkExt;
use iced::Subscription;
use iced::futures::Stream;
use inputsmith_ipc::api::routes;
use inputsmith_ipc::transport::{Conn, agent_path};
use inputsmith_ipc::{Message, Verb};

#[derive(Debug, Clone)]
pub enum Event {
    Connected,
    Disconnected,
    Message(Box<Message>),
}

fn outbox() -> &'static Mutex<Option<mpsc::Sender<Message>>> {
    static O: OnceLock<Mutex<Option<mpsc::Sender<Message>>>> = OnceLock::new();
    O.get_or_init(Default::default)
}

/// Sends a request to the agent; responses arrive as [`Event::Message`].
pub fn send(verb: Verb, path: &str, payload: Option<serde_json::Value>) {
    static ID: AtomicU64 = AtomicU64::new(1);
    let mut m = Message::new(verb, path);
    m.id = ID.fetch_add(1, Ordering::Relaxed);
    m.payload = payload;
    if let Some(tx) = outbox().lock().unwrap().as_ref() {
        let _ = tx.send(m);
    } else {
        log::debug!("not connected; dropped {} {path}", verb.as_str());
    }
}

pub fn get(path: &str) {
    send(Verb::Get, path, None);
}

pub fn set<T: serde::Serialize>(path: &str, payload: &T) {
    send(Verb::Set, path, serde_json::to_value(payload).ok());
}

pub fn remove<T: serde::Serialize>(path: &str, payload: &T) {
    send(Verb::Remove, path, serde_json::to_value(payload).ok());
}

/// Requests everything the UI renders.
pub fn refresh_all() {
    get(routes::DEVICES);
    get(routes::PROFILES);
    get(routes::APPLICATIONS);
    get(routes::BACKUPS);
}

/// Per-user directory for logs: `%LOCALAPPDATA%\inputsmith` on Windows, `~/.cache/inputsmith` elsewhere.
pub fn log_dir() -> Option<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
    };
    base.map(|b| b.join("inputsmith"))
}

/// Starts the agent next to this executable if none is reachable.
fn spawn_agent() {
    let Ok(exe) = std::env::current_exe() else { return };
    let agent = exe.with_file_name(if cfg!(windows) { "inputsmith-agent.exe" } else { "inputsmith-agent" });
    if agent.exists() {
        log::info!("starting agent {}", agent.display());
        let mut cmd = std::process::Command::new(agent);
        cmd.arg("run");
        // No console when started from here, so keep a log for troubleshooting.
        if let Some(dir) = log_dir() {
            cmd.arg("--log-file").arg(dir.join("agent.log"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let _ = cmd.spawn();
    }
}

fn run(mut tx: futures::channel::mpsc::Sender<Event>) {
    let mut spawned = false;
    loop {
        let conn = match Conn::connect(&agent_path()) {
            Ok(c) => c,
            Err(_) => {
                if !spawned {
                    spawned = true;
                    spawn_agent();
                }
                std::thread::sleep(Duration::from_millis(700));
                continue;
            }
        };
        let (otx, orx) = mpsc::channel::<Message>();
        *outbox().lock().unwrap() = Some(otx);
        let writer = conn.clone();
        std::thread::spawn(move || {
            for m in orx {
                if writer.send(&m).is_err() {
                    break;
                }
            }
        });
        for path in [routes::DEVICES_CHANGED, routes::PROFILES_CHANGED] {
            send(Verb::Subscribe, path, None);
        }
        let _ = futures::executor::block_on(tx.send(Event::Connected));
        for m in conn.messages() {
            match m {
                Ok(m) => {
                    if futures::executor::block_on(tx.send(Event::Message(Box::new(m)))).is_err() {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        *outbox().lock().unwrap() = None;
        let _ = futures::executor::block_on(tx.send(Event::Disconnected));
        std::thread::sleep(Duration::from_millis(700));
    }
}

fn stream() -> impl Stream<Item = Event> {
    iced::stream::channel(64, async |tx| {
        std::thread::spawn(move || run(tx));
        futures::future::pending::<()>().await;
    })
}

pub fn subscription() -> Subscription<Event> {
    Subscription::run(stream)
}

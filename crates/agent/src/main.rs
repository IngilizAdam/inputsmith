//! `inputsmith-agent` — background service and settings tool.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use inputsmith_agent::actions::Executor;
use inputsmith_agent::devicedb::DeviceDb;
use inputsmith_agent::devices::{Shared, run_channel};
use inputsmith_agent::logfile::RotatingLog;
use inputsmith_agent::store::{Settings, Store};

const USAGE: &str = "usage: inputsmith-agent [command]

commands:
  run [--log-file <file>]  run the agent (default); optionally log to <file> instead of stderr
  export <file>            write current settings to <file>
  import <file>            replace settings with <file> (inputsmith export or Options+ settings JSON)
  import-optionsplus [db]  replace settings with those of the local Logi Options+ (settings.db)
  backup [label]           create a local backup
  backups                  list local backups
  restore <file>           restore a backup (current settings are backed up first)
  path                     print the settings directory";

/// Size of the log file before it is rotated; one previous log is kept, so at
/// most about twice this on disk.
const LOG_ROTATE_BYTES: u64 = 1024 * 1024;

fn init_logging(log_file: Option<&str>) {
    let mut b = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Some(path) = log_file {
        let path = PathBuf::from(path);
        match RotatingLog::open(&path, LOG_ROTATE_BYTES) {
            Ok(f) => {
                b.target(env_logger::Target::Pipe(Box::new(f)));
            }
            Err(e) => eprintln!("cannot open log file {}: {e}", path.display()),
        }
    }
    b.init();
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let log_file = args.iter().position(|a| a == "--log-file").map(|i| {
        let v = args.get(i + 1).cloned().unwrap_or_default();
        args.drain(i..(i + 2).min(args.len()));
        v
    });
    init_logging(log_file.as_deref().filter(|p| !p.is_empty()));
    let store = Store::new(Store::default_dir());
    let result = match args.first().map(String::as_str) {
        None | Some("run") => run(store),
        Some("export") if args.len() == 2 => {
            store.load().and_then(|s| store.export(&s, args[1].as_ref())).map_err(Into::into)
        }
        Some("import") if args.len() == 2 => import(&store, &args[1]),
        Some("import-optionsplus") if args.len() <= 2 => store
            .import_options_plus_db(args.get(1).map(|p| p.as_ref()))
            .map_err(Into::into)
            .and_then(|s| replace(&store, s)),
        Some("backup") => store
            .load()
            .and_then(|s| store.create_backup(&s, args.get(1).map(String::as_str)))
            .map(|p| println!("{}", p.display()))
            .map_err(Into::into),
        Some("backups") => {
            store.list_backups().map(|v| v.iter().for_each(|p| println!("{}", p.display()))).map_err(Into::into)
        }
        Some("restore") if args.len() == 2 => store.restore_backup(args[1].as_ref()).map(|_| ()).map_err(Into::into),
        Some("path") => {
            println!("{}", Store::default_dir().display());
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type BoxError = Box<dyn std::error::Error>;

fn import(store: &Store, file: &str) -> Result<(), BoxError> {
    replace(store, store.import(file.as_ref())?)
}

fn replace(store: &Store, incoming: Settings) -> Result<(), BoxError> {
    let current = store.load()?;
    let backup = store.create_backup(&current, Some("before-import"))?;
    store.save(&incoming)?;
    println!(
        "imported {} profiles, {} applications (previous settings backed up to {})",
        incoming.profiles.len(),
        incoming.applications.len(),
        backup.display()
    );
    Ok(())
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn run(store: Store) -> Result<(), BoxError> {
    let settings: Settings = store.load()?;
    log::info!("settings: {} ({} profiles)", store.settings_path().display(), settings.profiles.len());
    let shared = Arc::new(Shared::new(
        settings,
        Store::new(Store::default_dir()),
        Executor::new(inputsmith_input::system_sink()),
        DeviceDb::load(),
    ));
    let server = inputsmith_agent::ipc::Server::new(shared.clone());
    if let Err(e) = server.start(&inputsmith_ipc::transport::agent_path()) {
        return Err(format!("cannot start IPC server (is another agent running?): {e}").into());
    }

    {
        let shared = shared.clone();
        ctrlc::set_handler(move || {
            log::info!("stopping...");
            shared.stop.store(true, Ordering::Relaxed);
        })?;
    }

    // Foreground application -> active application profile.
    let fg = {
        let shared = shared.clone();
        thread::spawn(move || {
            let mut last_exe = String::new();
            while !shared.stopping() {
                if let Some(app) = inputsmith_input::foreground_app() {
                    if app.exe_path != last_exe {
                        let profile =
                            shared.settings.read().unwrap().profile_for_exe(&app.exe_path).map(|p| p.id.clone());
                        let mut cur = shared.app_profile.write().unwrap();
                        if *cur != profile {
                            log::info!("foreground {} -> profile {:?}", app.exe_path, profile);
                            *cur = profile;
                        }
                        last_exe = app.exe_path;
                    }
                }
                thread::sleep(Duration::from_millis(250));
            }
        })
    };

    // Reload settings when the file changes (until the IPC server lands).
    let watcher = {
        let shared = shared.clone();
        let path = store.settings_path();
        thread::spawn(move || {
            let mut last = mtime(&path);
            while !shared.stopping() {
                thread::sleep(Duration::from_secs(1));
                let now = mtime(&path);
                if now != last {
                    last = now;
                    match Store::new(path.parent().unwrap()).load() {
                        // Our own saves land here too; only external edits differ.
                        Ok(s) if *shared.settings.read().unwrap() == s => {}
                        Ok(s) => {
                            log::info!("settings file changed externally, reapplying");
                            *shared.settings.write().unwrap() = s;
                            shared.reconfigure_all();
                            shared.emit(inputsmith_agent::devices::AgentEvent::ProfilesChanged);
                        }
                        Err(e) => log::warn!("ignoring unreadable settings: {e}"),
                    }
                }
            }
        })
    };

    // Hot-plug: one worker per HID++ interface.
    let mut workers: HashMap<String, JoinHandle<()>> = HashMap::new();
    while !shared.stopping() {
        workers.retain(|_, h| !h.is_finished());
        match inputsmith_hidpp::channel::enumerate() {
            Ok(ifaces) => {
                for info in ifaces {
                    if workers.contains_key(&info.container_id) {
                        continue;
                    }
                    let shared = shared.clone();
                    let key = info.container_id.clone();
                    workers.insert(key, thread::spawn(move || run_channel(info, shared)));
                }
            }
            Err(e) => log::warn!("enumeration failed: {e}"),
        }
        for _ in 0..20 {
            if shared.stopping() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    for (_, h) in workers {
        let _ = h.join();
    }
    let _ = fg.join();
    let _ = watcher.join();
    log::info!("stopped");
    Ok(())
}

//! Desktop integration: one GUI instance per user session, and starting at login.

/// Claims the single-instance lock. Returns `false` when another instance already runs; that
/// instance is asked to show its window (see [`on_activate`]) and this one should exit.
pub fn claim_single_instance() -> bool {
    imp::claim_single_instance()
}

/// Calls `f` (on a background thread) whenever a second launch asks this instance to show itself.
pub fn on_activate(f: impl Fn() + Send + 'static) {
    imp::on_activate(f)
}

/// Whether the GUI (and with it the agent) starts at login.
pub fn autostart_enabled() -> bool {
    imp::autostart_enabled()
}

/// Starts this executable with `--hidden` at login, or stops doing so.
pub fn set_autostart(on: bool) -> std::io::Result<()> {
    imp::set_autostart(on)
}

fn launch_command() -> std::io::Result<String> {
    Ok(format!("\"{}\" --hidden", std::env::current_exe()?.display()))
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::ptr::{null, null_mut};
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, GetLastError, HANDLE};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject};

    const MUTEX: &str = "Local\\inputsmith-gui-instance";
    const EVENT: &str = "Local\\inputsmith-gui-activate";
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const RUN_VALUE: &str = "inputsmith";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// The activation event, as an integer so it can cross threads; it lives as long as the process.
    static EVENT_HANDLE: OnceLock<isize> = OnceLock::new();

    fn event() -> HANDLE {
        *EVENT_HANDLE.get_or_init(|| unsafe { CreateEventW(null(), 0, 0, wide(EVENT).as_ptr()) as isize }) as HANDLE
    }

    pub fn claim_single_instance() -> bool {
        let ev = event();
        // Intentionally never closed: the mutex marks this process as the running instance.
        let mutex = unsafe { CreateMutexW(null(), 0, wide(MUTEX).as_ptr()) };
        if mutex.is_null() {
            return true; // cannot tell; better to run than to refuse
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            if !ev.is_null() {
                unsafe { SetEvent(ev) };
            }
            return false;
        }
        true
    }

    pub fn on_activate(f: impl Fn() + Send + 'static) {
        let ev = event() as isize;
        if ev == 0 {
            return;
        }
        std::thread::spawn(move || {
            while unsafe { WaitForSingleObject(ev as HANDLE, INFINITE) } == 0 {
                f();
            }
        });
    }

    pub fn autostart_enabled() -> bool {
        let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
        let mut size = 0u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                null_mut(),
                &mut size,
            )
        };
        r == 0
    }

    pub fn set_autostart(on: bool) -> io::Result<()> {
        let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
        let r = if on {
            let data = wide(&super::launch_command()?);
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            }
        } else {
            match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => 0,
                r => r,
            }
        };
        if r == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(r as i32)) }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::PathBuf;

    // TODO: single instance via a lock file / D-Bus name once Linux is in scope.
    pub fn claim_single_instance() -> bool {
        true
    }

    pub fn on_activate(_: impl Fn() + Send + 'static) {}

    /// XDG autostart entry (`~/.config/autostart/inputsmith.desktop`).
    fn entry() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(config.join("autostart").join("inputsmith.desktop"))
    }

    pub fn autostart_enabled() -> bool {
        entry().is_some_and(|p| p.exists())
    }

    pub fn set_autostart(on: bool) -> io::Result<()> {
        let path = entry().ok_or_else(|| io::Error::other("no config directory"))?;
        if on {
            std::fs::create_dir_all(path.parent().unwrap())?;
            let exec = super::launch_command()?;
            std::fs::write(&path, format!("[Desktop Entry]\nType=Application\nName=Inputsmith\nExec={exec}\n"))
        } else {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }
}

//! Synthetic input and desktop integration.
//!
//! Keys are addressed by HID keyboard usage (page 0x07), the encoding used by
//! action cards (`Macro.Keystroke.code` / `modifiers`), so the action layer
//! stays platform-neutral.

#[cfg(windows)]
mod windows;

pub mod usage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("OS error: {0}")]
    Os(#[from] std::io::Error),
    #[error("key usage {0:#04x} has no mapping on this platform")]
    UnmappedKey(u16),
    #[error("not supported on this platform")]
    Unsupported,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    /// From a HID button usage (page 0x09): 1 left, 2 right, 3 middle, 4 back, 5 forward.
    pub fn from_hid(usage: u32) -> Option<Self> {
        Some(match usage {
            1 => Self::Left,
            2 => Self::Right,
            3 => Self::Middle,
            4 => Self::Back,
            5 => Self::Forward,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKey {
    PlayPause,
    Stop,
    VolumeUp,
    VolumeDown,
    NextTrack,
    PreviousTrack,
    Mute,
}

/// The application owning the foreground window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundApp {
    pub pid: u32,
    /// Full executable path, lower-cased on Windows for stable matching.
    pub exe_path: String,
    pub window_title: String,
}

/// Sink for synthetic input events.
pub trait InputSink: Send + Sync {
    fn key(&self, usage: u16, down: bool) -> Result<()>;
    fn mouse_button(&self, button: MouseButton, down: bool) -> Result<()>;
    /// Wheel movement in 1/120 detent units; positive = up / right.
    fn wheel(&self, vertical: i32, horizontal: i32) -> Result<()>;
    fn move_relative(&self, dx: i32, dy: i32) -> Result<()>;
    fn media(&self, key: MediaKey) -> Result<()>;
    /// Types arbitrary text independent of keyboard layout.
    fn text(&self, text: &str) -> Result<()>;

    /// Presses `modifiers` then `key`, releases in reverse order.
    fn chord(&self, modifiers: &[u16], key: Option<u16>) -> Result<()> {
        for &m in modifiers {
            self.key(m, true)?;
        }
        let r = key.map_or(Ok(()), |k| self.key(k, true).and_then(|_| self.key(k, false)));
        for &m in modifiers.iter().rev() {
            self.key(m, false)?;
        }
        r
    }

    fn click(&self, button: MouseButton) -> Result<()> {
        self.mouse_button(button, true)?;
        self.mouse_button(button, false)
    }
}

/// The platform's default input sink.
pub fn system_sink() -> Box<dyn InputSink> {
    #[cfg(windows)]
    {
        Box::new(windows::SendInputSink)
    }
    #[cfg(not(windows))]
    {
        Box::new(UnsupportedSink)
    }
}

#[cfg(not(windows))]
struct UnsupportedSink;

#[cfg(not(windows))]
impl InputSink for UnsupportedSink {
    fn key(&self, _: u16, _: bool) -> Result<()> {
        Err(Error::Unsupported)
    }
    fn mouse_button(&self, _: MouseButton, _: bool) -> Result<()> {
        Err(Error::Unsupported)
    }
    fn wheel(&self, _: i32, _: i32) -> Result<()> {
        Err(Error::Unsupported)
    }
    fn move_relative(&self, _: i32, _: i32) -> Result<()> {
        Err(Error::Unsupported)
    }
    fn media(&self, _: MediaKey) -> Result<()> {
        Err(Error::Unsupported)
    }
    fn text(&self, _: &str) -> Result<()> {
        Err(Error::Unsupported)
    }
}

/// Returns the foreground application, if it can be determined.
pub fn foreground_app() -> Option<ForegroundApp> {
    #[cfg(windows)]
    {
        windows::foreground_app()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Whether the primary and secondary mouse buttons are swapped system-wide.
pub fn buttons_swapped() -> Result<bool> {
    #[cfg(windows)]
    {
        Ok(windows::buttons_swapped())
    }
    #[cfg(not(windows))]
    {
        Err(Error::Unsupported)
    }
}

/// Swaps (or restores) the primary and secondary mouse buttons for all mice,
/// persistently, as Options+ does (OS setting, nothing is sent to devices).
pub fn set_buttons_swapped(swapped: bool) -> Result<()> {
    #[cfg(windows)]
    {
        windows::set_buttons_swapped(swapped)
    }
    #[cfg(not(windows))]
    {
        let _ = swapped;
        Err(Error::Unsupported)
    }
}

/// Starts a program with arguments, detached.
pub fn launch(path: &str, args: &[String], working_dir: Option<&str>) -> Result<()> {
    let mut cmd = std::process::Command::new(path);
    cmd.args(args);
    if let Some(d) = working_dir.filter(|d| !d.is_empty()) {
        cmd.current_dir(d);
    }
    cmd.spawn()?;
    Ok(())
}

/// Opens a URL, file or folder with the desktop's default handler.
pub fn open(target: &str) -> Result<()> {
    #[cfg(windows)]
    {
        windows::shell_open(target)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(target).spawn()?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(target).spawn()?;
        Ok(())
    }
}

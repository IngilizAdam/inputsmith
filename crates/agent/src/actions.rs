//! Executes action cards ([`Card`]) through an [`InputSink`].
//!
//! Press/release semantics: keystrokes and mouse buttons are held while the
//! physical control is held (so remapped buttons can drag or auto-repeat);
//! every other action fires once on press.

use std::thread;
use std::time::Duration;

use inputsmith_input::{InputSink, MediaKey, MouseButton, usage};
use inputsmith_ipc::model::{
    AdvancedClick, AdvancedClickType, Attribute, Card, Macro, MacroType, MediaUsage, MouseAction, QuickLaunchAction,
    StateSequence, SystemAction,
};

/// Device-side operations an action may need (DPI changes, SmartShift).
pub trait DeviceOps {
    fn dpi_step(&self, _delta: i32) {}
    fn dpi_cycle(&self) {}
    fn dpi_default(&self) {}
    fn toggle_smartshift(&self) {}
}

/// No-op device operations (virtual devices, tests).
pub struct NoDevice;
impl DeviceOps for NoDevice {}

pub struct Executor {
    sink: Box<dyn InputSink>,
}

/// A keystroke chord in HID usages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Vec<u16>,
    pub key: Option<u16>,
}

fn chord(modifiers: &[u16], key: u16) -> Chord {
    Chord { modifiers: modifiers.to_vec(), key: (key != 0).then_some(key) }
}

/// Resolves the card to execute for a press: `ONE_OF` cards delegate to their
/// selected nested card; adapters and settings cards are not directly executable.
pub fn effective_card(card: &Card) -> &Card {
    let mut c = card;
    for _ in 0..8 {
        if c.attribute == Attribute::OneOf {
            match c.nested_cards.get(&c.selected_nested_card) {
                Some(n) => c = n,
                None => break,
            }
        } else {
            break;
        }
    }
    c
}

impl Executor {
    pub fn new(sink: Box<dyn InputSink>) -> Self {
        Self { sink }
    }

    pub fn sink(&self) -> &dyn InputSink {
        self.sink.as_ref()
    }

    /// The physical control was pressed.
    pub fn press(&self, card: &Card, dev: &dyn DeviceOps) {
        let card = effective_card(card);
        if card.attribute != Attribute::MacroPlayback {
            log::debug!("card {} ({:?}) is not directly executable", card.id, card.attribute);
            return;
        }
        if let Some(m) = &card.action {
            log::debug!("action down: {} ({:?})", card.id, m.kind);
            if let Err(e) = self.macro_down(m, dev) {
                log::warn!("action {} failed: {e}", card.id);
            }
        }
    }

    /// The physical control was released.
    pub fn release(&self, card: &Card) {
        let card = effective_card(card);
        if let Some(m) = card.action.as_ref().filter(|_| card.attribute == Attribute::MacroPlayback) {
            if let Err(e) = self.macro_up(m) {
                log::warn!("action {} release failed: {e}", card.id);
            }
        }
    }

    /// Press immediately followed by release.
    pub fn trigger(&self, card: &Card, dev: &dyn DeviceOps) {
        self.press(card, dev);
        self.release(card);
    }

    fn macro_down(&self, m: &Macro, dev: &dyn DeviceOps) -> inputsmith_input::Result<()> {
        let s = self.sink.as_ref();
        match &m.kind {
            MacroType::Keystroke => {
                let k = m.keystroke.as_ref().map(|k| chord_from_keystroke(k.code, &k.modifiers));
                if let Some(c) = k {
                    for &md in &c.modifiers {
                        s.key(md, true)?;
                    }
                    if let Some(key) = c.key {
                        s.key(key, true)?;
                    }
                }
            }
            MacroType::Mouse => {
                let Some(mm) = &m.mouse else { return Ok(()) };
                match &mm.action {
                    MouseAction::Button => {
                        if let Some(b) = MouseButton::from_hid(mm.hid_usage) {
                            s.mouse_button(b, true)?;
                        }
                    }
                    MouseAction::WinBack | MouseAction::OsxGestureBack => s.mouse_button(MouseButton::Back, true)?,
                    MouseAction::WinForward | MouseAction::OsxGestureForward => {
                        s.mouse_button(MouseButton::Forward, true)?
                    }
                    MouseAction::ButtonDoubleClick => {
                        let b = MouseButton::from_hid(mm.hid_usage).unwrap_or(MouseButton::Left);
                        s.click(b)?;
                        s.click(b)?;
                    }
                    MouseAction::WheelUp => s.wheel(120, 0)?,
                    MouseAction::WheelDown => s.wheel(-120, 0)?,
                    MouseAction::WheelLeft => s.wheel(0, -120)?,
                    MouseAction::WheelRight => s.wheel(0, 120)?,
                    MouseAction::DpiUp => dev.dpi_step(1),
                    MouseAction::DpiDown => dev.dpi_step(-1),
                    MouseAction::DpiCycle => dev.dpi_cycle(),
                    MouseAction::DpiDefault => dev.dpi_default(),
                    other => log::info!("mouse action {other:?} not implemented"),
                }
            }
            MacroType::Media => {
                if let Some(k) = m.media.as_ref().and_then(|x| media_key(&x.usage)) {
                    s.media(k)?;
                }
            }
            MacroType::System => {
                if let Some(a) = &m.system {
                    self.system_action(&a.action)?;
                }
            }
            MacroType::App => {
                if let Some(app) = &m.app {
                    let wd = Some(app.working_directory_path.as_str());
                    inputsmith_input::launch(&app.executable_path, &app.argument_list, wd)?;
                }
            }
            MacroType::OpenFileFolder => {
                if let Some(o) = &m.open_file_folder {
                    inputsmith_input::open(&o.path)?;
                }
            }
            MacroType::OpenWebPage => {
                if let Some(o) = &m.open_web_page {
                    inputsmith_input::open(&o.url)?;
                }
            }
            MacroType::TextBlock => {
                if let Some(t) = &m.text_block {
                    s.text(&t.text)?;
                }
            }
            MacroType::Delay => {
                if let Some(d) = &m.delay {
                    thread::sleep(Duration::from_millis(d.duration_ms as u64));
                }
            }
            MacroType::AdvancedClick => {
                if let Some(a) = &m.advanced_click {
                    self.advanced_click(a)?;
                }
            }
            MacroType::QuickLaunch => {
                if m.quick_launch.as_ref().is_some_and(|q| q.action == QuickLaunchAction::Calculator) {
                    inputsmith_input::launch(if cfg!(windows) { "calc.exe" } else { "gnome-calculator" }, &[], None)?;
                }
            }
            MacroType::ScreenCapture => self.system_action(&SystemAction::WindowsScreenCapture)?,
            MacroType::Sequence => {
                if let Some(seq) = &m.sequence {
                    let part = seq.simple_sequence.as_ref().or(seq.press_sequence.as_ref());
                    if let Some(part) = part {
                        self.run_sequence(part, seq.use_default_delay.then_some(seq.default_delay), dev)?;
                    }
                }
            }
            MacroType::DoNothing | MacroType::Invalid => {}
            other => log::info!("macro type {other:?} not implemented"),
        }
        Ok(())
    }

    fn macro_up(&self, m: &Macro) -> inputsmith_input::Result<()> {
        let s = self.sink.as_ref();
        match &m.kind {
            MacroType::Keystroke => {
                if let Some(k) = &m.keystroke {
                    let c = chord_from_keystroke(k.code, &k.modifiers);
                    if let Some(key) = c.key {
                        s.key(key, false)?;
                    }
                    for &md in c.modifiers.iter().rev() {
                        s.key(md, false)?;
                    }
                }
            }
            MacroType::Mouse => {
                if let Some(mm) = &m.mouse {
                    let b = match &mm.action {
                        MouseAction::Button => MouseButton::from_hid(mm.hid_usage),
                        MouseAction::WinBack | MouseAction::OsxGestureBack => Some(MouseButton::Back),
                        MouseAction::WinForward | MouseAction::OsxGestureForward => Some(MouseButton::Forward),
                        _ => None,
                    };
                    if let Some(b) = b {
                        s.mouse_button(b, false)?;
                    }
                }
            }
            MacroType::Sequence => {
                if let Some(rel) = m.sequence.as_ref().and_then(|s| s.release_sequence.as_ref()) {
                    self.run_sequence(rel, None, &NoDevice)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn run_sequence(
        &self,
        seq: &StateSequence,
        default_delay: Option<u32>,
        dev: &dyn DeviceOps,
    ) -> inputsmith_input::Result<()> {
        let s = self.sink.as_ref();
        for c in &seq.components {
            if let Some(k) = &c.keyboard {
                s.key(k.hid_usage as u16, k.is_down)?;
            } else if let Some(mb) = c.mouse.as_ref().and_then(|m| m.button.as_ref()) {
                if let Some(b) = MouseButton::from_hid(mb.hid_usage) {
                    s.mouse_button(b, mb.is_down)?;
                }
            } else if let Some(w) = c.mouse.as_ref().and_then(|m| m.wheel.as_ref()) {
                let amount = if w.is_positive { 120 } else { -120 };
                // Usage 0x238 (AC Pan) is the horizontal wheel.
                if w.hid_usage == 0x238 { s.wheel(0, amount)? } else { s.wheel(amount, 0)? }
            } else if let Some(d) = &c.delay {
                thread::sleep(Duration::from_millis(d.duration_ms as u64));
                continue;
            } else if let Some(t) = &c.text_block {
                s.text(&t.text)?;
            } else if let Some(k) = &c.keystroke {
                let ch = chord_from_keystroke(k.code, &k.modifiers);
                s.chord(&ch.modifiers, ch.key)?;
            } else if let Some(mm) = &c.mouse_macro {
                let m = Macro { kind: MacroType::Mouse, mouse: Some(mm.clone()), ..Default::default() };
                self.macro_down(&m, dev)?;
                self.macro_up(&m)?;
            } else if let Some(sys) = &c.system {
                self.system_action(&sys.action)?;
            } else if let Some(media) = c.media.as_ref().and_then(|m| media_key(&m.usage)) {
                s.media(media)?;
            } else if let Some(app) = &c.app {
                inputsmith_input::launch(&app.executable_path, &app.argument_list, None)?;
            }
            if let Some(d) = default_delay.filter(|&d| d > 0) {
                thread::sleep(Duration::from_millis(d as u64));
            }
        }
        Ok(())
    }

    fn advanced_click(&self, a: &AdvancedClick) -> inputsmith_input::Result<()> {
        use AdvancedClickType as T;
        let s = self.sink.as_ref();
        let mut mods = Vec::new();
        if a.is_shift_enabled {
            mods.push(usage::LEFT_SHIFT);
        }
        if a.is_ctrl_enabled {
            mods.push(usage::LEFT_CTRL);
        }
        if a.is_alt_enabled {
            mods.push(usage::LEFT_ALT);
        }
        if a.is_win_enabled {
            mods.push(usage::LEFT_GUI);
        }
        let (button, count) = match &a.click_type {
            T::SingleClick => (MouseButton::Left, 1),
            T::DoubleClick => (MouseButton::Left, 2),
            T::TripleClick => (MouseButton::Left, 3),
            T::RightClick => (MouseButton::Right, 1),
            T::MiddleClick => (MouseButton::Middle, 1),
            T::Other(_) => (MouseButton::Left, 1),
        };
        for &m in &mods {
            s.key(m, true)?;
        }
        for _ in 0..count {
            s.click(button)?;
        }
        for &m in mods.iter().rev() {
            s.key(m, false)?;
        }
        Ok(())
    }

    fn system_action(&self, a: &SystemAction) -> inputsmith_input::Result<()> {
        let s = self.sink.as_ref();
        match system_chord(a) {
            Some(c) => s.chord(&c.modifiers, c.key),
            None => match a {
                SystemAction::ScrollLeft => s.wheel(0, -120),
                SystemAction::ScrollRight => s.wheel(0, 120),
                SystemAction::MailClient => inputsmith_input::open("mailto:"),
                SystemAction::None => Ok(()),
                other => {
                    log::info!("system action {other:?} not implemented");
                    Ok(())
                }
            },
        }
    }
}

/// Keystroke cards encode the key in `code` and modifiers as HID usages
/// (0xE0..=0xE7). A `code` of 0 means a modifier-only chord (e.g. the Windows key).
pub fn chord_from_keystroke(code: u32, modifiers: &[u32]) -> Chord {
    Chord { modifiers: modifiers.iter().map(|&m| m as u16).collect(), key: (code != 0).then_some(code as u16) }
}

fn media_key(u: &MediaUsage) -> Option<MediaKey> {
    Some(match u {
        MediaUsage::PlayPause => MediaKey::PlayPause,
        MediaUsage::Stop => MediaKey::Stop,
        MediaUsage::VolumeUp => MediaKey::VolumeUp,
        MediaUsage::VolumeDown => MediaKey::VolumeDown,
        MediaUsage::NextTrack => MediaKey::NextTrack,
        MediaUsage::PreviousTrack => MediaKey::PreviousTrack,
        MediaUsage::Mute => MediaKey::Mute,
        _ => return None,
    })
}

/// Keyboard shortcuts implementing system actions on Windows.
#[cfg(not(target_os = "macos"))]
pub fn system_chord(a: &SystemAction) -> Option<Chord> {
    use SystemAction as A;
    use usage::*;
    let (ctrl, shift, alt, gui) = (LEFT_CTRL, LEFT_SHIFT, LEFT_ALT, LEFT_GUI);
    Some(match a {
        A::Close => chord(&[alt], F4),
        A::Minimize => chord(&[gui], DOWN),
        A::Maximize => chord(&[gui], UP),
        A::Fullscreen => chord(&[], 0x44), // F11
        A::Copy => chord(&[ctrl], letter(b'c')),
        A::Paste => chord(&[ctrl], letter(b'v')),
        A::Undo => chord(&[ctrl], letter(b'z')),
        A::Redo => chord(&[ctrl], letter(b'y')),
        A::Desktop | A::ShowDesktop => chord(&[gui], D),
        A::FileClient => chord(&[gui], letter(b'e')),
        A::ZoomIn => chord(&[ctrl], 0x2E),  // Ctrl+=
        A::ZoomOut => chord(&[ctrl], 0x2D), // Ctrl+-
        A::SnapLeft => chord(&[gui], LEFT),
        A::SnapRight => chord(&[gui], RIGHT),
        A::AppExpose | A::TaskView => chord(&[gui], TAB),
        A::SwitchBetweenDesktopsLeft => chord(&[ctrl, gui], LEFT),
        A::SwitchBetweenDesktopsRight => chord(&[ctrl, gui], RIGHT),
        // Persistent task switcher, like Options+'s "Switch application".
        A::SwitchApps => chord(&[ctrl, alt], TAB),
        A::ShiftSwitchApps => chord(&[ctrl, alt, shift], TAB),
        A::Lock => chord(&[gui], L),
        A::WindowsScreenCapture => chord(&[gui, shift], letter(b's')),
        A::WindowsPrintScreen => chord(&[], PRINT_SCREEN),
        A::AppMenu => chord(&[], APPLICATION),
        A::SwitchLanguage | A::LanguageSwitch => chord(&[gui], SPACE),
        A::EnableDictation => chord(&[gui], letter(b'h')),
        A::WindowsHighContrast => chord(&[alt, shift], PRINT_SCREEN),
        A::Home => chord(&[], 0x4A),
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
pub fn system_chord(_a: &SystemAction) -> Option<Chord> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl InputSink for Recorder {
        fn key(&self, u: u16, down: bool) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("key {u:#04x} {}", if down { "down" } else { "up" }));
            Ok(())
        }
        fn mouse_button(&self, b: MouseButton, down: bool) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("{b:?} {}", if down { "down" } else { "up" }));
            Ok(())
        }
        fn wheel(&self, v: i32, h: i32) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("wheel {v} {h}"));
            Ok(())
        }
        fn move_relative(&self, _: i32, _: i32) -> inputsmith_input::Result<()> {
            Ok(())
        }
        fn media(&self, k: MediaKey) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("media {k:?}"));
            Ok(())
        }
        fn text(&self, t: &str) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("text {t}"));
            Ok(())
        }
    }

    fn card(json: serde_json::Value) -> Card {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn keystroke_card_holds_until_release() {
        let rec = Recorder::default();
        let ex = Executor::new(Box::new(rec.clone()));
        // Ctrl+C as stored by Options+ (card_global_presets_win_copy).
        let c = card(serde_json::json!({"id": "copy", "attribute": "MACRO_PLAYBACK",
            "macro": {"type": "KEYSTROKE", "keystroke": {"code": 6, "modifiers": [224]}}}));
        ex.press(&c, &NoDevice);
        assert_eq!(*rec.0.lock().unwrap(), vec!["key 0xe0 down", "key 0x06 down"]);
        ex.release(&c);
        assert_eq!(rec.0.lock().unwrap()[2..], ["key 0x06 up", "key 0xe0 up"]);
    }

    #[test]
    fn one_of_uses_selected_nested_card_and_mouse_actions() {
        let rec = Recorder::default();
        let ex = Executor::new(Box::new(rec.clone()));
        let c = card(serde_json::json!({"id": "g", "attribute": "ONE_OF", "selectedNestedCard": "b",
        "nestedCards": {
            "a": {"id": "a", "attribute": "MACRO_PLAYBACK", "macro": {"type": "MEDIA", "media": {"usage": "MUTE"}}},
            "b": {"id": "b", "attribute": "MACRO_PLAYBACK", "macro": {"type": "MOUSE", "mouse": {"action": "WIN_BACK"}}}
        }}));
        ex.trigger(&c, &NoDevice);
        assert_eq!(*rec.0.lock().unwrap(), vec!["Back down", "Back up"]);
    }

    #[test]
    fn system_switch_apps_is_ctrl_alt_tab() {
        let c = system_chord(&SystemAction::SwitchApps).unwrap();
        assert_eq!(c.modifiers, vec![usage::LEFT_CTRL, usage::LEFT_ALT]);
        assert_eq!(c.key, Some(usage::TAB));
    }
}

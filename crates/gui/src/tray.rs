//! Notification-area icon with a menu, like the Options+ tray icon.

use std::sync::OnceLock;

use iced::Subscription;
use iced::futures::Stream;
use inputsmith_ipc::api;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    Open,
    Quit,
}

const OPEN_ID: &str = "open";
const QUIT_ID: &str = "quit";

/// Draws the 32x32 tray icon: a mouse silhouette with a teal wheel.
fn icon() -> Icon {
    const N: usize = 32;
    let mut px = vec![0u8; N * N * 4];
    let (cx, cy, rx, ry) = (15.5f32, 16.0f32, 9.5f32, 14.0f32);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            let d = ((fx - cx) / rx).powi(2) + ((fy - cy) / ry).powi(2);
            let i = (y * N + x) * 4;
            let wheel = (fx - cx).abs() <= 1.5 && (5.0..=11.0).contains(&fy);
            let split = (fx - cx).abs() < 0.6 && fy < 13.0;
            if d <= 1.0 {
                let c: [u8; 4] = if wheel {
                    [0x00, 0xE0, 0xC4, 0xFF]
                } else if split {
                    [0x30, 0x30, 0x30, 0xFF]
                } else {
                    [0xF2, 0xF2, 0xF2, 0xFF]
                };
                px[i..i + 4].copy_from_slice(&c);
            } else if d <= 1.15 {
                px[i..i + 4].copy_from_slice(&[0x20, 0x20, 0x20, 0xC0]);
            }
        }
    }
    Icon::from_rgba(px, N as u32, N as u32).expect("valid icon")
}

fn menu(devices: &[api::Device]) -> Menu {
    let m = Menu::new();
    let _ = m.append(&MenuItem::with_id(OPEN_ID, "Open Inputsmith", true, None));
    let _ = m.append(&PredefinedMenuItem::separator());
    if devices.is_empty() {
        let _ = m.append(&MenuItem::new("No devices", false, None));
    }
    for d in devices {
        let battery = d.battery.as_ref().and_then(|b| b.percent).map(|p| format!("  —  {p}%")).unwrap_or_default();
        let state = if d.online { "" } else { "  (inactive)" };
        let _ = m.append(&MenuItem::new(format!("{}{battery}{state}", d.name), false, None));
    }
    let _ = m.append(&PredefinedMenuItem::separator());
    let _ = m.append(&MenuItem::with_id(QUIT_ID, "Quit", true, None));
    m
}

thread_local! {
    static TRAY: std::cell::RefCell<Option<TrayIcon>> = const { std::cell::RefCell::new(None) };
}

/// Creates the tray icon; must run on the UI (event loop) thread.
pub fn create() {
    TRAY.with(|t| {
        if t.borrow().is_some() {
            return;
        }
        match TrayIconBuilder::new()
            .with_icon(icon())
            .with_tooltip("Inputsmith")
            .with_menu(Box::new(menu(&[])))
            .with_menu_on_left_click(false)
            .build()
        {
            Ok(icon) => *t.borrow_mut() = Some(icon),
            Err(e) => log::warn!("tray icon unavailable: {e}"),
        }
    });
}

/// Refreshes the device entries in the menu and tooltip.
pub fn update(devices: &[api::Device]) {
    TRAY.with(|t| {
        if let Some(icon) = t.borrow().as_ref() {
            icon.set_menu(Some(Box::new(menu(devices))));
            let tip = devices
                .iter()
                .map(|d| match d.battery.as_ref().and_then(|b| b.percent) {
                    Some(p) => format!("{}: {p}%", d.name),
                    None => d.name.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = icon.set_tooltip(Some(if tip.is_empty() {
                "Inputsmith".to_string()
            } else {
                format!("Inputsmith\n{tip}")
            }));
        }
    });
}

static EVENTS: OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<TrayEvent>>> = OnceLock::new();

/// Opens (or focuses) the window as if the tray icon was clicked; callable from any thread.
pub fn request_open() {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.lock().unwrap().send(TrayEvent::Open);
    }
}

fn stream() -> impl Stream<Item = TrayEvent> {
    iced::stream::channel(16, async |mut tx| {
        use futures::SinkExt;
        let (std_tx, std_rx) = std::sync::mpsc::channel::<TrayEvent>();
        let _ = EVENTS.set(std::sync::Mutex::new(std_tx.clone()));
        static INSTALLED: OnceLock<()> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            let t = std_tx.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let ev = match e.id {
                    MenuId(ref id) if id == OPEN_ID => TrayEvent::Open,
                    MenuId(ref id) if id == QUIT_ID => TrayEvent::Quit,
                    _ => return,
                };
                let _ = t.send(ev);
            }));
            let t = std_tx.clone();
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                    let _ = t.send(TrayEvent::Open);
                }
            }));
        });
        std::thread::spawn(move || {
            for ev in std_rx {
                if futures::executor::block_on(tx.send(ev)).is_err() {
                    break;
                }
            }
        });
        futures::future::pending::<()>().await;
    })
}

pub fn subscription() -> Subscription<TrayEvent> {
    Subscription::run(stream)
}

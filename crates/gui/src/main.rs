//! `inputsmith-gui` — settings window and tray icon for the inputsmith agent.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod client;
mod desktop;
mod model;
mod theme;
mod tray;
mod vendor;

use app::App;

/// 32x32 RGBA window icon (same artwork as the tray icon).
pub fn window_icon_rgba() -> Vec<u8> {
    const N: usize = 32;
    let mut px = vec![0u8; N * N * 4];
    let (cx, cy, rx, ry) = (15.5f32, 16.0f32, 9.5f32, 14.0f32);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            let d = ((fx - cx) / rx).powi(2) + ((fy - cy) / ry).powi(2);
            let i = (y * N + x) * 4;
            if d <= 1.0 {
                let wheel = (fx - cx).abs() <= 1.5 && (5.0..=11.0).contains(&fy);
                let c: [u8; 4] = if wheel { [0x00, 0xE0, 0xC4, 0xFF] } else { [0xF2, 0xF2, 0xF2, 0xFF] };
                px[i..i + 4].copy_from_slice(&c);
            }
        }
    }
    px
}

fn main() -> iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if !desktop::claim_single_instance() {
        log::info!("already running; asked the running instance to show its window");
        return Ok(());
    }
    desktop::on_activate(tray::request_open);
    let hidden = std::env::args().any(|a| a == "--hidden");
    iced::daemon(move || App::boot(hidden), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .antialiasing(true)
        .run()
}

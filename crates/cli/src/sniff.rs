//! `inputsmith sniff`: observe another HID++ host (such as Options+).
//!
//! Prints, with millisecond timestamps, every HID++ notification of a
//! receiver's devices (diverted buttons, raw XY with running totals) and, on
//! Windows, every *injected* keystroke and every wheel event (marked injected
//! or physical), so gesture step distances, repeat behaviour and synthesized
//! scrolling of the vendor software can be measured.

use std::sync::OnceLock;
use std::time::Instant;

use inputsmith_hidpp::Channel;
use inputsmith_hidpp::channel;
use inputsmith_hidpp::report::Report;

static START: OnceLock<Instant> = OnceLock::new();

fn ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

pub fn run() -> inputsmith_hidpp::Result<()> {
    ms();
    #[cfg(windows)]
    std::thread::spawn(hook::run);
    let mut rxs = Vec::new();
    for iface in channel::enumerate()? {
        let ch = Channel::open(&iface)?;
        rxs.push((ch.subscribe(), ch));
        eprintln!("listening on {:04x}:{:04x}", iface.vendor_id, iface.product_id);
    }
    let (mut tx, mut ty) = (0i64, 0i64);
    loop {
        for (rx, _) in &rxs {
            while let Ok(r) = rx.try_recv() {
                print_report(&r, &mut tx, &mut ty);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn be16(p: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([p[i], p[i + 1]])
}

fn print_report(r: &Report, tx: &mut i64, ty: &mut i64) {
    // Responses to another host's requests (non-zero software id) are shown
    // raw: they reveal what the other software reads and writes.
    if r.sw_id() != 0 && r.sub_id() < 0x80 {
        println!("{:>7} response sw{:x} {:?}", ms(), r.sw_id(), r);
        return;
    }
    let p = r.params();
    // Function 1 of the special-keys feature is raw XY; the feature index
    // varies per device, so recognise it by shape: any 0x1X event with 4
    // signed BE16 bytes is printed both ways.
    match r.function() {
        0 if r.sub_id() < 0x40 => {
            let cids: Vec<String> =
                (0..4).map(|i| be16(p, i * 2)).filter(|&c| c != 0).map(|c| format!("{c:#x}")).collect();
            *tx = 0;
            *ty = 0;
            println!("{:>7} dev{} feat{:#04x} buttons [{}]", ms(), r.device_index(), r.feature_index(), cids.join(","));
        }
        1 if r.sub_id() < 0x40 => {
            let (dx, dy) = (be16(p, 0) as i16 as i64, be16(p, 2) as i16 as i64);
            *tx += dx;
            *ty += dy;
            println!("{:>7} dev{} rawxy {dx:>4},{dy:>4}  total {tx:>5},{ty:>5}", ms(), r.device_index());
        }
        _ => println!("{:>7} {:?}", ms(), r),
    }
}

#[cfg(windows)]
mod hook {
    use super::ms;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
        SetWindowsHookExW, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
        WM_SYSKEYDOWN,
    };

    unsafe extern "system" fn proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            let k = unsafe { &*(l as *const KBDLLHOOKSTRUCT) };
            if k.flags & LLKHF_INJECTED != 0 {
                let down = w == WM_KEYDOWN as usize || w == WM_SYSKEYDOWN as usize;
                println!("{:>7} injected key vk={:#04x} {}", ms(), k.vkCode, if down { "down" } else { "up" });
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
    }

    unsafe extern "system" fn mouse_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 && w != WM_MOUSEMOVE as usize {
            let m = unsafe { &*(l as *const MSLLHOOKSTRUCT) };
            let injected = m.flags & LLMHF_INJECTED != 0;
            let src = if injected { "injected" } else { "physical" };
            let delta = (m.mouseData >> 16) as u16 as i16;
            match w as u32 {
                WM_MOUSEWHEEL => println!("{:>7} {src} wheel {delta}", ms()),
                WM_MOUSEHWHEEL => println!("{:>7} {src} hwheel {delta}", ms()),
                msg if injected => println!("{:>7} injected mouse msg {msg:#x}", ms()),
                _ => {}
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
    }

    pub fn run() {
        unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            let h = SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), module, 0);
            if h.is_null() {
                eprintln!("keyboard hook failed");
                return;
            }
            if SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0).is_null() {
                eprintln!("mouse hook failed");
            }
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        }
    }
}

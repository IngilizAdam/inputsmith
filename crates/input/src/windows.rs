//! Windows backend: `SendInput`, foreground window process, `ShellExecuteW`.

use std::mem::size_of;
use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    MAPVK_VK_TO_VSC, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW, SendInput,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowTextW, GetWindowThreadProcessId, SM_SWAPBUTTON,
    SPI_SETMOUSEBUTTONSWAP, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SW_SHOWNORMAL, SystemParametersInfoW,
};

use crate::{Error, ForegroundApp, InputSink, MediaKey, MouseButton, Result};

const XBUTTON1: u32 = 0x0001;
const XBUTTON2: u32 = 0x0002;

/// Maps a HID keyboard usage to `(virtual key, extended)`.
pub fn usage_to_vk(usage: u16) -> Option<(u16, bool)> {
    let vk = match usage {
        0x04..=0x1D => 0x41 + (usage - 0x04), // A..Z
        0x1E..=0x26 => 0x31 + (usage - 0x1E), // 1..9
        0x27 => 0x30,                         // 0
        0x28 => 0x0D,                         // Enter
        0x29 => 0x1B,                         // Escape
        0x2A => 0x08,                         // Backspace
        0x2B => 0x09,                         // Tab
        0x2C => 0x20,                         // Space
        0x2D => 0xBD,                         // -
        0x2E => 0xBB,                         // =
        0x2F => 0xDB,                         // [
        0x30 => 0xDD,                         // ]
        0x31 | 0x32 => 0xDC,                  // \ and non-US #
        0x33 => 0xBA,                         // ;
        0x34 => 0xDE,                         // '
        0x35 => 0xC0,                         // `
        0x36 => 0xBC,                         // ,
        0x37 => 0xBE,                         // .
        0x38 => 0xBF,                         // /
        0x39 => 0x14,                         // Caps Lock
        0x3A..=0x45 => 0x70 + (usage - 0x3A), // F1..F12
        0x46 => return Some((0x2C, true)),    // Print Screen
        0x47 => 0x91,                         // Scroll Lock
        0x48 => 0x13,                         // Pause
        0x49 => return Some((0x2D, true)),    // Insert
        0x4A => return Some((0x24, true)),    // Home
        0x4B => return Some((0x21, true)),    // Page Up
        0x4C => return Some((0x2E, true)),    // Delete
        0x4D => return Some((0x23, true)),    // End
        0x4E => return Some((0x22, true)),    // Page Down
        0x4F => return Some((0x27, true)),    // Right
        0x50 => return Some((0x25, true)),    // Left
        0x51 => return Some((0x28, true)),    // Down
        0x52 => return Some((0x26, true)),    // Up
        0x53 => return Some((0x90, true)),    // Num Lock
        0x54 => return Some((0x6F, true)),    // Keypad /
        0x55 => 0x6A,                         // Keypad *
        0x56 => 0x6D,                         // Keypad -
        0x57 => 0x6B,                         // Keypad +
        0x58 => return Some((0x0D, true)),    // Keypad Enter
        0x59..=0x61 => 0x61 + (usage - 0x59), // Keypad 1..9
        0x62 => 0x60,                         // Keypad 0
        0x63 => 0x6E,                         // Keypad .
        0x64 => 0xE2,                         // non-US \
        0x65 => return Some((0x5D, true)),    // Application
        0x68..=0x73 => 0x7C + (usage - 0x68), // F13..F24
        0x7F => 0xAD,                         // Mute
        0x80 => 0xAF,                         // Volume Up
        0x81 => 0xAE,                         // Volume Down
        0xE0 => 0xA2,                         // Left Ctrl
        0xE1 => 0xA0,                         // Left Shift
        0xE2 => 0xA4,                         // Left Alt
        0xE3 => return Some((0x5B, true)),    // Left GUI
        0xE4 => return Some((0xA3, true)),    // Right Ctrl
        0xE5 => 0xA1,                         // Right Shift
        0xE6 => return Some((0xA5, true)),    // Right Alt
        0xE7 => return Some((0x5C, true)),    // Right GUI
        _ => return None,
    };
    Some((vk, false))
}

fn send(inputs: &[INPUT]) -> Result<()> {
    let n = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    if n as usize != inputs.len() {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn key_input(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn mouse_input(flags: u32, data: i32, dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn vk_event(vk: u16, extended: bool, down: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    key_input(vk, scan, flags)
}

pub struct SendInputSink;

impl InputSink for SendInputSink {
    fn key(&self, usage: u16, down: bool) -> Result<()> {
        let (vk, ext) = usage_to_vk(usage).ok_or(Error::UnmappedKey(usage))?;
        send(&[vk_event(vk, ext, down)])
    }

    fn mouse_button(&self, button: MouseButton, down: bool) -> Result<()> {
        let (flags, data) = match (button, down) {
            (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
            (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
            (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
            (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
            (MouseButton::Back, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
            (MouseButton::Back, false) => (MOUSEEVENTF_XUP, XBUTTON1),
            (MouseButton::Forward, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
            (MouseButton::Forward, false) => (MOUSEEVENTF_XUP, XBUTTON2),
        };
        send(&[mouse_input(flags, data as i32, 0, 0)])
    }

    fn wheel(&self, vertical: i32, horizontal: i32) -> Result<()> {
        let mut v = Vec::new();
        if vertical != 0 {
            v.push(mouse_input(MOUSEEVENTF_WHEEL, vertical, 0, 0));
        }
        if horizontal != 0 {
            v.push(mouse_input(MOUSEEVENTF_HWHEEL, horizontal, 0, 0));
        }
        if v.is_empty() { Ok(()) } else { send(&v) }
    }

    fn move_relative(&self, dx: i32, dy: i32) -> Result<()> {
        send(&[mouse_input(MOUSEEVENTF_MOVE, 0, dx, dy)])
    }

    fn media(&self, key: MediaKey) -> Result<()> {
        let vk = match key {
            MediaKey::PlayPause => 0xB3,
            MediaKey::Stop => 0xB2,
            MediaKey::VolumeUp => 0xAF,
            MediaKey::VolumeDown => 0xAE,
            MediaKey::NextTrack => 0xB0,
            MediaKey::PreviousTrack => 0xB1,
            MediaKey::Mute => 0xAD,
        };
        send(&[vk_event(vk, true, true), vk_event(vk, true, false)])
    }

    fn text(&self, text: &str) -> Result<()> {
        let mut v = Vec::new();
        for unit in text.encode_utf16() {
            v.push(key_input(0, unit, KEYEVENTF_UNICODE));
            v.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
        if v.is_empty() { Ok(()) } else { send(&v) }
    }
}

pub fn foreground_app() -> Option<ForegroundApp> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut title = [0u16; 512];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        Some(ForegroundApp {
            pid,
            exe_path: String::from_utf16_lossy(&buf[..len as usize]).to_lowercase(),
            window_title: String::from_utf16_lossy(&title[..n]),
        })
    }
}

pub fn buttons_swapped() -> bool {
    unsafe { GetSystemMetrics(SM_SWAPBUTTON) != 0 }
}

pub fn set_buttons_swapped(swapped: bool) -> Result<()> {
    // Same effect as the Control Panel switch: live, written to the user
    // profile (SwapMouseButtons) and broadcast to running applications.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETMOUSEBUTTONSWAP,
            swapped as u32,
            std::ptr::null_mut(),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn shell_open(target: &str) -> Result<()> {
    let wide: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), wide.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    // ShellExecute returns a value > 32 on success.
    if (r as isize) <= 32 {
        return Err(std::io::Error::other(format!("ShellExecuteW failed ({})", r as isize)).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_usages() {
        assert_eq!(usage_to_vk(0x06), Some((0x43, false))); // C
        assert_eq!(usage_to_vk(0x2B), Some((0x09, false))); // Tab
        assert_eq!(usage_to_vk(0x4F), Some((0x27, true))); // Right arrow, extended
        assert_eq!(usage_to_vk(0xE3), Some((0x5B, true))); // Left Win
        assert_eq!(usage_to_vk(0x3A), Some((0x70, false))); // F1
        assert_eq!(usage_to_vk(0x00), None);
    }
}

//! HID keyboard usage constants (usage page 0x07) used by action cards.

pub const A: u16 = 0x04;
pub const D: u16 = 0x07;
pub const L: u16 = 0x0F;
pub const ENTER: u16 = 0x28;
pub const ESCAPE: u16 = 0x29;
pub const BACKSPACE: u16 = 0x2A;
pub const TAB: u16 = 0x2B;
pub const SPACE: u16 = 0x2C;
pub const F4: u16 = 0x3D;
pub const PRINT_SCREEN: u16 = 0x46;
pub const RIGHT: u16 = 0x4F;
pub const LEFT: u16 = 0x50;
pub const DOWN: u16 = 0x51;
pub const UP: u16 = 0x52;
pub const APPLICATION: u16 = 0x65;

pub const LEFT_CTRL: u16 = 0xE0;
pub const LEFT_SHIFT: u16 = 0xE1;
pub const LEFT_ALT: u16 = 0xE2;
pub const LEFT_GUI: u16 = 0xE3;
pub const RIGHT_CTRL: u16 = 0xE4;
pub const RIGHT_SHIFT: u16 = 0xE5;
pub const RIGHT_ALT: u16 = 0xE6;
pub const RIGHT_GUI: u16 = 0xE7;

/// HID usage of a letter `a..=z` (case-insensitive).
pub const fn letter(c: u8) -> u16 {
    (c.to_ascii_lowercase() - b'a') as u16 + A
}

pub fn is_modifier(usage: u16) -> bool {
    (LEFT_CTRL..=RIGHT_GUI).contains(&usage)
}

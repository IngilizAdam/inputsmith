//! Typed wrappers over HID++ 2.0 features.
//!
//! Each module exposes a zero-cost view borrowing a [`Device`](crate::Device).

pub mod battery;
pub mod change_host;
pub mod config_change;
pub mod device_info;
pub mod dpi;
pub mod fn_inversion;
pub mod reprog_controls;
pub mod wheel;

pub(crate) fn be16(p: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([p[at], p[at + 1]])
}

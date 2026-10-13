//! 0x19B0 Haptic Feedback (MX Master 4). Implemented from public HID++ knowledge; untested on
//! hardware.

use crate::Device;
use crate::error::Result;
use crate::feature_id::HAPTIC_FEEDBACK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HapticConfig {
    /// Configuration flags; bit 0 = haptics enabled. Other bits are kept as read.
    pub flags: u8,
    /// Strength 0–100.
    pub level: u8,
    /// The device only knows four levels (25, 50, 75, 100).
    pub four_levels: bool,
}

impl HapticConfig {
    pub fn enabled(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

pub struct Haptics<'a>(pub &'a Device);

impl Haptics<'_> {
    pub fn config(&self) -> Result<HapticConfig> {
        let p = self.0.call(HAPTIC_FEEDBACK, 1, &[])?;
        Ok(HapticConfig { flags: p[0], level: p[1], four_levels: p[2] & 0x01 != 0 })
    }

    /// Enables or disables haptics at `level` (0–100), keeping the other configuration flags.
    pub fn set(&self, current: &HapticConfig, enabled: bool, level: u8) -> Result<()> {
        let flags = (current.flags & !0x01) | enabled as u8;
        let level = if current.four_levels { snap_to_four(level) } else { level.min(100) };
        self.0.call(HAPTIC_FEEDBACK, 2, &[flags, level])?;
        Ok(())
    }

    /// Plays one of the device's built-in waveforms (0 = sharp state change, …).
    pub fn play(&self, waveform: u8) -> Result<()> {
        self.0.call(HAPTIC_FEEDBACK, 4, &[waveform])?;
        Ok(())
    }
}

/// Nearest of 25/50/75/100 for devices with four levels.
fn snap_to_four(level: u8) -> u8 {
    (((level.clamp(13, 100) as u16 + 12) / 25 * 25) as u8).clamp(25, 100)
}

#[cfg(test)]
mod tests {
    #[test]
    fn four_level_devices_get_the_nearest_level() {
        assert_eq!(super::snap_to_four(0), 25);
        assert_eq!(super::snap_to_four(60), 50);
        assert_eq!(super::snap_to_four(63), 75);
        assert_eq!(super::snap_to_four(100), 100);
    }
}

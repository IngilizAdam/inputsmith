//! 0x2110 / 0x2111 SmartShift, 0x2121 Hi-res wheel, 0x2150 Thumbwheel.

use super::be16;
use crate::Device;
use crate::error::{Error, Result};
use crate::feature_id::{HIRES_WHEEL, SMART_SHIFT, SMART_SHIFT_TUNABLE, THUMBWHEEL};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelMode {
    FreeSpin,
    Ratchet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmartShiftState {
    pub mode: WheelMode,
    /// Speed threshold that disengages the ratchet (1..=254; 255 = never).
    pub auto_disengage: u8,
    pub auto_disengage_default: u8,
    /// Ratchet force (0x2111 only).
    pub torque: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmartShiftCapabilities {
    pub tunable_torque: bool,
    pub auto_disengage_default: u8,
    pub default_torque: u8,
    pub max_force: u8,
}

pub struct SmartShift<'a>(pub &'a Device);

fn mode(b: u8) -> WheelMode {
    if b == 2 { WheelMode::Ratchet } else { WheelMode::FreeSpin }
}

fn mode_byte(m: Option<WheelMode>) -> u8 {
    match m {
        None => 0,
        Some(WheelMode::FreeSpin) => 1,
        Some(WheelMode::Ratchet) => 2,
    }
}

impl SmartShift<'_> {
    fn tunable(&self) -> Result<bool> {
        if self.0.has_feature(SMART_SHIFT_TUNABLE) {
            Ok(true)
        } else if self.0.has_feature(SMART_SHIFT) {
            Ok(false)
        } else {
            Err(Error::FeatureNotSupported(SMART_SHIFT))
        }
    }

    pub fn capabilities(&self) -> Result<Option<SmartShiftCapabilities>> {
        if !self.tunable()? {
            return Ok(None);
        }
        let p = self.0.call(SMART_SHIFT_TUNABLE, 0, &[])?;
        Ok(Some(SmartShiftCapabilities {
            tunable_torque: p[0] & 0x01 != 0,
            auto_disengage_default: p[1],
            default_torque: p[2],
            max_force: p[3],
        }))
    }

    pub fn get(&self) -> Result<SmartShiftState> {
        if self.tunable()? {
            let caps = self.0.call(SMART_SHIFT_TUNABLE, 0, &[])?;
            let p = self.0.call(SMART_SHIFT_TUNABLE, 1, &[])?;
            Ok(SmartShiftState {
                mode: mode(p[0]),
                auto_disengage: p[1],
                auto_disengage_default: caps[1],
                torque: Some(p[2]),
            })
        } else {
            let p = self.0.call(SMART_SHIFT, 0, &[])?;
            Ok(SmartShiftState { mode: mode(p[0]), auto_disengage: p[1], auto_disengage_default: p[2], torque: None })
        }
    }

    /// Updates mode/threshold/torque; `None` fields are left unchanged.
    pub fn set(&self, wheel_mode: Option<WheelMode>, auto_disengage: Option<u8>, torque: Option<u8>) -> Result<()> {
        let m = mode_byte(wheel_mode);
        let a = auto_disengage.unwrap_or(0);
        if self.tunable()? {
            self.0.call(SMART_SHIFT_TUNABLE, 2, &[m, a, torque.unwrap_or(0)])?;
        } else {
            self.0.call(SMART_SHIFT, 1, &[m, a, 0])?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HiResCapabilities {
    pub multiplier: u8,
    pub has_invert: bool,
    pub has_ratchet_switch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HiResMode {
    /// Wheel events are diverted to HID++ notifications instead of HID.
    pub diverted: bool,
    pub high_resolution: bool,
    pub inverted: bool,
}

impl HiResMode {
    fn to_byte(self) -> u8 {
        self.diverted as u8 | (self.high_resolution as u8) << 1 | (self.inverted as u8) << 2
    }
    fn from_byte(b: u8) -> Self {
        Self { diverted: b & 1 != 0, high_resolution: b & 2 != 0, inverted: b & 4 != 0 }
    }
}

pub struct HiResWheel<'a>(pub &'a Device);

impl HiResWheel<'_> {
    pub fn capabilities(&self) -> Result<HiResCapabilities> {
        let p = self.0.call(HIRES_WHEEL, 0, &[])?;
        Ok(HiResCapabilities { multiplier: p[0], has_invert: p[1] & 0x08 != 0, has_ratchet_switch: p[1] & 0x04 != 0 })
    }

    pub fn mode(&self) -> Result<HiResMode> {
        Ok(HiResMode::from_byte(self.0.call(HIRES_WHEEL, 1, &[])?[0]))
    }

    pub fn set_mode(&self, mode: HiResMode) -> Result<()> {
        self.0.call(HIRES_WHEEL, 2, &[mode.to_byte()])?;
        Ok(())
    }

    /// Current physical ratchet state for wheels with a mode switch.
    pub fn ratchet_state(&self) -> Result<WheelMode> {
        Ok(if self.0.call(HIRES_WHEEL, 3, &[])?[0] & 1 != 0 { WheelMode::Ratchet } else { WheelMode::FreeSpin })
    }
}

/// Wheel movement reported while the wheel is diverted (0x2121 event 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelMovement {
    /// Signed movement, positive = up / away from the user. In ratchet notches
    /// unless `high_resolution`, then in 1/multiplier notches.
    pub delta: i16,
    pub high_resolution: bool,
    /// Number of sampling periods the movement was accumulated over.
    pub periods: u8,
}

/// Parses a 0x2121 wheel-movement event, given the feature's index on the device.
pub fn parse_wheel_movement(feature_index: u8, r: &crate::report::Report) -> Option<WheelMovement> {
    if r.feature_index() != feature_index || r.sw_id() != 0 || r.function() != 0 {
        return None;
    }
    let p = r.params();
    Some(WheelMovement { delta: be16(p, 1) as i16, high_resolution: p[0] & 0x10 != 0, periods: p[0] & 0x0f })
}

/// Thumb wheel rotation reported while the thumb wheel is diverted (0x2150 event 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbwheelMovement {
    /// Signed rotation in diverted-resolution units; positive is towards the front of the mouse.
    pub rotation: i16,
    /// 0 inactive, 1 start, 2 active, 3 stop.
    pub state: u8,
}

/// Parses a 0x2150 rotation event, given the feature's index on the device.
pub fn parse_thumbwheel_movement(feature_index: u8, r: &crate::report::Report) -> Option<ThumbwheelMovement> {
    if r.feature_index() != feature_index || r.sw_id() != 0 || r.function() != 0 {
        return None;
    }
    let p = r.params();
    Some(ThumbwheelMovement { rotation: be16(p, 0) as i16, state: p[4] })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbwheelInfo {
    pub native_resolution: u16,
    pub diverted_resolution: u16,
    pub default_direction_inverted: bool,
    pub has_timestamp: bool,
    pub has_touch: bool,
    pub has_proximity: bool,
    pub has_single_tap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbwheelStatus {
    pub diverted: bool,
    pub inverted: bool,
}

pub struct Thumbwheel<'a>(pub &'a Device);

impl Thumbwheel<'_> {
    pub fn info(&self) -> Result<ThumbwheelInfo> {
        let p = self.0.call(THUMBWHEEL, 0, &[])?;
        Ok(ThumbwheelInfo {
            native_resolution: be16(&p, 0),
            diverted_resolution: be16(&p, 2),
            default_direction_inverted: p[4] & 0x01 != 0,
            has_timestamp: p[5] & 0x01 != 0,
            has_touch: p[5] & 0x02 != 0,
            has_proximity: p[5] & 0x04 != 0,
            has_single_tap: p[5] & 0x08 != 0,
        })
    }

    pub fn status(&self) -> Result<ThumbwheelStatus> {
        let p = self.0.call(THUMBWHEEL, 1, &[])?;
        Ok(ThumbwheelStatus { diverted: p[0] & 1 != 0, inverted: p[1] & 1 != 0 })
    }

    pub fn set(&self, status: ThumbwheelStatus) -> Result<()> {
        self.0.call(THUMBWHEEL, 2, &[status.diverted as u8, status.inverted as u8])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Report;

    #[test]
    fn parses_wheel_movement() {
        // Captured from an MX Anywhere 3 with the wheel diverted (docs/BEHAVIOR.md).
        let mut b = [0u8; 20];
        b[..7].copy_from_slice(&[0x11, 0x01, 0x0e, 0x00, 0x02, 0xff, 0xf6]);
        let r = Report::parse(&b).unwrap();
        assert_eq!(
            parse_wheel_movement(0x0e, &r),
            Some(WheelMovement { delta: -10, high_resolution: false, periods: 2 })
        );
        assert_eq!(parse_wheel_movement(0x0d, &r), None);
    }

    #[test]
    fn parses_thumbwheel_movement() {
        // Layout as documented by Solaar: rotation (BE16), timestamp, state, flags.
        let mut b = [0u8; 20];
        b[..9].copy_from_slice(&[0x11, 0x01, 0x10, 0x00, 0xff, 0xfd, 0x12, 0x34, 0x02]);
        let r = Report::parse(&b).unwrap();
        assert_eq!(parse_thumbwheel_movement(0x10, &r), Some(ThumbwheelMovement { rotation: -3, state: 2 }));
    }
}

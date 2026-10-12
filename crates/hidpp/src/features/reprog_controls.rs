//! 0x1B04 Special Keys & Mouse Buttons (reprogrammable controls v4).
//!
//! Lists the device's controls (CIDs) and lets software "divert" them: a
//! diverted control no longer produces its native HID event but a HID++
//! notification, which the host turns into whatever action is assigned.

use super::be16;
use crate::Device;
use crate::error::Result;
use crate::feature_id::SPECIAL_KEYS_1B04;
use crate::report::Report;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlFlags(pub u16);

impl ControlFlags {
    pub fn mouse_button(self) -> bool {
        self.0 & 0x0001 != 0
    }
    pub fn fn_key(self) -> bool {
        self.0 & 0x0002 != 0
    }
    pub fn nonstandard(self) -> bool {
        self.0 & 0x0004 != 0
    }
    pub fn fn_sensitive(self) -> bool {
        self.0 & 0x0008 != 0
    }
    pub fn reprogrammable(self) -> bool {
        self.0 & 0x0010 != 0
    }
    pub fn divertable(self) -> bool {
        self.0 & 0x0020 != 0
    }
    pub fn persistently_divertable(self) -> bool {
        self.0 & 0x0040 != 0
    }
    pub fn virtual_control(self) -> bool {
        self.0 & 0x0080 != 0
    }
    pub fn raw_xy(self) -> bool {
        self.0 & 0x0100 != 0
    }
    pub fn force_raw_xy(self) -> bool {
        self.0 & 0x0200 != 0
    }
    pub fn analytics_key_events(self) -> bool {
        self.0 & 0x0400 != 0
    }
    pub fn raw_wheel(self) -> bool {
        self.0 & 0x0800 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlInfo {
    pub cid: u16,
    /// Task ID: the control's native function.
    pub tid: u16,
    pub flags: ControlFlags,
    /// Position on the device (F-key number for function keys), 0 if not applicable.
    pub position: u8,
    pub group: u8,
    /// Groups this control can be remapped to.
    pub group_mask: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reporting {
    pub diverted: bool,
    pub persistently_diverted: bool,
    pub raw_xy: bool,
    pub force_raw_xy: bool,
    pub analytics_key_events: bool,
    pub raw_wheel: bool,
    /// CID this control is remapped to (0 = not remapped / itself).
    pub remapped: u16,
}

/// Fields to change with [`ReprogControls::update_reporting`]; `None` leaves a field as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReportingUpdate {
    pub divert: Option<bool>,
    pub raw_xy: Option<bool>,
    /// Report wheel turns as [`ControlEvent::RawWheel`] while the control is held
    /// (controls with [`ControlFlags::raw_wheel`]; used for "hold + wheel" horizontal scrolling).
    pub raw_wheel: Option<bool>,
    pub analytics_key_events: Option<bool>,
}

impl ReportingUpdate {
    /// The flags byte and the extra flags byte of `setCidReporting`.
    fn bytes(self) -> (u8, u8) {
        // Each field has a value bit and a "valid" bit right above it.
        fn bits(v: Option<bool>, shift: u8) -> u8 {
            v.map_or(0, |on| (0b10 | on as u8) << shift)
        }
        (bits(self.divert, 0) | bits(self.raw_xy, 4), bits(self.analytics_key_events, 0) | bits(self.raw_wheel, 2))
    }
}

pub struct ReprogControls<'a>(pub &'a Device);

impl ReprogControls<'_> {
    pub fn count(&self) -> Result<u8> {
        Ok(self.0.call(SPECIAL_KEYS_1B04, 0, &[])?[0])
    }

    pub fn control(&self, index: u8) -> Result<ControlInfo> {
        let p = self.0.call(SPECIAL_KEYS_1B04, 1, &[index])?;
        Ok(ControlInfo {
            cid: be16(&p, 0),
            tid: be16(&p, 2),
            flags: ControlFlags(p[4] as u16 | (p[8] as u16) << 8),
            position: p[5],
            group: p[6],
            group_mask: p[7],
        })
    }

    pub fn controls(&self) -> Result<Vec<ControlInfo>> {
        (0..self.count()?).map(|i| self.control(i)).collect()
    }

    pub fn reporting(&self, cid: u16) -> Result<Reporting> {
        let c = cid.to_be_bytes();
        let p = self.0.call(SPECIAL_KEYS_1B04, 2, &[c[0], c[1]])?;
        Ok(Reporting {
            diverted: p[2] & 0x01 != 0,
            persistently_diverted: p[2] & 0x04 != 0,
            raw_xy: p[2] & 0x10 != 0,
            force_raw_xy: p[2] & 0x40 != 0,
            remapped: be16(&p, 3),
            analytics_key_events: p[5] & 0x01 != 0,
            raw_wheel: p[5] & 0x04 != 0,
        })
    }

    /// Sets temporary diversion (and optionally raw XY) for a control. Only the
    /// "valid" bits for the provided fields are set, so other state is untouched.
    pub fn set_diversion(&self, cid: u16, divert: bool, raw_xy: Option<bool>) -> Result<()> {
        self.update_reporting(cid, ReportingUpdate { divert: Some(divert), raw_xy, ..Default::default() })
    }

    /// Changes the reporting fields that are `Some`; the others keep their state.
    pub fn update_reporting(&self, cid: u16, u: ReportingUpdate) -> Result<()> {
        let c = cid.to_be_bytes();
        let (flags, extra) = u.bytes();
        self.0.call(SPECIAL_KEYS_1B04, 3, &[c[0], c[1], flags, 0, 0, extra])?;
        Ok(())
    }

    /// Remaps `cid` to behave as `target` (0 restores the default).
    pub fn remap(&self, cid: u16, target: u16) -> Result<()> {
        let c = cid.to_be_bytes();
        let t = target.to_be_bytes();
        self.0.call(SPECIAL_KEYS_1B04, 3, &[c[0], c[1], 0, t[0], t[1], 0])?;
        Ok(())
    }
}

/// Events emitted for diverted controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlEvent {
    /// Currently pressed diverted controls (up to four; empty = all released).
    Buttons(Vec<u16>),
    /// Raw pointer motion while a raw-XY diverted control is held.
    RawXY { dx: i16, dy: i16 },
    /// Wheel movement while a raw-wheel control is held (positive = up / away from the user).
    RawWheel { delta: i16 },
}

/// Parses a 0x1B04 event, given the feature's index on the device.
pub fn parse_event(feature_index: u8, r: &Report) -> Option<ControlEvent> {
    if r.feature_index() != feature_index || r.sw_id() != 0 {
        return None;
    }
    let p = r.params();
    match r.function() {
        0 => Some(ControlEvent::Buttons((0..4).map(|i| be16(p, i * 2)).filter(|&c| c != 0).collect())),
        1 => Some(ControlEvent::RawXY { dx: be16(p, 0) as i16, dy: be16(p, 2) as i16 }),
        // Function 2 = analytics key events (cid, event pairs), not used.
        // Function 4: byte 0 is 0x01 in every observed report, bytes 1-2 the signed delta.
        4 => Some(ControlEvent::RawWheel { delta: be16(p, 1) as i16 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporting_bytes_match_options_plus() {
        let u = |divert, raw_xy, raw_wheel| ReportingUpdate {
            divert: Some(divert),
            raw_xy: Some(raw_xy),
            raw_wheel,
            analytics_key_events: Some(true),
        };
        // Values Options+ was observed to send (docs/BEHAVIOR.md).
        assert_eq!(u(true, false, Some(true)).bytes(), (0x23, 0x0f)); // horizontal scroll button
        assert_eq!(u(false, false, Some(false)).bytes(), (0x22, 0x0b)); // reset on reconnect
        assert_eq!(u(true, true, None).bytes(), (0x33, 0x03)); // gesture button
        assert_eq!(ReportingUpdate::default().bytes(), (0, 0));
    }

    #[test]
    fn parses_raw_wheel_event() {
        let r =
            Report::parse(&[0x11, 0x01, 0x09, 0x40, 0x01, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(parse_event(0x09, &r), Some(ControlEvent::RawWheel { delta: -1 }));
    }
}

/// Well-known control IDs (as used in the vendor device database's
/// `specialKeys.programmable` lists).
pub fn control_name(cid: u16) -> Option<&'static str> {
    Some(match cid {
        0x0050 => "Left Button",
        0x0051 => "Right Button",
        0x0052 => "Middle Button",
        0x0053 => "Back",
        0x0056 => "Forward",
        0x005B => "Left Tilt / Scroll Left",
        0x005D => "Right Tilt / Scroll Right",
        0x00C3 => "Gesture Button",
        0x00C4 => "Smart Shift",
        0x00D7 => "Virtual Gesture Button",
        0x00FD => "DPI Switch",
        // TODO: fill from the vendor control-name strings once mapped.
        _ => return None,
    })
}

//! 0x1000 Battery Level Status, 0x1004 Unified Battery.

use crate::Device;
use crate::error::{Error, Result};
use crate::feature_id::{BATTERY_LEVEL_STATUS, UNIFIED_BATTERY};
use crate::report::Report;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargingStatus {
    Discharging,
    Charging,
    ChargingSlow,
    Full,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryLevel {
    Critical,
    Low,
    Good,
    Full,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    /// State of charge in percent when the device reports it.
    pub percent: Option<u8>,
    pub level: BatteryLevel,
    pub charging: ChargingStatus,
    pub external_power: bool,
}

fn level_from_mask(mask: u8) -> BatteryLevel {
    if mask & 0x08 != 0 {
        BatteryLevel::Full
    } else if mask & 0x04 != 0 {
        BatteryLevel::Good
    } else if mask & 0x02 != 0 {
        BatteryLevel::Low
    } else if mask & 0x01 != 0 {
        BatteryLevel::Critical
    } else {
        BatteryLevel::Unknown
    }
}

fn level_from_percent(p: u8) -> BatteryLevel {
    match p {
        0..=5 => BatteryLevel::Critical,
        6..=20 => BatteryLevel::Low,
        21..=80 => BatteryLevel::Good,
        _ => BatteryLevel::Full,
    }
}

/// Parses a 0x1004 getStatus response or battery event.
pub fn parse_unified(p: &[u8], soc_supported: bool) -> BatteryStatus {
    BatteryStatus {
        percent: soc_supported.then_some(p[0]),
        level: level_from_mask(p[1]),
        charging: match p[2] {
            0 => ChargingStatus::Discharging,
            1 => ChargingStatus::Charging,
            2 => ChargingStatus::ChargingSlow,
            3 => ChargingStatus::Full,
            _ => ChargingStatus::Error,
        },
        external_power: p[3] != 0,
    }
}

/// Parses a 0x1000 getBatteryLevelStatus response or event.
pub fn parse_level_status(p: &[u8]) -> BatteryStatus {
    let percent = p[0];
    BatteryStatus {
        percent: (percent != 0).then_some(percent),
        level: level_from_percent(percent),
        charging: match p[2] {
            0 => ChargingStatus::Discharging,
            1 | 4 => ChargingStatus::Charging,
            2 | 3 => ChargingStatus::Full,
            _ => ChargingStatus::Error,
        },
        external_power: matches!(p[2], 1..=4),
    }
}

pub struct Battery<'a>(pub &'a Device);

impl Battery<'_> {
    /// Reads the battery through whichever battery feature the device has.
    pub fn status(&self) -> Result<BatteryStatus> {
        if self.0.has_feature(UNIFIED_BATTERY) {
            let caps = self.0.call(UNIFIED_BATTERY, 0, &[])?;
            let soc = caps[1] & 0x02 != 0;
            let p = self.0.call(UNIFIED_BATTERY, 1, &[])?;
            Ok(parse_unified(&p, soc))
        } else if self.0.has_feature(BATTERY_LEVEL_STATUS) {
            Ok(parse_level_status(&self.0.call(BATTERY_LEVEL_STATUS, 0, &[])?))
        } else {
            Err(Error::FeatureNotSupported(UNIFIED_BATTERY))
        }
    }

    /// Decodes a battery broadcast event from this device, if `r` is one.
    pub fn parse_event(&self, r: &Report) -> Option<BatteryStatus> {
        if r.device_index() != self.0.index() || r.function() != 0 || r.sw_id() != 0 {
            return None;
        }
        let f = self.0.features().into_iter().find(|f| f.index == r.feature_index())?;
        match f.id {
            UNIFIED_BATTERY => Some(parse_unified(r.params(), true)),
            BATTERY_LEVEL_STATUS => Some(parse_level_status(r.params())),
            _ => None,
        }
    }
}

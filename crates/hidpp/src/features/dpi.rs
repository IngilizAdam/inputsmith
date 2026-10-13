//! 0x2201 Adjustable DPI, 0x2202 Extended Adjustable DPI, 0x2205 Pointer Motion Scaling.

use super::be16;
use crate::Device;
use crate::error::Result;
use crate::feature_id::{ADJUSTABLE_DPI, EXTENDED_ADJUSTABLE_DPI, POINTER_MOTION_SCALING};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpiList {
    /// Discrete supported values.
    Values(Vec<u16>),
    /// Inclusive range with a step.
    Range { min: u16, max: u16, step: u16 },
}

impl DpiList {
    /// Parses the BE16 word list of getSensorDpiList. A word `0xE000 | step`
    /// between two values means "every `step` from previous to next".
    pub fn parse(words: &[u16]) -> Self {
        let words: Vec<u16> = words.iter().copied().take_while(|&w| w != 0).collect();
        if let Some(pos) = words.iter().position(|&w| w >= 0xE000)
            && pos > 0
            && pos + 1 < words.len()
        {
            return DpiList::Range { min: words[pos - 1], max: words[pos + 1], step: words[pos] & 0x1FFF };
        }
        DpiList::Values(words)
    }

    pub fn contains(&self, dpi: u16) -> bool {
        match self {
            DpiList::Values(v) => v.contains(&dpi),
            DpiList::Range { min, max, step } => {
                dpi >= *min && dpi <= *max && (dpi - min).is_multiple_of(*step.max(&1))
            }
        }
    }
}

pub struct AdjustableDpi<'a>(pub &'a Device);

impl AdjustableDpi<'_> {
    pub fn sensor_count(&self) -> Result<u8> {
        Ok(self.0.call(ADJUSTABLE_DPI, 0, &[])?[0])
    }

    pub fn dpi_list(&self, sensor: u8) -> Result<DpiList> {
        // p[0] echoes the sensor index, followed by BE16 words up to a 0 terminator.
        let p = self.0.call(ADJUSTABLE_DPI, 1, &[sensor])?;
        let words: Vec<u16> = (1..p.len() - 1).step_by(2).map(|i| be16(&p, i)).collect();
        Ok(DpiList::parse(&words))
    }

    /// Returns `(current, default)`.
    pub fn dpi(&self, sensor: u8) -> Result<(u16, u16)> {
        let p = self.0.call(ADJUSTABLE_DPI, 2, &[sensor])?;
        Ok((be16(&p, 1), be16(&p, 3)))
    }

    pub fn set_dpi(&self, sensor: u8, dpi: u16) -> Result<()> {
        let b = dpi.to_be_bytes();
        self.0.call(ADJUSTABLE_DPI, 3, &[sensor, b[0], b[1]])?;
        Ok(())
    }
}

/// Sensor state of 0x2202 getSensorDpi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtendedDpiState {
    pub x: u16,
    pub default_x: u16,
    pub y: u16,
    pub default_y: u16,
    /// Lift-off distance (0 low, 1 medium, 2 high) when supported.
    pub lod: u8,
}

/// 0x2202: like 0x2201, with separate X/Y resolutions and lift-off distance (MX Master 3S and other
/// high-resolution sensors). Implemented from public HID++ knowledge; untested on hardware.
pub struct ExtendedDpi<'a>(pub &'a Device);

impl ExtendedDpi<'_> {
    /// Returns (separate Y resolution, lift-off distance) support for a sensor.
    pub fn capabilities(&self, sensor: u8) -> Result<(bool, bool)> {
        let p = self.0.call(EXTENDED_ADJUSTABLE_DPI, 1, &[sensor])?;
        Ok((p[2] & 0x01 != 0, p[2] & 0x02 != 0))
    }

    /// The X resolutions a sensor supports. The list spans pages; each reply echoes sensor,
    /// direction and page before its BE16 words, and the list ends with a 0 word.
    pub fn dpi_list(&self, sensor: u8) -> Result<DpiList> {
        let mut words = Vec::new();
        for page in 0..=u8::MAX {
            let p = self.0.call(EXTENDED_ADJUSTABLE_DPI, 2, &[sensor, 0, page])?;
            let page_words: Vec<u16> = (3..p.len().saturating_sub(1)).step_by(2).map(|i| be16(&p, i)).collect();
            let end = page_words.contains(&0);
            words.extend(page_words);
            if end {
                break;
            }
        }
        Ok(DpiList::parse(&words))
    }

    pub fn dpi(&self, sensor: u8) -> Result<ExtendedDpiState> {
        let p = self.0.call(EXTENDED_ADJUSTABLE_DPI, 5, &[sensor])?;
        Ok(ExtendedDpiState { x: be16(&p, 1), default_x: be16(&p, 3), y: be16(&p, 5), default_y: be16(&p, 7), lod: p[9] })
    }

    /// Sets X (and Y, when the sensor has a separate Y resolution) to `dpi`, keeping the lift-off
    /// distance.
    pub fn set_dpi(&self, sensor: u8, dpi: u16, separate_y: bool, lod: u8) -> Result<()> {
        let d = dpi.to_be_bytes();
        let y = if separate_y { d } else { [0, 0] };
        self.0.call(EXTENDED_ADJUSTABLE_DPI, 6, &[sensor, d[0], d[1], y[0], y[1], lod])?;
        Ok(())
    }
}

pub struct PointerSpeed<'a>(pub &'a Device);

impl PointerSpeed<'_> {
    /// Speed in 1/256 units (0x0100 = 1.0x).
    pub fn get(&self) -> Result<u16> {
        Ok(be16(&self.0.call(POINTER_MOTION_SCALING, 0, &[])?, 0))
    }

    pub fn set(&self, speed: u16) -> Result<()> {
        self.0.call(POINTER_MOTION_SCALING, 1, &speed.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_range_and_values() {
        assert_eq!(DpiList::parse(&[200, 0xE000 | 50, 4000, 0]), DpiList::Range { min: 200, max: 4000, step: 50 });
        assert_eq!(DpiList::parse(&[400, 800, 1600, 0, 0]), DpiList::Values(vec![400, 800, 1600]));
        assert!(DpiList::Range { min: 200, max: 4000, step: 50 }.contains(1050));
    }
}

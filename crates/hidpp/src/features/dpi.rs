//! 0x2201 Adjustable DPI, 0x2205 Pointer Motion Scaling.

use super::be16;
use crate::Device;
use crate::error::Result;
use crate::feature_id::{ADJUSTABLE_DPI, POINTER_MOTION_SCALING};

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

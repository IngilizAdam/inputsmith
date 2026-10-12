//! 0x40A0 / 0x40A2 / 0x40A3 Fn inversion.

use crate::Device;
use crate::error::{Error, Result};
use crate::feature_id::{FN_INVERSION, FN_INVERSION_2, FN_INVERSION_3};

pub struct FnInversion<'a>(pub &'a Device);

impl FnInversion<'_> {
    fn which(&self) -> Result<u16> {
        [FN_INVERSION_3, FN_INVERSION_2, FN_INVERSION]
            .into_iter()
            .find(|&id| self.0.has_feature(id))
            .ok_or(Error::FeatureNotSupported(FN_INVERSION))
    }

    /// Whether F-keys send their media function by default (inverted).
    pub fn get(&self) -> Result<bool> {
        Ok(match self.which()? {
            FN_INVERSION_3 => self.0.call(FN_INVERSION_3, 0, &[0xFF])?[1] & 0x01 != 0,
            id => self.0.call(id, 0, &[])?[0] & 0x01 != 0,
        })
    }

    pub fn set(&self, inverted: bool) -> Result<()> {
        let v = inverted as u8;
        match self.which()? {
            FN_INVERSION_3 => self.0.call(FN_INVERSION_3, 1, &[0xFF, v])?,
            id => self.0.call(id, 1, &[v])?,
        };
        Ok(())
    }
}

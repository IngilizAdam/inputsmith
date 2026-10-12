//! 0x1814 Change Host (Easy-Switch).

use crate::Device;
use crate::error::Result;
use crate::feature_id::CHANGE_HOST;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostInfo {
    pub host_count: u8,
    /// Zero-based current host.
    pub current: u8,
    pub enhanced_host_switch: bool,
}

pub struct ChangeHost<'a>(pub &'a Device);

impl ChangeHost<'_> {
    pub fn host_info(&self) -> Result<HostInfo> {
        let p = self.0.call(CHANGE_HOST, 0, &[])?;
        Ok(HostInfo { host_count: p[0], current: p[1], enhanced_host_switch: p[2] & 0x01 != 0 })
    }

    /// Switches the device to `host` (zero-based). The device disconnects
    /// immediately, so no response is awaited beyond the send.
    pub fn set_current_host(&self, host: u8) -> Result<()> {
        match self.0.call(CHANGE_HOST, 1, &[host]) {
            Ok(_) | Err(crate::Error::Timeout) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

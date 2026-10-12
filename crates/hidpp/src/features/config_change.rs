//! 0x0020 ConfigChange: a 16-bit "configuration cookie" kept by the device.
//!
//! The cookie is reset (to 0) when the device loses its volatile
//! configuration, e.g. on power loss. Host software writes its own value after
//! configuring the device; finding that value again on reconnect means the
//! configuration is still in place. Options+ writes 0x4825.

use super::be16;
use crate::Device;
use crate::error::Result;
use crate::feature_id::CONFIG_CHANGE;

pub struct ConfigChange<'a>(pub &'a Device);

impl ConfigChange<'_> {
    pub fn cookie(&self) -> Result<u16> {
        Ok(be16(&self.0.call(CONFIG_CHANGE, 0, &[])?, 0))
    }

    pub fn set_cookie(&self, cookie: u16) -> Result<()> {
        self.0.call(CONFIG_CHANGE, 1, &cookie.to_be_bytes())?;
        Ok(())
    }
}

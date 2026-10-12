//! Minimal HID access layer.
//!
//! Only what the HID++ stack needs: enumerate HID collections, open them, and
//! exchange raw reports. Report buffers always include the report ID as the
//! first byte, on every platform.

use std::time::Duration;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("device disconnected")]
    Disconnected,
    #[error("report too large: {len} bytes, collection accepts {max}")]
    ReportTooLarge { len: usize, max: usize },
    #[error("unsupported platform")]
    Unsupported,
}

pub type Result<T> = std::result::Result<T, Error>;

/// One HID top-level collection as exposed by the OS.
///
/// On Windows every top-level collection is a separate device path; on Linux a
/// hidraw node covers the whole interface, so `usage_page`/`usage` describe the
/// first collection and `collections` lists all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    /// USB interface number when known (`&mi_XX` on Windows).
    pub interface_number: Option<u8>,
    /// Max input report length including report ID (0 if unknown).
    pub input_report_len: u16,
    /// Max output report length including report ID (0 if unknown).
    pub output_report_len: u16,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub bus: Bus,
    /// Stable identifier of the parent physical device/interface, used to group
    /// the collections of one interface together.
    pub container_id: String,
    /// All (usage_page, usage) pairs reachable through this path.
    pub collections: Vec<(u16, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Bluetooth,
    Unknown,
}

/// Enumerates HID collections, optionally filtered by vendor ID.
pub fn enumerate(vendor_id: Option<u16>) -> Result<Vec<DeviceInfo>> {
    let mut all = platform::enumerate()?;
    if let Some(vid) = vendor_id {
        all.retain(|d| d.vendor_id == vid);
    }
    Ok(all)
}

/// An open HID collection. Safe to share between a reader and writer thread.
pub struct HidDevice {
    inner: platform::Device,
    info: DeviceInfo,
}

impl HidDevice {
    pub fn open(info: &DeviceInfo) -> Result<Self> {
        Ok(Self { inner: platform::Device::open(info)?, info: info.clone() })
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Writes one output report (first byte = report ID). Shorter buffers are
    /// zero-padded to the collection's output report length where required.
    pub fn write(&self, report: &[u8]) -> Result<()> {
        self.inner.write(report)
    }

    /// Reads one input report into `buf`, returning its length, or `Ok(0)` on
    /// timeout. `None` blocks indefinitely.
    pub fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> Result<usize> {
        self.inner.read(buf, timeout)
    }
}

// The platform devices are internally synchronized (overlapped I/O with
// per-call events on Windows, plain fds on Linux).
unsafe impl Send for HidDevice {}
unsafe impl Sync for HidDevice {}

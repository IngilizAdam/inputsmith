//! Linux backend: /sys/class/hidraw enumeration + /dev/hidrawN I/O.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use crate::{Bus, DeviceInfo, Error, Result};

/// Parses the top-level (usage_page, usage) collections out of a report descriptor.
fn top_level_collections(desc: &[u8]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let (mut page, mut usage, mut depth) = (0u16, 0u16, 0i32);
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // long item
            let size = *desc.get(i + 1).unwrap_or(&0) as usize;
            i += 3 + size;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            s => s as usize,
        };
        let mut val = 0u32;
        for k in 0..size {
            val |= (*desc.get(i + 1 + k).unwrap_or(&0) as u32) << (8 * k);
        }
        match prefix & 0xFC {
            0x04 => page = val as u16,                // Usage Page
            0x08 if depth == 0 => usage = val as u16, // Usage
            0xA0 => {
                if depth == 0 {
                    out.push((page, usage));
                }
                depth += 1;
            }
            0xC0 => depth -= 1,
            _ => {}
        }
        i += 1 + size;
    }
    out
}

fn uevent_field<'a>(uevent: &'a str, key: &str) -> Option<&'a str> {
    uevent.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
}

pub fn enumerate() -> Result<Vec<DeviceInfo>> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return Ok(out) };
    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let sys = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(sys.join("uevent")) else { continue };
        // HID_ID=0003:0000046D:0000C52B
        let Some(id) = uevent_field(&uevent, "HID_ID") else { continue };
        let parts: Vec<&str> = id.split(':').collect();
        if parts.len() != 3 {
            continue;
        }
        let bus = match u16::from_str_radix(parts[0], 16).unwrap_or(0) {
            0x03 => Bus::Usb,
            0x05 => Bus::Bluetooth,
            _ => Bus::Unknown,
        };
        let vendor_id = u32::from_str_radix(parts[1], 16).unwrap_or(0) as u16;
        let product_id = u32::from_str_radix(parts[2], 16).unwrap_or(0) as u16;
        let desc = fs::read(sys.join("report_descriptor")).unwrap_or_default();
        let collections = top_level_collections(&desc);
        let (usage_page, usage) = collections.first().copied().unwrap_or((0, 0));
        let interface_number = fs::canonicalize(&sys).ok().and_then(|p| {
            // .../1-2:1.2/0003:046D:C52B.0003 -> interface 2
            let iface = p.parent()?.file_name()?.to_string_lossy().into_owned();
            iface.rsplit('.').next()?.parse().ok()
        });
        let container_id = fs::canonicalize(&sys).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(DeviceInfo {
            path: format!("/dev/{name}"),
            vendor_id,
            product_id,
            usage_page,
            usage,
            interface_number,
            input_report_len: 0,
            output_report_len: 0,
            product: uevent_field(&uevent, "HID_NAME").map(str::to_string),
            serial: uevent_field(&uevent, "HID_UNIQ").filter(|s| !s.is_empty()).map(str::to_string),
            bus,
            container_id,
            collections,
        });
    }
    Ok(out)
}

pub struct Device {
    file: File,
}

impl Device {
    pub fn open(info: &DeviceInfo) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(Path::new(&info.path))?;
        Ok(Self { file })
    }

    pub fn write(&self, report: &[u8]) -> Result<()> {
        (&self.file).write_all(report).map_err(map_io)
    }

    pub fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> Result<usize> {
        let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ms = timeout.map_or(-1, |d| d.as_millis().min(i32::MAX as u128) as i32);
        let r = unsafe { libc::poll(&mut pfd, 1, ms) };
        if r < 0 {
            return Err(io::Error::last_os_error().into());
        }
        if r == 0 {
            return Ok(0);
        }
        if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            return Err(Error::Disconnected);
        }
        (&self.file).read(buf).map_err(map_io)
    }
}

fn map_io(e: io::Error) -> Error {
    if e.raw_os_error() == Some(libc::ENODEV) { Error::Disconnected } else { Error::Io(e) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hidpp_descriptor_collections() {
        // Usage Page 0xFF00, Usage 1, Collection(App) ... End, Usage 2, Collection ... End
        let desc = [0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0xC0, 0x09, 0x02, 0xA1, 0x01, 0xC0];
        assert_eq!(top_level_collections(&desc), vec![(0xFF00, 1), (0xFF00, 2)]);
    }
}

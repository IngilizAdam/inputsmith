//! Windows backend: SetupAPI enumeration + overlapped ReadFile/WriteFile.

use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::Duration;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData,
    HidD_GetProductString, HidD_GetSerialNumberString, HidP_GetCaps,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_DEVICE_NOT_CONNECTED, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, GENERIC_READ, GENERIC_WRITE,
    GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows_sys::core::GUID;

use crate::{Bus, DeviceInfo, Error, Result};

const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn open_path(path: &str, access: u32, overlapped: bool) -> io::Result<OwnedHandle> {
    let w = wide(path);
    let flags = if overlapped { FILE_FLAG_OVERLAPPED } else { 0 };
    let h = unsafe {
        CreateFileW(w.as_ptr(), access, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, flags, null_mut())
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(OwnedHandle(h))
}

fn interface_paths() -> Result<Vec<String>> {
    let mut guid: GUID = unsafe { zeroed() };
    unsafe { HidD_GetHidGuid(&mut guid) };
    let set = unsafe { SetupDiGetClassDevsW(&guid, null(), null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) };
    if set == INVALID_HANDLE_VALUE as _ {
        return Err(io::Error::last_os_error().into());
    }
    let mut paths = Vec::new();
    let mut index = 0;
    loop {
        let mut iface: SP_DEVICE_INTERFACE_DATA = unsafe { zeroed() };
        iface.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
        if unsafe { SetupDiEnumDeviceInterfaces(set, null(), &guid, index, &mut iface) } == 0 {
            break;
        }
        index += 1;
        let mut required = 0u32;
        unsafe { SetupDiGetDeviceInterfaceDetailW(set, &iface, null_mut(), 0, &mut required, null_mut()) };
        if required == 0 {
            continue;
        }
        // u64 backing keeps the detail struct suitably aligned.
        let mut storage = vec![0u64; (required as usize).div_ceil(8)];
        let detail = storage.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
        if unsafe { SetupDiGetDeviceInterfaceDetailW(set, &iface, detail, required, null_mut(), null_mut()) } == 0 {
            continue;
        }
        let path_ptr = unsafe { std::ptr::addr_of!((*detail).DevicePath) } as *const u16;
        let max = (required as usize - 4) / 2;
        let slice = unsafe { std::slice::from_raw_parts(path_ptr, max) };
        paths.push(from_wide(slice));
    }
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    Ok(paths)
}

/// `\\?\hid#vid_046d&pid_c52b&mi_02&col01#8&26907756&0&0000#{guid}` ->
/// `hid#vid_046d&pid_c52b&mi_02#8&26907756&0`
fn container_id(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let body = lower.trim_start_matches(r"\\?\");
    let mut parts: Vec<&str> = body.split('#').collect();
    if parts.len() >= 3 {
        parts.pop(); // interface class GUID
    }
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let segs: Vec<&str> = p.split('&').filter(|s| !s.starts_with("col")).collect();
        let segs = if i == 2 && segs.len() > 1 { &segs[..segs.len() - 1] } else { &segs[..] };
        out.push(segs.join("&"));
    }
    out.join("#")
}

fn interface_number(path: &str) -> Option<u8> {
    let lower = path.to_ascii_lowercase();
    let pos = lower.find("&mi_")?;
    u8::from_str_radix(lower.get(pos + 4..pos + 6)?, 16).ok()
}

fn bus_of(path: &str) -> Bus {
    let lower = path.to_ascii_lowercase();
    if lower.contains("00001812-0000-1000-8000-00805f9b34fb") || lower.contains("_dev_vid&") || lower.contains("bth") {
        Bus::Bluetooth
    } else if lower.contains("vid_") {
        Bus::Usb
    } else {
        Bus::Unknown
    }
}

fn hid_string(h: HANDLE, f: unsafe extern "system" fn(HANDLE, *mut c_void, u32) -> bool) -> Option<String> {
    let mut buf = [0u16; 128];
    let ok = unsafe { f(h, buf.as_mut_ptr() as *mut c_void, (buf.len() * 2) as u32) };
    if !ok {
        return None;
    }
    let s = from_wide(&buf);
    (!s.is_empty()).then_some(s)
}

fn query(path: &str) -> Option<DeviceInfo> {
    // Access 0 lets us query attributes even for collections the OS holds
    // exclusively (system mice/keyboards).
    let h = open_path(path, 0, false).ok()?;
    let mut attrs: HIDD_ATTRIBUTES = unsafe { zeroed() };
    attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
    if !unsafe { HidD_GetAttributes(h.0, &mut attrs) } {
        return None;
    }
    let mut caps: HIDP_CAPS = unsafe { zeroed() };
    let mut pp = 0;
    if unsafe { HidD_GetPreparsedData(h.0, &mut pp) } {
        let st = unsafe { HidP_GetCaps(pp, &mut caps) };
        unsafe { HidD_FreePreparsedData(pp) };
        if st != HIDP_STATUS_SUCCESS {
            caps = unsafe { zeroed() };
        }
    }
    Some(DeviceInfo {
        path: path.to_string(),
        vendor_id: attrs.VendorID,
        product_id: attrs.ProductID,
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        interface_number: interface_number(path),
        input_report_len: caps.InputReportByteLength,
        output_report_len: caps.OutputReportByteLength,
        product: hid_string(h.0, HidD_GetProductString),
        serial: hid_string(h.0, HidD_GetSerialNumberString),
        bus: bus_of(path),
        container_id: container_id(path),
        collections: vec![(caps.UsagePage, caps.Usage)],
    })
}

pub fn enumerate() -> Result<Vec<DeviceInfo>> {
    Ok(interface_paths()?.iter().filter_map(|p| query(p)).collect())
}

struct Event(OwnedHandle);

impl Event {
    fn new() -> io::Result<Self> {
        let h = unsafe { CreateEventW(null(), 1, 0, null()) };
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(OwnedHandle(h)))
    }
}

pub struct Device {
    handle: OwnedHandle,
    input_len: usize,
    output_len: usize,
}

fn map_err(code: u32) -> Error {
    if code == ERROR_DEVICE_NOT_CONNECTED || code == 0x1F
    /* ERROR_GEN_FAILURE */
    {
        Error::Disconnected
    } else {
        Error::Io(io::Error::from_raw_os_error(code as i32))
    }
}

impl Device {
    pub fn open(info: &DeviceInfo) -> Result<Self> {
        let handle = open_path(&info.path, GENERIC_READ | GENERIC_WRITE, true)?;
        Ok(Self { handle, input_len: info.input_report_len as usize, output_len: info.output_report_len as usize })
    }

    /// Runs one overlapped operation; returns transferred bytes or None on timeout.
    fn overlapped(
        &self,
        timeout: Option<Duration>,
        start: impl FnOnce(*mut OVERLAPPED) -> bool,
    ) -> Result<Option<usize>> {
        let event = Event::new()?;
        let mut ov: OVERLAPPED = unsafe { zeroed() };
        ov.hEvent = event.0.0;
        if !start(&mut ov) {
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return Err(map_err(err));
            }
        }
        let ms = timeout.map_or(INFINITE, |d| d.as_millis().min(u32::MAX as u128 - 1) as u32);
        let w = unsafe { WaitForSingleObject(event.0.0, ms) };
        if w == WAIT_TIMEOUT {
            unsafe { CancelIoEx(self.handle.0, &ov) };
        } else if w != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error().into());
        }
        // Always reap the operation; it may have completed right before the cancel.
        let mut n = 0u32;
        if unsafe { GetOverlappedResult(self.handle.0, &ov, &mut n, 1) } == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_OPERATION_ABORTED {
                return Ok(None);
            }
            return Err(map_err(err));
        }
        Ok(Some(n as usize))
    }

    pub fn write(&self, report: &[u8]) -> Result<()> {
        let mut buf = report.to_vec();
        if self.output_len > 0 {
            if buf.len() > self.output_len {
                return Err(Error::ReportTooLarge { len: buf.len(), max: self.output_len });
            }
            buf.resize(self.output_len, 0);
        }
        let h = self.handle.0;
        let r = self.overlapped(Some(Duration::from_secs(2)), |ov| unsafe {
            WriteFile(h, buf.as_ptr(), buf.len() as u32, null_mut(), ov) != 0
        })?;
        match r {
            Some(_) => Ok(()),
            None => Err(Error::Io(io::Error::new(io::ErrorKind::TimedOut, "HID write timed out"))),
        }
    }

    pub fn read(&self, out: &mut [u8], timeout: Option<Duration>) -> Result<usize> {
        let mut buf = vec![0u8; self.input_len.max(out.len()).max(64)];
        let h = self.handle.0;
        let len = buf.len() as u32;
        let ptr = buf.as_mut_ptr();
        match self.overlapped(timeout, |ov| unsafe { ReadFile(h, ptr, len, null_mut(), ov) != 0 })? {
            None => Ok(0),
            Some(n) => {
                let n = n.min(out.len());
                out[..n].copy_from_slice(&buf[..n]);
                Ok(n)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_strips_collection_and_instance_suffix() {
        let a = container_id(
            r"\\?\hid#vid_046d&pid_c52b&mi_02&col01#8&26907756&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}",
        );
        let b = container_id(
            r"\\?\hid#vid_046d&pid_c52b&mi_02&col02#8&26907756&0&0001#{4d1e55b2-f16f-11cf-88cb-001111000030}",
        );
        assert_eq!(a, b);
        assert_eq!(a, "hid#vid_046d&pid_c52b&mi_02#8&26907756&0");
        assert_eq!(interface_number(r"\\?\hid#vid_046d&pid_c52b&mi_02&col01#x"), Some(2));
    }
}

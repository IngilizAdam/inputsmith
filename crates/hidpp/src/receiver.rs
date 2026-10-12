//! HID++ 1.0 receivers (Unifying, Nano, Lightspeed, Bolt).

use std::time::Duration;

use crate::channel::{Channel, DEFAULT_TIMEOUT};
use crate::error::{Error, Result};
use crate::report::{DEVICE_INDEX_RECEIVER, Report, ReportKind, sub_id};

pub mod register {
    pub const NOTIFICATIONS: u8 = 0x00;
    pub const CONNECTION_STATE: u8 = 0x02;
    pub const DEVICE_ACTIVITY: u8 = 0xB3;
    pub const RECEIVER_INFO: u8 = 0xB5;
    pub const FIRMWARE: u8 = 0xF1;
}

/// Sub-registers of `RECEIVER_INFO` (0xB5).
pub mod info {
    pub const SERIAL: u8 = 0x03;
    pub const PAIRING: u8 = 0x20;
    pub const EXTENDED_PAIRING: u8 = 0x30;
    pub const DEVICE_NAME: u8 = 0x40;
    pub const BOLT_PAIRING: u8 = 0x50;
    pub const BOLT_DEVICE_NAME: u8 = 0x60;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    Unifying,
    Bolt,
    Nano,
    Lightspeed,
    Unknown,
}

/// Classifies a receiver by USB product ID.
pub fn receiver_kind(pid: u16) -> Option<ReceiverKind> {
    Some(match pid {
        0xC52B | 0xC532 => ReceiverKind::Unifying,
        0xC548 => ReceiverKind::Bolt,
        0xC52F | 0xC526 | 0xC52E | 0xC51A | 0xC51B | 0xC521 | 0xC525 | 0xC531 | 0xC534 | 0xC517 | 0xC518 | 0xC537 => {
            ReceiverKind::Nano
        }
        0xC539 | 0xC53A | 0xC53D | 0xC53F | 0xC541 | 0xC545 | 0xC547 | 0xC54D => ReceiverKind::Lightspeed,
        _ => return None,
    })
}

/// Device kinds from receiver pairing information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Unknown,
    Keyboard,
    Mouse,
    Numpad,
    Presenter,
    Remote,
    Trackball,
    Touchpad,
    Tablet,
    Gamepad,
    Joystick,
    Headset,
}

impl From<u8> for DeviceKind {
    fn from(k: u8) -> Self {
        match k & 0x0F {
            1 => Self::Keyboard,
            2 => Self::Mouse,
            3 => Self::Numpad,
            4 => Self::Presenter,
            7 => Self::Remote,
            8 => Self::Trackball,
            9 => Self::Touchpad,
            0x0A => Self::Tablet,
            0x0B => Self::Gamepad,
            0x0C => Self::Joystick,
            0x0D => Self::Headset,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevice {
    /// Device index on the receiver (1-based).
    pub index: u8,
    /// Wireless product ID.
    pub wpid: u16,
    pub kind: DeviceKind,
    pub serial: Option<String>,
    pub name: Option<String>,
    /// Report interval in ms, when the receiver reports it.
    pub polling_ms: Option<u8>,
}

/// A device (dis)connection notification from the receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionEvent {
    pub index: u8,
    pub wpid: u16,
    pub kind: DeviceKind,
    pub link_established: bool,
    pub encrypted: bool,
}

impl ConnectionEvent {
    /// Parses a `0x41` device connection notification.
    pub fn parse(r: &Report) -> Option<Self> {
        if r.sub_id() != sub_id::DEVICE_CONNECTION {
            return None;
        }
        let p = r.params();
        Some(Self {
            index: r.device_index(),
            wpid: u16::from_le_bytes([p[1], p[2]]),
            kind: DeviceKind::from(p[0]),
            link_established: p[0] & 0x40 == 0,
            encrypted: p[0] & 0x20 != 0,
        })
    }
}

pub struct Receiver {
    channel: Channel,
    kind: ReceiverKind,
    timeout: Duration,
}

impl Receiver {
    pub fn new(channel: Channel) -> Self {
        let kind = receiver_kind(channel.info().product_id).unwrap_or(ReceiverKind::Unknown);
        Self { channel, kind, timeout: DEFAULT_TIMEOUT }
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    pub fn kind(&self) -> ReceiverKind {
        self.kind
    }

    pub fn max_devices(&self) -> u8 {
        match self.kind {
            ReceiverKind::Unifying | ReceiverKind::Bolt => 6,
            _ => 1,
        }
    }

    fn register_request(&self, sub: u8, long: bool, address: u8, params: &[u8]) -> Result<Vec<u8>> {
        let kind = if long { ReportKind::Long } else { ReportKind::Short };
        let req = Report::new(kind, DEVICE_INDEX_RECEIVER, sub, address, params);
        Ok(self.channel.request(&req, self.timeout)?.params().to_vec())
    }

    pub fn read_register(&self, address: u8, params: &[u8]) -> Result<Vec<u8>> {
        self.register_request(sub_id::GET_REGISTER, false, address, params)
    }

    pub fn read_long_register(&self, address: u8, params: &[u8]) -> Result<Vec<u8>> {
        self.register_request(sub_id::GET_LONG_REGISTER, false, address, params)
    }

    pub fn write_register(&self, address: u8, params: &[u8]) -> Result<Vec<u8>> {
        self.register_request(sub_id::SET_REGISTER, false, address, params)
    }

    /// Number of devices currently connected (link up).
    pub fn connected_count(&self) -> Result<u8> {
        Ok(self.read_register(register::CONNECTION_STATE, &[])?[1])
    }

    /// Asks the receiver to emit a connection notification for every paired
    /// device, which is how current link state is discovered.
    pub fn trigger_device_arrival(&self) -> Result<()> {
        self.write_register(register::CONNECTION_STATE, &[0x02])?;
        Ok(())
    }

    pub fn serial(&self) -> Result<String> {
        let p = self.read_long_register(register::RECEIVER_INFO, &[info::SERIAL])?;
        Ok(hex(&p[1..5]))
    }

    /// Firmware version string, e.g. `RQR24.11_B0036`.
    pub fn firmware(&self) -> Result<String> {
        let a = self.read_register(register::FIRMWARE, &[0x01])?;
        let b = self.read_register(register::FIRMWARE, &[0x02])?;
        Ok(format!("{:02X}.{:02X}_B{:02X}{:02X}", a[1], a[2], b[1], b[2]))
    }

    /// Pairing information for receiver slot `index` (1-based). `Ok(None)` for empty slots.
    /// Enables device (dis)connection notifications and announces that host
    /// software is present (register 0x00 flags `wireless` 0x000100 and
    /// `software present` 0x000800), preserving other flags.
    pub fn enable_notifications(&self) -> Result<()> {
        let mut flags = self.read_register(register::NOTIFICATIONS, &[])?;
        flags.truncate(3);
        flags[1] |= 0x09;
        self.write_register(register::NOTIFICATIONS, &flags)?;
        Ok(())
    }

    pub fn paired_device(&self, index: u8) -> Result<Option<PairedDevice>> {
        match self.kind {
            ReceiverKind::Bolt => self.bolt_paired_device(index),
            _ => self.unifying_paired_device(index),
        }
    }

    fn unifying_paired_device(&self, index: u8) -> Result<Option<PairedDevice>> {
        let slot = index - 1;
        let p = match self.read_long_register(register::RECEIVER_INFO, &[info::PAIRING + slot]) {
            Ok(p) => p,
            Err(Error::Hidpp10(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        let wpid = u16::from_be_bytes([p[3], p[4]]);
        if wpid == 0 {
            return Ok(None);
        }
        let serial = self
            .read_long_register(register::RECEIVER_INFO, &[info::EXTENDED_PAIRING + slot])
            .ok()
            .map(|e| hex(&e[1..5]));
        let name = self.read_long_register(register::RECEIVER_INFO, &[info::DEVICE_NAME + slot]).ok().map(|n| {
            let len = (n[1] as usize).min(n.len() - 2);
            String::from_utf8_lossy(&n[2..2 + len]).into_owned()
        });
        Ok(Some(PairedDevice { index, wpid, kind: DeviceKind::from(p[7]), serial, name, polling_ms: Some(p[2]) }))
    }

    fn bolt_paired_device(&self, index: u8) -> Result<Option<PairedDevice>> {
        let p = match self.read_long_register(register::RECEIVER_INFO, &[info::BOLT_PAIRING + index]) {
            Ok(p) => p,
            Err(Error::Hidpp10(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        let wpid = u16::from_le_bytes([p[2], p[3]]);
        if wpid == 0 {
            return Ok(None);
        }
        let name =
            self.read_long_register(register::RECEIVER_INFO, &[info::BOLT_DEVICE_NAME + index, 1]).ok().map(|n| {
                let len = (n[2] as usize).min(n.len() - 3);
                String::from_utf8_lossy(&n[3..3 + len]).into_owned()
            });
        Ok(Some(PairedDevice {
            index,
            wpid,
            kind: DeviceKind::from(p[1]),
            serial: Some(hex(&p[4..8])),
            name,
            polling_ms: None,
        }))
    }

    pub fn paired_devices(&self) -> Result<Vec<PairedDevice>> {
        let mut out = Vec::new();
        for i in 1..=self.max_devices() {
            if let Some(d) = self.paired_device(i)? {
                out.push(d);
            }
        }
        Ok(out)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

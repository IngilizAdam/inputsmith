//! 0x0003 Device Information, 0x0005 Device Name & Type, 0x0007 Friendly Name.

use super::be16;
use crate::Device;
use crate::error::Result;
use crate::feature_id::{DEVICE_FRIENDLY_NAME, DEVICE_INFORMATION, DEVICE_NAME_TYPE};

/// Transport bits of 0x0003 getDeviceInfo.
pub mod transport {
    pub const BT: u16 = 0x01;
    pub const BTLE: u16 = 0x02;
    pub const EQUAD: u16 = 0x04;
    pub const USB: u16 = 0x08;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub entity_count: u8,
    pub unit_id: [u8; 4],
    pub transport: u16,
    /// One product ID per supported transport, in transport-bit order.
    pub model_ids: [u16; 3],
    pub extended_model_id: u8,
    pub capabilities: u8,
}

impl DeviceInfo {
    /// The model identifier used by the vendor software and its device
    /// database, e.g. `6b025` for an MX Anywhere 3: transport bits in hex
    /// followed by the first product ID.
    pub fn model_id(&self) -> String {
        // Without transport bits there is no leading digit (Options+ lists e.g. `4040`).
        if self.transport == 0 {
            return format!("{:04x}", self.model_ids[0]);
        }
        format!("{:x}{:04x}", self.transport, self.model_ids[0])
    }

    /// Model ID including the extended model (color/variant) suffix, e.g. `6b025_ext2`.
    pub fn extended_model_id_string(&self) -> String {
        if self.extended_model_id == 0 {
            self.model_id()
        } else {
            format!("{}_ext{}", self.model_id(), self.extended_model_id)
        }
    }

    pub fn has_serial_number(&self) -> bool {
        self.capabilities & 0x01 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareType {
    MainApplication,
    Bootloader,
    Hardware,
    Touchpad,
    OpticalSensor,
    Softdevice,
    RfCompanion,
    FactoryApplication,
    RgbCustomEffect,
    MotorDrive,
    Other(u8),
}

impl From<u8> for FirmwareType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::MainApplication,
            1 => Self::Bootloader,
            2 => Self::Hardware,
            3 => Self::Touchpad,
            4 => Self::OpticalSensor,
            5 => Self::Softdevice,
            6 => Self::RfCompanion,
            7 => Self::FactoryApplication,
            8 => Self::RgbCustomEffect,
            9 => Self::MotorDrive,
            o => Self::Other(o),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareEntity {
    pub kind: FirmwareType,
    pub prefix: String,
    pub number: u8,
    pub revision: u8,
    pub build: u16,
    pub active: bool,
    pub transport_pid: u16,
}

impl FirmwareEntity {
    /// Display form, e.g. `RBM14.00_B0013`.
    pub fn version_string(&self) -> String {
        format!("{}{:02X}.{:02X}_B{:04X}", self.prefix, self.number, self.revision, self.build)
    }
}

/// Device type from 0x0005 getDeviceType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Keyboard,
    RemoteControl,
    Numpad,
    Mouse,
    Trackpad,
    Trackball,
    Presenter,
    Receiver,
    Headset,
    Webcam,
    SteeringWheel,
    Joystick,
    Gamepad,
    Dock,
    Speaker,
    Microphone,
    IlluminationLight,
    ProgrammableController,
    CarSimPedals,
    Adapter,
    Other(u8),
}

impl From<u8> for DeviceType {
    fn from(v: u8) -> Self {
        use DeviceType::*;
        match v {
            0 => Keyboard,
            1 => RemoteControl,
            2 => Numpad,
            3 => Mouse,
            4 => Trackpad,
            5 => Trackball,
            6 => Presenter,
            7 => Receiver,
            8 => Headset,
            9 => Webcam,
            10 => SteeringWheel,
            11 => Joystick,
            12 => Gamepad,
            13 => Dock,
            14 => Speaker,
            15 => Microphone,
            16 => IlluminationLight,
            17 => ProgrammableController,
            18 => CarSimPedals,
            19 => Adapter,
            o => Other(o),
        }
    }
}

pub struct DeviceInformation<'a>(pub &'a Device);

impl DeviceInformation<'_> {
    pub fn device_info(&self) -> Result<DeviceInfo> {
        let p = self.0.call(DEVICE_INFORMATION, 0, &[])?;
        Ok(DeviceInfo {
            entity_count: p[0],
            unit_id: [p[1], p[2], p[3], p[4]],
            transport: be16(&p, 5),
            model_ids: [be16(&p, 7), be16(&p, 9), be16(&p, 11)],
            extended_model_id: p[13],
            capabilities: p[14],
        })
    }

    pub fn firmware(&self, entity: u8) -> Result<FirmwareEntity> {
        let p = self.0.call(DEVICE_INFORMATION, 1, &[entity])?;
        Ok(FirmwareEntity {
            kind: FirmwareType::from(p[0]),
            prefix: String::from_utf8_lossy(&p[1..4]).trim_end_matches('\0').to_string(),
            number: p[4],
            revision: p[5],
            build: be16(&p, 6),
            active: p[8] & 0x01 != 0,
            transport_pid: be16(&p, 9),
        })
    }

    pub fn all_firmware(&self) -> Result<Vec<FirmwareEntity>> {
        let n = self.device_info()?.entity_count;
        (0..n).map(|i| self.firmware(i)).collect()
    }

    /// 12-character serial number (feature version >= 4 with capability bit).
    pub fn serial_number(&self) -> Result<String> {
        let p = self.0.call(DEVICE_INFORMATION, 2, &[])?;
        Ok(String::from_utf8_lossy(&p[..12]).trim_end_matches('\0').to_string())
    }
}

pub struct DeviceName<'a>(pub &'a Device);

impl DeviceName<'_> {
    pub fn name(&self) -> Result<String> {
        let len = self.0.call(DEVICE_NAME_TYPE, 0, &[])?[0] as usize;
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let chunk = self.0.call(DEVICE_NAME_TYPE, 1, &[out.len() as u8])?;
            let take = (len - out.len()).min(chunk.len());
            out.extend_from_slice(&chunk[..take]);
        }
        Ok(String::from_utf8_lossy(&out).trim_end_matches('\0').to_string())
    }

    pub fn device_type(&self) -> Result<DeviceType> {
        Ok(DeviceType::from(self.0.call(DEVICE_NAME_TYPE, 2, &[])?[0]))
    }
}

pub struct FriendlyName<'a>(pub &'a Device);

impl FriendlyName<'_> {
    /// Returns `(name, max_length)`.
    pub fn get(&self) -> Result<(String, u8)> {
        let p = self.0.call(DEVICE_FRIENDLY_NAME, 0, &[])?;
        let (len, max) = (p[0] as usize, p[1]);
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let chunk = self.0.call(DEVICE_FRIENDLY_NAME, 1, &[out.len() as u8])?;
            // chunk[0] echoes the offset
            let take = (len - out.len()).min(chunk.len() - 1);
            if take == 0 {
                break;
            }
            out.extend_from_slice(&chunk[1..1 + take]);
        }
        Ok((String::from_utf8_lossy(&out).trim_end_matches('\0').to_string(), max))
    }
}

#[cfg(test)]
mod model_id_tests {
    use super::DeviceInfo;

    fn info(transport: u16, pid: u16) -> DeviceInfo {
        DeviceInfo { entity_count: 0, unit_id: [0; 4], transport, model_ids: [pid, 0, 0], extended_model_id: 0, capabilities: 0 }
    }

    #[test]
    fn formats_like_options_plus() {
        assert_eq!(info(0x06, 0xb025).model_id(), "6b025");
        assert_eq!(info(0x04, 0x4057).model_id(), "44057");
        assert_eq!(info(0x00, 0x4040).model_id(), "4040");
    }
}

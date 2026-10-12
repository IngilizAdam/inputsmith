use std::fmt;

/// HID++ 1.0 register access error codes (sub ID 0x8F).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hidpp10Error {
    InvalidSubId,
    InvalidAddress,
    InvalidValue,
    ConnectFail,
    TooManyDevices,
    AlreadyExists,
    Busy,
    UnknownDevice,
    ResourceError,
    RequestUnavailable,
    InvalidParamValue,
    WrongPinCode,
    Other(u8),
}

impl From<u8> for Hidpp10Error {
    fn from(c: u8) -> Self {
        match c {
            0x01 => Self::InvalidSubId,
            0x02 => Self::InvalidAddress,
            0x03 => Self::InvalidValue,
            0x04 => Self::ConnectFail,
            0x05 => Self::TooManyDevices,
            0x06 => Self::AlreadyExists,
            0x07 => Self::Busy,
            0x08 => Self::UnknownDevice,
            0x09 => Self::ResourceError,
            0x0A => Self::RequestUnavailable,
            0x0B => Self::InvalidParamValue,
            0x0C => Self::WrongPinCode,
            o => Self::Other(o),
        }
    }
}

/// HID++ 2.0 feature error codes (feature index 0xFF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hidpp20Error {
    NoError,
    Unknown,
    InvalidArgument,
    OutOfRange,
    HardwareError,
    LogitechInternal,
    InvalidFeatureIndex,
    InvalidFunctionId,
    Busy,
    Unsupported,
    Other(u8),
}

impl From<u8> for Hidpp20Error {
    fn from(c: u8) -> Self {
        match c {
            0x00 => Self::NoError,
            0x01 => Self::Unknown,
            0x02 => Self::InvalidArgument,
            0x03 => Self::OutOfRange,
            0x04 => Self::HardwareError,
            0x05 => Self::LogitechInternal,
            0x06 => Self::InvalidFeatureIndex,
            0x07 => Self::InvalidFunctionId,
            0x08 => Self::Busy,
            0x09 => Self::Unsupported,
            o => Self::Other(o),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HID transport: {0}")]
    Hid(#[from] inputsmith_hid::Error),
    #[error("request timed out")]
    Timeout,
    #[error("HID++ 1.0 error: {0:?}")]
    Hidpp10(Hidpp10Error),
    #[error("HID++ 2.0 error: {0:?}")]
    Hidpp20(Hidpp20Error),
    #[error("feature {0:#06x} not supported by device")]
    FeatureNotSupported(u16),
    #[error("device does not speak HID++ 2.0")]
    NotHidpp20,
    #[error("malformed response: {0}")]
    Malformed(&'static str),
    #[error("no HID++ interface found")]
    NoInterface,
    #[error("channel closed")]
    Closed,
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Hidpp20Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

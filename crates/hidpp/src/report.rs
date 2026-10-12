//! HID++ report framing.

pub const REPORT_ID_SHORT: u8 = 0x10;
pub const REPORT_ID_LONG: u8 = 0x11;
pub const REPORT_ID_VERY_LONG: u8 = 0x12;

pub const SHORT_LEN: usize = 7;
pub const LONG_LEN: usize = 20;
pub const VERY_LONG_LEN: usize = 64;

/// Device index addressing the receiver itself or a directly connected device.
pub const DEVICE_INDEX_RECEIVER: u8 = 0xFF;
/// Device index used by some wired / Bluetooth devices.
pub const DEVICE_INDEX_DIRECT: u8 = 0xFF;

/// HID++ 1.0 sub IDs.
pub mod sub_id {
    pub const SET_REGISTER: u8 = 0x80;
    pub const GET_REGISTER: u8 = 0x81;
    pub const SET_LONG_REGISTER: u8 = 0x82;
    pub const GET_LONG_REGISTER: u8 = 0x83;
    pub const ERROR_MSG: u8 = 0x8F;
    pub const DEVICE_DISCONNECTION: u8 = 0x40;
    pub const DEVICE_CONNECTION: u8 = 0x41;
}

/// HID++ 2.0 error feature index.
pub const ERROR_FEATURE_INDEX: u8 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Short,
    Long,
    VeryLong,
}

impl ReportKind {
    pub fn id(self) -> u8 {
        match self {
            ReportKind::Short => REPORT_ID_SHORT,
            ReportKind::Long => REPORT_ID_LONG,
            ReportKind::VeryLong => REPORT_ID_VERY_LONG,
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(self) -> usize {
        match self {
            ReportKind::Short => SHORT_LEN,
            ReportKind::Long => LONG_LEN,
            ReportKind::VeryLong => VERY_LONG_LEN,
        }
    }

    pub fn from_id(id: u8) -> Option<Self> {
        match id {
            REPORT_ID_SHORT => Some(ReportKind::Short),
            REPORT_ID_LONG => Some(ReportKind::Long),
            REPORT_ID_VERY_LONG => Some(ReportKind::VeryLong),
            _ => None,
        }
    }
}

/// A complete HID++ report including the report ID byte.
///
/// Layout: `[report_id, device_index, sub_id | feature_index, address | (function << 4 | sw_id), params...]`
#[derive(Clone, PartialEq, Eq)]
pub struct Report {
    bytes: Vec<u8>,
}

impl Report {
    /// Builds a report of `kind`, zero-padding/truncating `params`.
    pub fn new(kind: ReportKind, device_index: u8, b2: u8, b3: u8, params: &[u8]) -> Self {
        let mut bytes = vec![0u8; kind.len()];
        bytes[0] = kind.id();
        bytes[1] = device_index;
        bytes[2] = b2;
        bytes[3] = b3;
        let n = params.len().min(kind.len() - 4);
        bytes[4..4 + n].copy_from_slice(&params[..n]);
        Self { bytes }
    }

    /// HID++ 2.0 feature request.
    pub fn feature(
        kind: ReportKind,
        device_index: u8,
        feature_index: u8,
        function: u8,
        sw_id: u8,
        params: &[u8],
    ) -> Self {
        Self::new(kind, device_index, feature_index, (function << 4) | (sw_id & 0x0F), params)
    }

    /// Parses raw bytes received from a HID++ collection; `None` if not HID++.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let kind = ReportKind::from_id(*bytes.first()?)?;
        if bytes.len() < 4 {
            return None;
        }
        let mut v = bytes[..bytes.len().min(kind.len())].to_vec();
        v.resize(kind.len(), 0);
        Some(Self { bytes: v })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn kind(&self) -> ReportKind {
        ReportKind::from_id(self.bytes[0]).expect("validated on construction")
    }

    pub fn device_index(&self) -> u8 {
        self.bytes[1]
    }

    /// HID++ 1.0 sub ID or HID++ 2.0 feature index.
    pub fn sub_id(&self) -> u8 {
        self.bytes[2]
    }

    pub fn feature_index(&self) -> u8 {
        self.bytes[2]
    }

    /// HID++ 1.0 register address or HID++ 2.0 `function << 4 | sw_id`.
    pub fn address(&self) -> u8 {
        self.bytes[3]
    }

    pub fn function(&self) -> u8 {
        self.bytes[3] >> 4
    }

    pub fn sw_id(&self) -> u8 {
        self.bytes[3] & 0x0F
    }

    pub fn params(&self) -> &[u8] {
        &self.bytes[4..]
    }

    /// Whether this is an error report (HID++ 1.0 `0x8F` or HID++ 2.0 `0xFF`).
    pub fn is_error(&self) -> bool {
        self.sub_id() == sub_id::ERROR_MSG || self.sub_id() == ERROR_FEATURE_INDEX
    }

    /// For error reports: `(echoed byte 2, echoed byte 3, error code)`.
    pub fn error_info(&self) -> Option<(u8, u8, u8)> {
        self.is_error().then(|| (self.bytes[3], self.bytes[4], self.bytes[5]))
    }
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, b) in self.bytes.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_feature_request() {
        let r = Report::feature(ReportKind::Long, 1, 0x00, 0x1, 0xD, &[0, 0, 0x5A]);
        assert_eq!(r.as_bytes().len(), LONG_LEN);
        assert_eq!(&r.as_bytes()[..7], &[0x11, 0x01, 0x00, 0x1D, 0x00, 0x00, 0x5A]);
        assert_eq!(r.function(), 1);
        assert_eq!(r.sw_id(), 0xD);
    }

    #[test]
    fn parses_hidpp20_error() {
        let r = Report::parse(&[0x11, 0x01, 0xFF, 0x05, 0x1D, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert!(r.is_error());
        assert_eq!(r.error_info(), Some((0x05, 0x1D, 0x02)));
    }

    #[test]
    fn rejects_non_hidpp() {
        assert!(Report::parse(&[0x20, 1, 2, 3]).is_none());
    }
}

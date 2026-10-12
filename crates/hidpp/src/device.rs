//! HID++ 2.0 device: protocol detection, feature table, feature calls.

use std::sync::RwLock;
use std::time::Duration;

use crate::channel::{Channel, DEFAULT_TIMEOUT};
use crate::error::{Error, Hidpp10Error, Result};
use crate::feature_id;
use crate::report::{Report, ReportKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Feature type flags returned by Root/FeatureSet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureFlags(pub u8);

impl FeatureFlags {
    pub fn obsolete(self) -> bool {
        self.0 & 0x80 != 0
    }
    pub fn hidden(self) -> bool {
        self.0 & 0x40 != 0
    }
    pub fn engineering(self) -> bool {
        self.0 & 0x20 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureEntry {
    pub index: u8,
    pub id: u16,
    pub flags: FeatureFlags,
    pub version: u8,
}

/// Probes the HID++ protocol version of the device at `index`.
///
/// Returns `Ok(None)` when no device answers at that index (empty receiver slot
/// or sleeping device), `Ok(Some(1.0))` for HID++ 1.0 devices.
pub fn ping(channel: &Channel, index: u8, timeout: Duration) -> Result<Option<ProtocolVersion>> {
    const PING_DATA: u8 = 0x5A;
    let kind = channel.effective_kind(ReportKind::Short);
    let req = Report::feature(kind, index, 0x00, 0x1, channel.sw_id(), &[0, 0, PING_DATA]);
    match channel.request(&req, timeout) {
        Ok(r) => {
            let p = r.params();
            Ok(Some(ProtocolVersion { major: p[0], minor: p[1] }))
        }
        // HID++ 1.0 devices reject the root call as an unknown sub ID.
        Err(Error::Hidpp10(Hidpp10Error::InvalidSubId)) => Ok(Some(ProtocolVersion { major: 1, minor: 0 })),
        Err(Error::Hidpp10(Hidpp10Error::UnknownDevice | Hidpp10Error::ResourceError | Hidpp10Error::ConnectFail)) => {
            Ok(None)
        }
        Err(Error::Timeout) => Ok(None),
        Err(e) => Err(e),
    }
}

/// A HID++ 2.0 device reachable through a channel at a device index.
pub struct Device {
    channel: Channel,
    index: u8,
    protocol: ProtocolVersion,
    features: RwLock<Vec<FeatureEntry>>,
    timeout: Duration,
}

impl Device {
    /// Pings the device and loads its complete feature table.
    pub fn open(channel: Channel, index: u8) -> Result<Self> {
        let protocol = ping(&channel, index, DEFAULT_TIMEOUT)?.ok_or(Error::Timeout)?;
        if protocol.major < 2 {
            return Err(Error::NotHidpp20);
        }
        let dev = Self { channel, index, protocol, features: RwLock::new(Vec::new()), timeout: DEFAULT_TIMEOUT };
        dev.load_features()?;
        Ok(dev)
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    pub fn index(&self) -> u8 {
        self.index
    }

    pub fn protocol(&self) -> ProtocolVersion {
        self.protocol
    }

    pub fn features(&self) -> Vec<FeatureEntry> {
        self.features.read().unwrap().clone()
    }

    pub fn feature(&self, id: u16) -> Option<FeatureEntry> {
        self.features.read().unwrap().iter().find(|f| f.id == id).copied()
    }

    pub fn has_feature(&self, id: u16) -> bool {
        self.feature(id).is_some()
    }

    /// Raw call by feature index. Returns the 16 (or 60) parameter bytes.
    pub fn call_index(&self, feature_index: u8, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        // HID++ 2.0 calls always go out long: Bluetooth devices have no short
        // collection, and every receiver accepts long requests.
        let kind =
            if params.len() <= 16 { self.channel.effective_kind(ReportKind::Long) } else { ReportKind::VeryLong };
        let req = Report::feature(kind, self.index, feature_index, function, self.channel.sw_id(), params);
        Ok(self.channel.request(&req, self.timeout)?.params().to_vec())
    }

    /// Calls `function` of feature `id`.
    pub fn call(&self, id: u16, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        let f = self.feature(id).ok_or(Error::FeatureNotSupported(id))?;
        self.call_index(f.index, function, params)
    }

    /// Root.getFeature: resolves a feature ID to its index.
    pub fn root_get_feature(&self, id: u16) -> Result<Option<FeatureEntry>> {
        let p = self.call_index(0, 0x0, &id.to_be_bytes())?;
        if p[0] == 0 && id != feature_id::ROOT {
            return Ok(None);
        }
        Ok(Some(FeatureEntry { index: p[0], id, flags: FeatureFlags(p[1]), version: p[2] }))
    }

    fn load_features(&self) -> Result<()> {
        let mut table = vec![FeatureEntry { index: 0, id: feature_id::ROOT, flags: FeatureFlags(0), version: 0 }];
        if let Some(fs) = self.root_get_feature(feature_id::FEATURE_SET)? {
            let count = self.call_index(fs.index, 0x0, &[])?[0];
            for i in 1..=count {
                let p = self.call_index(fs.index, 0x1, &[i])?;
                table.push(FeatureEntry {
                    index: i,
                    id: u16::from_be_bytes([p[0], p[1]]),
                    flags: FeatureFlags(p[2]),
                    version: p[3],
                });
            }
        }
        *self.features.write().unwrap() = table;
        Ok(())
    }
}

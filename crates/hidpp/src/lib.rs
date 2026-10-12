//! Logitech HID++ protocol.
//!
//! * [`channel`] — HID++ interfaces, request/response matching, notifications.
//! * [`receiver`] — HID++ 1.0 receivers and their pairing tables.
//! * [`device`] — HID++ 2.0 devices and their feature tables.
//! * [`features`] — typed feature wrappers.

pub mod channel;
pub mod device;
pub mod error;
pub mod feature_id;
pub mod features;
pub mod receiver;
pub mod report;

pub use channel::{Channel, InterfaceInfo};
pub use device::{Device, FeatureEntry, ProtocolVersion};
pub use error::{Error, Result};
pub use receiver::Receiver;
pub use report::{Report, ReportKind};

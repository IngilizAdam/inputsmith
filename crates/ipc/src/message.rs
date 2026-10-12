//! Messages between the Inputsmith GUI (or CLI) and the agent.
//!
//! A request names a `verb` and a `path` (a route, see [`crate::api::routes`]) and may carry a JSON
//! payload. The reply echoes `id`, `verb` and `path` and adds a `status`. Clients `subscribe` to a path
//! to receive `event` messages for it.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    Get,
    Set,
    Remove,
    Subscribe,
    /// Sent by the agent to subscribers; never answered.
    Event,
}

impl Verb {
    pub fn as_str(self) -> &'static str {
        match self {
            Verb::Get => "get",
            Verb::Set => "set",
            Verb::Remove => "remove",
            Verb::Subscribe => "subscribe",
            Verb::Event => "event",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "get" => Verb::Get,
            "set" => Verb::Set,
            "remove" => Verb::Remove,
            "subscribe" => Verb::Subscribe,
            "event" => Verb::Event,
            _ => return None,
        })
    }
}

/// Outcome of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    /// The payload is missing or malformed.
    InvalidArgument,
    /// The device, profile or file does not exist.
    NotFound,
    /// No such route.
    UnknownRoute,
    /// The request was valid but could not be carried out.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Chosen by the client, echoed in the reply; 0 for events.
    #[serde(default)]
    pub id: u64,
    pub verb: Verb,
    pub path: String,
    /// Set on replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    /// Human-readable reason when `status` is not `ok`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

impl Message {
    pub fn new(verb: Verb, path: impl Into<String>) -> Self {
        Self { id: 0, verb, path: path.into(), status: None, error: String::new(), payload: None }
    }

    pub fn with_payload<T: Serialize>(mut self, payload: &T) -> Result<Self, serde_json::Error> {
        self.payload = Some(serde_json::to_value(payload)?);
        Ok(self)
    }

    /// A reply to `self` with the given status.
    pub fn reply(&self, status: Status) -> Self {
        Self {
            id: self.id,
            verb: self.verb,
            path: self.path.clone(),
            status: Some(status),
            error: String::new(),
            payload: None,
        }
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = error.into();
        self
    }

    pub fn is_ok(&self) -> bool {
        self.status.is_none_or(|s| s == Status::Ok)
    }

    /// Decodes the payload as `T`.
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.payload.clone().unwrap_or(Value::Null))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Message serialization is infallible")
    }

    pub fn from_json(s: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = Message { id: 7, ..Message::new(Verb::Get, "/devices/list") };
        let json = m.to_json();
        assert_eq!(json, r#"{"id":7,"verb":"get","path":"/devices/list"}"#);
        let r = m.reply(Status::NotFound).with_error("no such device");
        let back = Message::from_json(r.to_json().as_bytes()).unwrap();
        assert_eq!(back.status, Some(Status::NotFound));
        assert!(!back.is_ok());
    }
}

//! Framing on the agent pipe: each message is a little-endian `u32` length followed by that many bytes
//! of UTF-8 JSON.

use std::io;

/// Upper bound accepted for a single frame.
pub const MAX_FRAME: usize = 64 * 1024 * 1024;

/// Encodes one frame.
pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Incremental decoder for chunked input.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// The next complete frame's payload, if one has arrived.
    pub fn next_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        let Some(len) = self.buf.get(..4) else { return Ok(None) };
        let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let payload = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Ok(Some(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_split_and_joined_frames() {
        let e = encode(br#"{"a":1}"#);
        assert_eq!(&e[..4], &7u32.to_le_bytes());
        let mut d = Decoder::default();
        d.push(&e[..5]);
        assert_eq!(d.next_frame().unwrap(), None);
        d.push(&e[5..]);
        d.push(&e);
        assert_eq!(d.next_frame().unwrap().as_deref(), Some(&br#"{"a":1}"#[..]));
        assert_eq!(d.next_frame().unwrap().as_deref(), Some(&br#"{"a":1}"#[..]));
        assert_eq!(d.next_frame().unwrap(), None);
    }
}

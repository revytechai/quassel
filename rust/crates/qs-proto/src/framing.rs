// Copyright (C) 2026 by the Quassel Project and the REVYTECH Rust port authors
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, write to the Free Software Foundation,
// Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

/*!
The framing of a DataStream message.

Every message is a 32 bit big endian length followed by that many bytes of
payload.  The core refuses messages bigger than 64 MB.
*/

use crate::error::ProtoError;

/// The biggest message the core accepts.
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;

/// Wraps a payload in a length prefix.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Turns a stream of bytes into messages.
#[derive(Debug)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    max_size: usize,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameDecoder {
    /// A decoder that accepts messages up to [`MAX_MESSAGE_SIZE`].
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            max_size: MAX_MESSAGE_SIZE,
        }
    }

    /// A decoder that accepts messages up to `max_size` bytes.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_size,
        }
    }

    /// Adds bytes that arrived from the network.
    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Takes the next complete message out of the buffer.
    ///
    /// Returns `Ok(None)` when there is not enough data yet.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, ProtoError> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let length = u32::from_be_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;
        if length > self.max_size {
            return Err(ProtoError::MessageTooLarge(length));
        }
        if self.buffer.len() < 4 + length {
            return Ok(None);
        }
        let payload = self.buffer[4..4 + length].to_vec();
        self.buffer.drain(..4 + length);
        Ok(Some(payload))
    }

    /// Whether the decoder holds no data.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

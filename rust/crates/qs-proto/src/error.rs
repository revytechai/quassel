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
Errors of the Quassel protocol layer.
*/

use qs_datastream::DecodeError;

/// Everything that can go wrong while speaking the Quassel protocol.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    /// The bytes on the wire were not a valid DataStream value.
    #[error("malformed data stream: {0}")]
    DataStream(#[from] DecodeError),
    /// A message ended in the middle of a value.
    #[error("unexpected end of input")]
    UnexpectedEof,
    /// The peer said something the protocol does not allow.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// A message was bigger than the core is willing to accept.
    #[error("message of {0} bytes is too large")]
    MessageTooLarge(usize),
    /// The request type of a signal proxy message is not one we know.
    #[error("unknown request type {0}")]
    UnknownRequestType(i64),
    /// The handshake message did not have the expected shape.
    #[error("malformed handshake message")]
    MalformedHandshake,
    /// The peer does not speak a protocol we support.
    #[error("no supported protocol")]
    NoSupportedProtocol,
}

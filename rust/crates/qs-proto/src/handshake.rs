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
The connection handshake.

The handshake is the one part of the protocol that is __not__ a DataStream
message.  The client opens a plain TCP connection and sends:

```text
uint32  0x42b33f00 | connection features the client wants
uint32  protocol 1  (protocol type in the low byte, protocol features in the
                    next two bytes; the last entry has bit 31 set)
uint32  protocol 2
...
```

The core answers with a single 32 bit word.  The word is a big endian number,
so the bytes on the wire are:

```text
byte 0  the connection features the core enabled (0x01 = encryption, 0x02 =
        compression)
byte 1  the protocol features the core enabled, high byte
byte 2  the protocol features the core enabled, low byte
byte 3  the protocol the core picked
```

That is the layout the C++ core uses: it writes
`peer->protocol() | peer->enabledFeatures() << 8 | _connectionFeatures << 24`
as a big endian word (`src/core/coreauthhandler.cpp`), and the C++ client
reads the protocol from the low byte of the word and the connection features
from the high byte (`src/client/clientauthhandler.cpp`).

Only then does the connection become a TLS one — the core does __not__ start a
TLS handshake when the socket opens.  A client that sends a TLS ClientHello
first is read as a legacy protocol packet and dropped.
*/

use crate::error::ProtoError;

/// The magic number that starts the handshake.
pub const MAGIC: u32 = 0x42b33f00;

/// The magic number of a proxy protocol line, which may precede the handshake.
pub const PROXY_MAGIC: u32 = 0x50524f58;

/// The bit that marks the last entry of the protocol list.
pub const LAST_PROTOCOL_FLAG: u32 = 0x8000_0000;

/// The protocol a peer speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolType {
    /// The internal protocol, used between the core's own parts.
    Internal = 0x00,
    /// The old Qt protocol.
    Legacy = 0x01,
    /// The DataStream protocol, the one everybody uses today.
    DataStream = 0x02,
}

impl ProtocolType {
    /// The wire id of the protocol.
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Maps a wire id back to a protocol.
    pub const fn from_id(id: u8) -> Option<Self> {
        Some(match id {
            0x00 => Self::Internal,
            0x01 => Self::Legacy,
            0x02 => Self::DataStream,
            _ => return None,
        })
    }

    /// The name the core prints for this protocol.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Internal => "InternalProtocol",
            Self::Legacy => "LegacyProtocol",
            Self::DataStream => "DataStreamProtocol",
        }
    }
}

/// The connection features a peer can ask for or be offered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionFeatures(u8);

impl ConnectionFeatures {
    /// The peer wants the connection to be encrypted.
    pub const ENCRYPTION: u8 = 0x01;
    /// The peer wants the connection to be compressed.
    pub const COMPRESSION: u8 = 0x02;

    /// An empty feature set.
    pub const fn none() -> Self {
        Self(0)
    }

    /// The raw bits.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Builds a feature set from raw bits.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Whether encryption was negotiated.
    pub const fn encryption(self) -> bool {
        self.0 & Self::ENCRYPTION != 0
    }

    /// Whether compression was negotiated.
    pub const fn compression(self) -> bool {
        self.0 & Self::COMPRESSION != 0
    }

    /// Adds a feature.
    pub const fn with(self, other: u8) -> Self {
        Self(self.0 | other)
    }
}

/// One entry of the protocol list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolDescriptor {
    /// The protocol.
    pub protocol: ProtocolType,
    /// The protocol specific features the peer supports.
    pub features: u16,
    /// Whether this is the last entry of the list.
    pub last: bool,
}

impl ProtocolDescriptor {
    /// A protocol entry that is not the last one.
    pub const fn new(protocol: ProtocolType, features: u16) -> Self {
        Self {
            protocol,
            features,
            last: false,
        }
    }

    /// The wire form of the entry.
    pub const fn encode(self) -> u32 {
        let mut word = self.protocol.id() as u32 | ((self.features as u32) << 8);
        if self.last {
            word |= LAST_PROTOCOL_FLAG;
        }
        word
    }

    /// Reads an entry from its wire form.
    ///
    /// A protocol id the core does not know is an error.  It is not the legacy
    /// protocol, and answering as if it were would tell the client that the
    /// core speaks a protocol it cannot speak.
    pub fn decode(word: u32) -> Result<Self, ProtoError> {
        let id = (word & 0xff) as u8;
        let Some(protocol) = ProtocolType::from_id(id) else {
            return Err(ProtoError::Protocol(format!(
                "the client offered the unknown protocol {id:#04x}"
            )));
        };
        Ok(Self {
            protocol,
            features: ((word >> 8) & 0xffff) as u16,
            last: word >= LAST_PROTOCOL_FLAG,
        })
    }
}

/// The handshake a client sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handshake {
    /// The connection features the client asks for.
    pub features: u8,
    /// The protocols the client speaks, most preferred first.
    pub protocols: Vec<ProtocolDescriptor>,
}

impl Handshake {
    /// A handshake that offers the DataStream protocol and the legacy one.
    pub fn datastream_with_legacy() -> Self {
        Self {
            features: 0,
            protocols: vec![
                ProtocolDescriptor::new(ProtocolType::DataStream, 0),
                ProtocolDescriptor {
                    protocol: ProtocolType::Legacy,
                    features: 0,
                    last: true,
                },
            ],
        }
    }

    /// The wire form of the handshake.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 4 * self.protocols.len());
        out.extend_from_slice(&(MAGIC | u32::from(self.features)).to_be_bytes());
        for protocol in &self.protocols {
            out.extend_from_slice(&protocol.encode().to_be_bytes());
        }
        out
    }

    /// Reads a handshake from the front of `bytes`.
    ///
    /// Returns the handshake and how many bytes it used.  The handshake ends
    /// at the protocol entry that carries the "last protocol" bit.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), ProtoError> {
        if bytes.len() < 4 {
            return Err(ProtoError::UnexpectedEof);
        }
        let magic = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic & 0xffff_ff00 != MAGIC {
            return Err(ProtoError::Protocol(format!(
                "not a Quassel handshake: magic is {magic:#010x}"
            )));
        }
        let features = (magic & 0xff) as u8;
        let mut protocols = Vec::new();
        let mut offset = 4;
        // The core reads at most sixteen protocols; so do we.
        while protocols.len() < 16 {
            if bytes.len() < offset + 4 {
                return Err(ProtoError::UnexpectedEof);
            }
            let word = u32::from_be_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;
            let descriptor = ProtocolDescriptor::decode(word)?;
            let last = descriptor.last;
            protocols.push(descriptor);
            if last {
                return Ok((
                    Self {
                        features,
                        protocols,
                    },
                    offset,
                ));
            }
        }
        Err(ProtoError::Protocol(
            "the client sent more than sixteen protocols".to_owned(),
        ))
    }
}

/// The core's answer to the handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandshakeReply {
    /// The protocol the core picked.
    pub protocol: ProtocolType,
    /// The protocol features the core enabled.
    pub features: u16,
    /// The connection features the core enabled.
    pub connection_features: ConnectionFeatures,
}

impl HandshakeReply {
    /// The wire form of the reply.
    ///
    /// The word is the one the C++ core writes: the protocol in the low byte,
    /// the protocol features in the two bytes above it and the connection
    /// features in the high byte.
    pub fn encode(self) -> [u8; 4] {
        let word = self.protocol.id() as u32
            | ((self.features as u32) << 8)
            | ((self.connection_features.bits() as u32) << 24);
        word.to_be_bytes()
    }

    /// Reads the reply.
    ///
    /// A protocol id the core does not know is an error.
    pub fn decode(bytes: [u8; 4]) -> Result<Self, ProtoError> {
        let word = u32::from_be_bytes(bytes);
        let id = (word & 0xff) as u8;
        let Some(protocol) = ProtocolType::from_id(id) else {
            return Err(ProtoError::Protocol(format!(
                "the core answered with the unknown protocol {id:#04x}"
            )));
        };
        Ok(Self {
            protocol,
            features: ((word >> 8) & 0xffff) as u16,
            connection_features: ConnectionFeatures::from_bits((word >> 24) as u8),
        })
    }
}

/// Picks the protocol to use, the way the core's peer factory does.
///
/// The first protocol of the list that the core supports wins.
///
/// Only the DataStream protocol is supported: the core has no legacy codec,
/// so a client that offers only the legacy protocol is not answered as if the
/// core had understood it.
pub fn select_protocol(offered: &[ProtocolDescriptor]) -> Option<ProtocolType> {
    offered
        .iter()
        .find(|descriptor| descriptor.protocol == ProtocolType::DataStream)
        .map(|descriptor| descriptor.protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_protocol_id_is_an_error() {
        // 0xff is not a protocol the core knows.
        let word = 0x8000_00ff;
        let error = ProtocolDescriptor::decode(word).unwrap_err();
        assert!(error.to_string().contains("unknown protocol"), "{error}");
    }

    #[test]
    fn the_legacy_protocol_is_not_the_cores_own() {
        // The old code turned every unknown id into the legacy protocol.
        assert!(ProtocolDescriptor::decode(0x8000_00ff).is_err());
        assert!(ProtocolDescriptor::decode(0x8000_0001).is_ok());
    }

    #[test]
    fn a_handshake_with_an_unknown_protocol_is_refused() {
        let mut bytes = Handshake {
            features: 0,
            protocols: vec![ProtocolDescriptor {
                protocol: ProtocolType::DataStream,
                features: 0,
                last: true,
            }],
        }
        .encode();
        // Replace the protocol id with one the core does not know.  The
        // word is big endian, so the id is its last byte.
        bytes[7] = 0xff;
        assert!(Handshake::decode(&bytes).is_err());
    }

    #[test]
    fn the_reply_is_the_word_the_cpp_core_writes() {
        // src/core/coreauthhandler.cpp:
        //   reply = protocol | enabledFeatures << 8 | connectionFeatures << 24
        // written as a big endian word, so the protocol is the last byte on
        // the wire and the connection features are the first one.
        let reply = HandshakeReply {
            protocol: ProtocolType::DataStream,
            features: 0,
            connection_features: ConnectionFeatures::from_bits(ConnectionFeatures::ENCRYPTION),
        };
        let bytes = reply.encode();
        assert_eq!(bytes, [0x01, 0x00, 0x00, 0x02]);
        assert_eq!(HandshakeReply::decode(bytes).unwrap(), reply);
    }
}

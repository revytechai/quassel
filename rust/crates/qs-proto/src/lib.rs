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
`qs-proto` — the Quassel core wire protocol.

The crate has three parts:

- the __handshake__ ([`handshake`]): the plaintext probe a client sends when
  the connection opens, the protocol list, and the core's answer;
- the __handshake messages__ ([`messages`]): `ClientInit`, `ClientLogin`,
  `SessionInit` and their friends;
- the __signal proxy layer__ ([`signal`]): the sync objects' wire shapes —
  sync calls, RPC calls, init requests and init data.

The bytes themselves are turned into values by `qs-datastream`; this crate
only says what those values mean.

# The handshake

```text
client -> core:  0x42b33f00 | connection features
                   protocol entries, the last one with bit 31 set
core -> client:  protocol | protocol features | connection features
```

The core does __not__ start a TLS handshake when the socket opens; it reads
this plaintext exchange first and only then upgrades the same socket to TLS.
*/

#![deny(missing_docs)]

mod error;
mod features;
mod framing;
mod handshake;
mod messages;
mod signal;

pub use error::ProtoError;
pub use features::{Features, FEATURES};
pub use framing::{frame, FrameDecoder, MAX_MESSAGE_SIZE};
pub use handshake::{
    select_protocol, ConnectionFeatures, Handshake, HandshakeReply, ProtocolDescriptor,
    ProtocolType, LAST_PROTOCOL_FLAG, MAGIC, PROXY_MAGIC,
};
pub use messages::{HandshakeMessage, SessionState};
pub use signal::{RequestType, SignalMessage};

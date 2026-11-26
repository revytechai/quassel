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
`qs-irc` — the IRC side of the Quassel core.

The crate does the networking: it opens a TCP connection, optionally wraps it
in TLS, and reads and writes IRC lines.

# Certificates

The certificate of an IRC server is checked against the system's roots.  A
server with a self-signed certificate would fail that check, and the C++ core
answers such a server by turning the verification off — which means anybody
in the middle could impersonate the server.

This crate adds a third option: pin the SHA-256 fingerprint of the
certificate.  The certificate then has to be exactly the one that was pinned
down, and the verification stays on.

```
use qs_irc::{Fingerprint, ServerVerification};

let fingerprint = Fingerprint::from_hex(
    "e6bf4b8b7e0f0c1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192",
)
.expect("a hex fingerprint");
let verification = ServerVerification::PinnedSha256(fingerprint);
```

# Lines

IRC is a line based protocol, and a line that holds a line break would send
more than one command.  The core refuses such a line.

```
use qs_irc::check_line;

assert!(check_line("PRIVMSG #channel :hello").is_ok());
assert!(check_line("PRIVMSG #channel :hello\r\nJOIN #secret").is_err());
```
*/

#![deny(missing_docs)]

mod connection;
mod error;
mod tls;

pub use connection::{
    check_line, IrcConnection, IrcServer, ReconnectPolicy, DEFAULT_RECONNECT_INTERVAL_SECONDS,
    DEFAULT_RECONNECT_RETRIES, MAX_LINE,
};
pub use error::IrcError;
pub use tls::{client_config, Fingerprint, ServerVerification, FINGERPRINT_LEN};

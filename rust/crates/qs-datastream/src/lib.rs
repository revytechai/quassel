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
`qs-datastream` — the Qt `QDataStream`/`QVariant` codec, as Quassel uses it.

This crate is deliberately free of I/O: it turns values into bytes and bytes
back into values, and nothing else.  The transport lives in `qs-proto`.

# The shapes that matter

A Quassel message body is a _list body_ — a 32 bit element count followed by
the elements — with __no__ `QVariant` type header in front of the count.  Each
element is a full `QVariant`: a 32 bit type id, an 8 bit null flag, and a
type specific payload.

| type | id | payload |
|---|---|---|
| `bool` | 1 | one byte |
| `int` | 2 | 32 bit signed |
| `uint` | 3 | 32 bit unsigned |
| `QVariantMap` | 8 | count, then (string key, value) pairs |
| `QVariantList` | 9 | count, then values |
| `QString` | 10 | byte count, then UTF-16BE |
| `QStringList` | 11 | count, then strings |
| `QByteArray` | 12 | length, then raw bytes |
| user type | 127 | type name, then the payload |

A null `QString` or `QByteArray` is the length `0xffffffff` and nothing else.
A map key and a string list element are plain string bodies; only a map's
value carries the `QVariant` header.

The decoder is strict where the wire can only be written one way: a string
payload must be a whole number of UTF-16 code units, and a value may not be
nested deeper than 64 levels.  Both are refused with an error rather than
repaired, so what comes out of the decoder is what went in.

A user type is the type id 127, then the type name (a length that counts the
NUL terminator, the name, the NUL), then the payload.  The core rejects names
it does not know, and the server list type is called `Network::Server`, not
`Network_Server`.

# Example

```
use qs_datastream::{encode_list_body_to_vec, decode_list_body, Value};

let body = encode_list_body_to_vec(&[Value::string("ClientLogin"), Value::string("alice")]);
let (values, used) = decode_list_body(&body).unwrap();
assert_eq!(used, body.len());
assert_eq!(values[0], Value::string("ClientLogin"));
```
*/

#![deny(missing_docs)]

mod decode;
mod encode;
mod error;
mod value;
mod wire;

pub use decode::{decode_list_body, decode_list_body_exact, decode_value, decode_value_exact};
pub use encode::{encode_list_body, encode_list_body_to_vec, encode_value, encode_value_to_vec};
pub use error::DecodeError;
pub use value::{IdType, Map, MapType, UserType, Value, VariantType};
pub use wire::{
    buffer_id, identity, identity_id, network_id, network_info, network_server, BUFFER_ID,
    BUFFER_INFO, IDENTITY, IDENTITY_ID, MSG_ID, NETWORK_ID, NETWORK_INFO, NETWORK_SERVER,
};

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
The signal proxy layer: the messages that carry the core's live state.

Once the handshake is over, every message is a DataStream _list body_ whose
first element is the request type:

| type | meaning |
|---|---|
| 1 | sync: a slot call on a synchronised object |
| 2 | RPC call: a signal with its arguments |
| 3 | init request: ask for an object's initial data |
| 4 | init data: the answer to an init request |
| 5 | heartbeat |
| 6 | heartbeat reply |

The request type is read as a 16 bit number by the core, so a 32 bit integer
works as well; this crate writes a 32 bit integer and reads both.
*/

use qs_datastream::{decode_list_body, encode_list_body_to_vec, Map, Value};

use crate::error::ProtoError;

/// The kind of a signal proxy message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RequestType {
    /// A slot call on a synchronised object.
    Sync = 1,
    /// A signal with its arguments.
    RpcCall = 2,
    /// A request for an object's initial data.
    InitRequest = 3,
    /// The answer to an init request.
    InitData = 4,
    /// A heartbeat.
    HeartBeat = 5,
    /// The answer to a heartbeat.
    HeartBeatReply = 6,
}

impl RequestType {
    /// The wire value of the request type.
    pub const fn id(self) -> i32 {
        self as i32
    }

    /// Maps a wire value back to a request type.
    pub const fn from_id(id: i64) -> Option<Self> {
        Some(match id {
            1 => Self::Sync,
            2 => Self::RpcCall,
            3 => Self::InitRequest,
            4 => Self::InitData,
            5 => Self::HeartBeat,
            6 => Self::HeartBeatReply,
            _ => return None,
        })
    }
}

/// A message of the signal proxy layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalMessage {
    /// A slot call on a synchronised object.
    Sync {
        /// The class of the synchronised object.
        class_name: String,
        /// The name of the synchronised object.
        object_name: String,
        /// The slot to call.
        slot_name: String,
        /// The slot's arguments.
        params: Vec<Value>,
    },
    /// A signal with its arguments.
    RpcCall {
        /// The signal name, for example `2createNetwork(NetworkInfo,QStringList)`.
        signal_name: String,
        /// The signal's arguments.
        params: Vec<Value>,
    },
    /// A request for an object's initial data.
    InitRequest {
        /// The class of the synchronised object.
        class_name: String,
        /// The name of the synchronised object.
        object_name: String,
    },
    /// The answer to an init request.
    InitData {
        /// The class of the synchronised object.
        class_name: String,
        /// The name of the synchronised object.
        object_name: String,
        /// The object's initial data.
        init_data: Map,
    },
    /// A heartbeat.
    HeartBeat {
        /// The timestamp, as a Julian day and milliseconds since midnight.
        timestamp: (i32, u32),
    },
    /// The answer to a heartbeat.
    HeartBeatReply {
        /// The timestamp, as a Julian day and milliseconds since midnight.
        timestamp: (i32, u32),
    },
}

impl SignalMessage {
    /// The wire form of the message.
    pub fn encode(&self) -> Vec<u8> {
        let mut items = Vec::new();
        match self {
            Self::Sync {
                class_name,
                object_name,
                slot_name,
                params,
            } => {
                items.push(Value::Int(RequestType::Sync.id()));
                items.push(Value::bytes(class_name.clone()));
                items.push(Value::bytes(object_name.clone()));
                items.push(Value::bytes(slot_name.clone()));
                items.extend(params.iter().cloned());
            }
            Self::RpcCall {
                signal_name,
                params,
            } => {
                items.push(Value::Int(RequestType::RpcCall.id()));
                items.push(Value::bytes(signal_name.clone()));
                items.extend(params.iter().cloned());
            }
            Self::InitRequest {
                class_name,
                object_name,
            } => {
                items.push(Value::Int(RequestType::InitRequest.id()));
                items.push(Value::bytes(class_name.clone()));
                items.push(Value::bytes(object_name.clone()));
            }
            Self::InitData {
                class_name,
                object_name,
                init_data,
            } => {
                items.push(Value::Int(RequestType::InitData.id()));
                items.push(Value::bytes(class_name.clone()));
                items.push(Value::bytes(object_name.clone()));
                for (key, value) in init_data.iter() {
                    items.push(Value::bytes(key.clone()));
                    items.push(value.clone());
                }
            }
            Self::HeartBeat { timestamp } => {
                items.push(Value::Int(RequestType::HeartBeat.id()));
                items.push(Value::DateTime(timestamp.0, timestamp.1));
            }
            Self::HeartBeatReply { timestamp } => {
                items.push(Value::Int(RequestType::HeartBeatReply.id()));
                items.push(Value::DateTime(timestamp.0, timestamp.1));
            }
        }
        encode_list_body_to_vec(&items)
    }

    /// Reads a message.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtoError> {
        let (items, _) = decode_list_body(bytes)?;
        Self::from_items(&items)
    }

    /// Builds a message from the elements of a list body.
    pub fn from_items(items: &[Value]) -> Result<Self, ProtoError> {
        let Some(first) = items.first() else {
            return Err(ProtoError::MalformedHandshake);
        };
        let request_type = request_type_of(first)?;
        let request_type = RequestType::from_id(request_type)
            .ok_or(ProtoError::UnknownRequestType(request_type))?;
        let rest = &items[1..];
        match request_type {
            RequestType::Sync => sync_from_items(rest),
            RequestType::RpcCall => rpc_call_from_items(rest),
            RequestType::InitRequest => init_request_from_items(rest),
            RequestType::InitData => init_data_from_items(rest),
            RequestType::HeartBeat => Ok(Self::HeartBeat {
                timestamp: timestamp_of(rest.first())?,
            }),
            RequestType::HeartBeatReply => Ok(Self::HeartBeatReply {
                timestamp: timestamp_of(rest.first())?,
            }),
        }
    }
}

/// Reads the request type from the first element of a list body.
fn request_type_of(value: &Value) -> Result<i64, ProtoError> {
    match value {
        Value::Int(value) => Ok(i64::from(*value)),
        Value::Short(value) => Ok(i64::from(*value)),
        Value::UInt(value) => Ok(i64::from(*value)),
        Value::UShort(value) => Ok(i64::from(*value)),
        _ => Err(ProtoError::MalformedHandshake),
    }
}

/// Reads a text value: a byte array or a string.
fn text_of(value: &Value) -> Result<String, ProtoError> {
    match value {
        Value::Bytes(Some(data)) => Ok(String::from_utf8_lossy(data).into_owned()),
        Value::Str(Some(text)) => Ok(text.clone()),
        _ => Err(ProtoError::MalformedHandshake),
    }
}

/// Reads the text field at `index` of `rest`.
fn text_at(rest: &[Value], index: usize) -> Result<String, ProtoError> {
    match rest.get(index) {
        Some(value) => text_of(value),
        None => Err(ProtoError::MalformedHandshake),
    }
}

/// Reads a sync call: the class, the object, the slot and the arguments.
fn sync_from_items(rest: &[Value]) -> Result<SignalMessage, ProtoError> {
    if rest.len() < 3 {
        return Err(ProtoError::MalformedHandshake);
    }
    Ok(SignalMessage::Sync {
        class_name: text_at(rest, 0)?,
        object_name: text_at(rest, 1)?,
        slot_name: text_at(rest, 2)?,
        params: rest[3..].to_vec(),
    })
}

/// Reads an RPC call: the signal and its arguments.
fn rpc_call_from_items(rest: &[Value]) -> Result<SignalMessage, ProtoError> {
    if rest.is_empty() {
        return Err(ProtoError::MalformedHandshake);
    }
    Ok(SignalMessage::RpcCall {
        signal_name: text_at(rest, 0)?,
        params: rest[1..].to_vec(),
    })
}

/// Reads an init request: the class and the object.
fn init_request_from_items(rest: &[Value]) -> Result<SignalMessage, ProtoError> {
    if rest.len() < 2 {
        return Err(ProtoError::MalformedHandshake);
    }
    Ok(SignalMessage::InitRequest {
        class_name: text_at(rest, 0)?,
        object_name: text_at(rest, 1)?,
    })
}

/// Reads the answer to an init request: the class, the object and the data.
fn init_data_from_items(rest: &[Value]) -> Result<SignalMessage, ProtoError> {
    if rest.len() < 2 {
        return Err(ProtoError::MalformedHandshake);
    }
    Ok(SignalMessage::InitData {
        class_name: text_at(rest, 0)?,
        object_name: text_at(rest, 1)?,
        init_data: init_data_map(&rest[2..])?,
    })
}

/// Reads the key/value pairs of an init data answer.
///
/// The C++ client reads the data as a flat list of key/value pairs
/// (`src/common/protocols/datastream/datastreampeer.cpp`), while the C++ core
/// sends it as one list of pairs.  Both are read here.
fn init_data_map(items: &[Value]) -> Result<Map, ProtoError> {
    if items.len() == 1 {
        match &items[0] {
            Value::List(pairs) => return pairs_from_items(pairs),
            Value::Map(map) => return Ok(map.clone()),
            _ => {}
        }
    }
    pairs_from_items(items)
}

/// Reads a flat list of key/value pairs.
fn pairs_from_items(items: &[Value]) -> Result<Map, ProtoError> {
    if items.len() % 2 != 0 {
        return Err(ProtoError::MalformedHandshake);
    }
    let mut map = Map::new();
    for pair in items.chunks_exact(2) {
        let key = text_of(&pair[0])?;
        map.insert(key, pair[1].clone());
    }
    Ok(map)
}

/// Reads a timestamp, which the core writes as a `QDateTime`.
fn timestamp_of(value: Option<&Value>) -> Result<(i32, u32), ProtoError> {
    match value {
        Some(Value::DateTime(day, millis)) => Ok((*day, *millis)),
        Some(Value::Date(day)) => Ok((*day, 0)),
        Some(Value::Long(seconds)) => Ok((0, *seconds as u32)),
        _ => Err(ProtoError::MalformedHandshake),
    }
}

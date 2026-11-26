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
Encoding of Quassel DataStream values.

Everything is big endian, like `QDataStream` in its default byte order.

The shapes that are easy to get wrong, and that the core relies on:

- A __message body__ is a _list body_: a 32 bit element count followed by the
  elements.  There is no `QVariant` type header in front of it.  Writing a
  `QVariant`-wrapped list instead makes the core read the element count from
  the type field and silently ignore the message.
- A __map key__ and a __string list element__ are plain `QString` bodies
  (length plus UTF-16BE); only the value of a map carries a `QVariant`
  header.
- A __user type__ is the type id 127, then the type name as a length
  prefixed, NUL terminated string, then the payload.
*/

use crate::value::{Map, UserType, Value};

/// Appends the wire form of a value to `out`.
///
/// A value is a 32 bit type id, an 8 bit null flag and a type specific
/// payload.
pub fn encode_value(value: &Value, out: &mut Vec<u8>) {
    out.extend_from_slice(&value.variant_type().id().to_be_bytes());
    out.push(if is_null(value) { 1 } else { 0 });
    encode_payload(value, out);
}

/// Encodes a value into a fresh buffer.
pub fn encode_value_to_vec(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    encode_value(value, &mut out);
    out
}

/// Appends the wire form of a list *body* to `out`.
///
/// This is what a Quassel message body looks like: an element count followed
/// by the elements, without a `QVariant` header in front of the count.
pub fn encode_list_body(values: &[Value], out: &mut Vec<u8>) {
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for value in values {
        encode_value(value, out);
    }
}

/// Encodes a list body into a fresh buffer.
pub fn encode_list_body_to_vec(values: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_list_body(values, &mut out);
    out
}

/// Whether the value is written with the null flag set.
fn is_null(value: &Value) -> bool {
    matches!(value, Value::Void | Value::Str(None) | Value::Bytes(None))
}

/// Appends the payload of a value to `out`.
///
/// The kinds of value that carry more than a number are written here; the
/// ones that carry a number are written by `encode_number`.
fn encode_payload(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Void => {}
        Value::Map(map) => encode_map_body(map, out),
        Value::List(items) => encode_list_body(items, out),
        Value::Str(text) => encode_qstring(text.as_deref(), out),
        Value::StringList(items) => encode_string_list_body(items, out),
        Value::Bytes(data) => encode_qbytearray(data.as_deref(), out),
        Value::DateTime(day, millis) => {
            out.extend_from_slice(&day.to_be_bytes());
            out.extend_from_slice(&millis.to_be_bytes());
        }
        Value::Variant(inner) => encode_value(inner, out),
        Value::UserType(user_type) => encode_user_type(user_type, out),
        _ => encode_number(value, out),
    }
}

/// Appends the payload of a value that carries a number.
fn encode_number(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Bool(flag) => out.push(u8::from(*flag)),
        Value::Int(number) => out.extend_from_slice(&number.to_be_bytes()),
        Value::UInt(number) => out.extend_from_slice(&number.to_be_bytes()),
        Value::QChar(code) => out.extend_from_slice(&code.to_be_bytes()),
        Value::Date(day) => out.extend_from_slice(&day.to_be_bytes()),
        Value::Time(millis) => out.extend_from_slice(&millis.to_be_bytes()),
        Value::Long(number) => out.extend_from_slice(&number.to_be_bytes()),
        Value::Short(number) => out.extend_from_slice(&number.to_be_bytes()),
        Value::SignedChar(number) => out.push(*number as u8),
        Value::ULong(number) => out.extend_from_slice(&number.to_be_bytes()),
        Value::UShort(number) => out.extend_from_slice(&number.to_be_bytes()),
        Value::UChar(number) => out.push(*number),
        // Every other kind of value is written by `encode_payload` itself.
        _ => unreachable!("only the number kinds reach `encode_number`"),
    }
}

/// Appends a `QStringList` body: an element count, then the elements.
fn encode_string_list_body(items: &[Option<String>], out: &mut Vec<u8>) {
    out.extend_from_slice(&(items.len() as u32).to_be_bytes());
    for item in items {
        encode_qstring(item.as_deref(), out);
    }
}

/// Appends a `QVariantMap` *body*: an entry count, then key/value pairs.
fn encode_map_body(map: &Map, out: &mut Vec<u8>) {
    out.extend_from_slice(&(map.len() as u32).to_be_bytes());
    for (key, value) in map.iter() {
        encode_qstring(Some(key), out);
        encode_value(value, out);
    }
}

/// Appends a `QString` body: a byte count, then UTF-16BE.
///
/// A null string is the byte count `0xffffffff` and nothing else.
fn encode_qstring(text: Option<&str>, out: &mut Vec<u8>) {
    let Some(text) = text else {
        out.extend_from_slice(&u32::MAX.to_be_bytes());
        return;
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    out.extend_from_slice(&((units.len() * 2) as u32).to_be_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_be_bytes());
    }
}

/// Appends a `QByteArray` body: a length, then the raw bytes.
///
/// A null byte array is the length `0xffffffff` and nothing else.
fn encode_qbytearray(data: Option<&[u8]>, out: &mut Vec<u8>) {
    let Some(data) = data else {
        out.extend_from_slice(&u32::MAX.to_be_bytes());
        return;
    };
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
}

/// Appends a user type: the name, then the payload.
fn encode_user_type(user_type: &UserType, out: &mut Vec<u8>) {
    encode_user_type_name(user_type.name(), out);
    match user_type {
        UserType::Id { value, .. } => out.extend_from_slice(&value.to_be_bytes()),
        UserType::MsgId { value } => out.extend_from_slice(&value.to_be_bytes()),
        UserType::BufferInfo {
            buffer_id,
            network_id,
            buffer_type,
            group_id,
            name,
        } => {
            out.extend_from_slice(&buffer_id.to_be_bytes());
            out.extend_from_slice(&network_id.to_be_bytes());
            out.extend_from_slice(&buffer_type.to_be_bytes());
            out.extend_from_slice(&group_id.to_be_bytes());
            encode_qbytearray(Some(name), out);
        }
        UserType::Map { fields, .. } => encode_map_body(fields, out),
    }
}

/// Appends a user type name: a length that counts the NUL, the name, the NUL.
///
/// The core strips trailing NULs off the name it reads, so the terminator is
/// optional on the wire, but the C++ client always sends it and so do we.
fn encode_user_type_name(name: &str, out: &mut Vec<u8>) {
    let bytes = name.as_bytes();
    out.extend_from_slice(&((bytes.len() + 1) as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    out.push(0);
}

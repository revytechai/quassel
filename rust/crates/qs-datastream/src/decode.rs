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
Decoding of Quassel DataStream values.
*/

use crate::error::DecodeError;
use crate::value::{IdType, Map, MapType, UserType, Value, VariantType};

/// The largest length the decoder accepts for a string or byte array.
///
/// The C++ core uses 64 MB for the same limit.
const MAX_LENGTH: u64 = 64 * 1024 * 1024;

/// The deepest a value may be nested.
///
/// The decoder walks into nested values, so a value that nests deeply enough
/// would exhaust the stack.  The core never nests a value this deep, so the
/// limit is far above anything real traffic carries.
const MAX_DEPTH: u32 = 64;

/// A cursor over a byte buffer.
struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn take(&mut self, count: usize, wanted: &'static str) -> Result<&'a [u8], DecodeError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(DecodeError::UnexpectedEof { wanted })?;
        if end > self.data.len() {
            return Err(DecodeError::UnexpectedEof { wanted });
        }
        let slice = &self.data[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1, "a byte")?[0])
    }

    fn i8(&mut self) -> Result<i8, DecodeError> {
        Ok(self.u8()? as i8)
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        let bytes = self.take(2, "a 16 bit number")?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn i16(&mut self) -> Result<i16, DecodeError> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        let bytes = self.take(4, "a 32 bit number")?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i32(&mut self) -> Result<i32, DecodeError> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        let bytes = self.take(8, "a 64 bit number")?;
        let mut array = [0u8; 8];
        array.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(array))
    }

    fn i64(&mut self) -> Result<i64, DecodeError> {
        Ok(self.u64()? as i64)
    }

    /// Reads a length prefix and rejects absurd ones.
    ///
    /// `0xffffffff` is the marker for a null string or byte array and is
    /// passed through; the callers that read a count use [`Reader::count`].
    fn length(&mut self) -> Result<u32, DecodeError> {
        let length = self.u32()?;
        if length != u32::MAX && u64::from(length) > MAX_LENGTH {
            return Err(DecodeError::TooLarge {
                length: u64::from(length),
                limit: MAX_LENGTH,
            });
        }
        Ok(length)
    }

    /// Reads an element or entry count.
    fn count(&mut self) -> Result<u32, DecodeError> {
        let count = self.u32()?;
        if u64::from(count) > MAX_LENGTH {
            return Err(DecodeError::TooLarge {
                length: u64::from(count),
                limit: MAX_LENGTH,
            });
        }
        Ok(count)
    }

    /// Reads a `QString` body.
    fn qstring(&mut self) -> Result<Option<String>, DecodeError> {
        let bytes = self.length()?;
        if bytes == u32::MAX {
            return Ok(None);
        }
        let raw = self.take(bytes as usize, "a string")?;
        Ok(Some(decode_utf16(raw)?))
    }

    /// Reads a `QByteArray` body.
    fn qbytearray(&mut self) -> Result<Option<Vec<u8>>, DecodeError> {
        let length = self.length()?;
        if length == u32::MAX {
            return Ok(None);
        }
        Ok(Some(self.take(length as usize, "a byte array")?.to_vec()))
    }

    /// Reads a `QVariantMap` body: an entry count, then key/value pairs.
    ///
    /// `depth` is the depth of the values in the map; they are nested one
    /// level inside the map.
    fn map_body(&mut self, depth: u32) -> Result<Map, DecodeError> {
        let count = self.count()?;
        let mut map = Map::new();
        for _ in 0..count {
            let key = self.qstring()?.ok_or(DecodeError::NullMapKey)?;
            let value = self.value(depth)?;
            map.insert(key, value);
        }
        Ok(map)
    }

    /// Reads a value: a type id, a null flag and a payload.
    ///
    /// `depth` is how deeply the value is nested; the outermost value is at
    /// depth zero.
    fn value(&mut self, depth: u32) -> Result<Value, DecodeError> {
        if depth > MAX_DEPTH {
            return Err(DecodeError::TooDeep {
                depth,
                limit: MAX_DEPTH,
            });
        }
        let type_id = self.u32()?;
        let _is_null = self.i8()?;
        let variant_type =
            VariantType::from_id(type_id).ok_or(DecodeError::UnknownVariantType(type_id))?;
        self.payload(variant_type, depth)
    }

    /// Reads the payload of a value of the given type.
    ///
    /// The kinds of value that carry more than a number are read here; the
    /// ones that carry a number are read by `number_payload`.
    fn payload(&mut self, variant_type: VariantType, depth: u32) -> Result<Value, DecodeError> {
        match variant_type {
            VariantType::QVariantMap => Ok(Value::Map(self.map_body(depth + 1)?)),
            VariantType::QVariantList => Ok(Value::List(self.list_body(Self::value, depth + 1)?)),
            VariantType::QString => Ok(Value::Str(self.qstring()?)),
            VariantType::QStringList => Ok(Value::StringList(
                self.list_body(Self::string_element, depth + 1)?,
            )),
            VariantType::QByteArray => Ok(Value::Bytes(self.qbytearray()?)),
            VariantType::QDateTime => self.date_time(),
            VariantType::QVariant => Ok(Value::Variant(Box::new(self.value(depth + 1)?))),
            VariantType::UserType => self.user_type(depth),
            _ => self.number_payload(variant_type),
        }
    }

    /// Reads the payload of a value that carries a number.
    fn number_payload(&mut self, variant_type: VariantType) -> Result<Value, DecodeError> {
        match variant_type {
            VariantType::Void => Ok(Value::Void),
            VariantType::Bool => self.u8().map(bool_value),
            VariantType::Int => self.i32().map(Value::Int),
            VariantType::UInt => self.u32().map(Value::UInt),
            VariantType::QChar => self.u16().map(Value::QChar),
            VariantType::QDate => self.i32().map(Value::Date),
            VariantType::QTime => self.u32().map(Value::Time),
            VariantType::Long => self.i64().map(Value::Long),
            VariantType::Short => self.i16().map(Value::Short),
            VariantType::Char => self.i8().map(Value::SignedChar),
            VariantType::ULong => self.u64().map(Value::ULong),
            VariantType::UShort => self.u16().map(Value::UShort),
            VariantType::UChar => self.u8().map(Value::UChar),
            _ => Err(DecodeError::UnknownVariantType(variant_type.id())),
        }
    }

    /// Reads a list body: an element count, then the elements.
    ///
    /// The elements are read by `read`, which is the same for every list.
    /// `depth` is the depth of the elements; they are nested one level
    /// inside the list.
    fn list_body<T>(
        &mut self,
        read: fn(&mut Self, u32) -> Result<T, DecodeError>,
        depth: u32,
    ) -> Result<Vec<T>, DecodeError> {
        let count = self.count()?;
        let mut items = Vec::with_capacity(count.min(1024) as usize);
        for _ in 0..count {
            items.push(read(self, depth)?);
        }
        Ok(items)
    }

    /// Reads a string list element.
    ///
    /// A string cannot nest, so the depth is of no interest here.
    fn string_element(&mut self, _depth: u32) -> Result<Option<String>, DecodeError> {
        self.qstring()
    }

    /// Reads a `QDateTime` body: the day, then the milliseconds.
    fn date_time(&mut self) -> Result<Value, DecodeError> {
        let day = self.i32()?;
        let millis = self.u32()?;
        Ok(Value::DateTime(day, millis))
    }

    /// Reads a user type name: a length that counts the NUL, then the bytes.
    fn user_type_name(&mut self) -> Result<String, DecodeError> {
        let length = self.length()?;
        let raw = self.take(length as usize, "a user type name")?;
        let trimmed = raw.strip_suffix(&[0]).unwrap_or(raw);
        Ok(String::from_utf8_lossy(trimmed).into_owned())
    }

    /// Reads a user type: the name, then the payload.
    ///
    /// `depth` is the depth of the user type; the fields of a map shaped
    /// payload are nested one level inside it, so they are read at
    /// `depth + 1`.
    fn user_type(&mut self, depth: u32) -> Result<Value, DecodeError> {
        let name = self.user_type_name()?;
        if let Some(id) = IdType::from_name(&name) {
            return Ok(Value::UserType(UserType::Id {
                name: id,
                value: self.i32()?,
            }));
        }
        if let Some(map) = MapType::from_name(&name) {
            return Ok(Value::UserType(UserType::Map {
                name: map,
                fields: self.map_body(depth + 1)?,
            }));
        }
        self.user_type_payload(&name)
    }

    /// Reads the payload of a user type whose name is fixed by its variant.
    fn user_type_payload(&mut self, name: &str) -> Result<Value, DecodeError> {
        match name {
            crate::wire::MSG_ID => Ok(Value::UserType(UserType::MsgId { value: self.i64()? })),
            crate::wire::BUFFER_INFO => {
                let buffer_id = self.i32()?;
                let network_id = self.i32()?;
                let buffer_type = self.i16()?;
                let group_id = self.i32()?;
                let name = self.qbytearray()?.unwrap_or_default();
                Ok(Value::UserType(UserType::BufferInfo {
                    buffer_id,
                    network_id,
                    buffer_type,
                    group_id,
                    name,
                }))
            }
            _ => Err(DecodeError::InvalidUserType(name.to_owned())),
        }
    }
}

/// Turns a UTF-16BE payload into text.
///
/// A payload with an odd byte count, or one that carries an unpaired
/// surrogate, is not a string the core could have written: it is rejected
/// rather than silently repaired.
fn decode_utf16(raw: &[u8]) -> Result<String, DecodeError> {
    if raw.len() % 2 != 0 {
        return Err(DecodeError::InvalidUtf16);
    }
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| DecodeError::InvalidUtf16)
}

/// The value of a boolean byte: everything but zero is true.
fn bool_value(flag: u8) -> Value {
    Value::Bool(flag != 0)
}

/// Decodes one value from the front of `input`.
///
/// Returns the value and how many bytes it used.
pub fn decode_value(input: &[u8]) -> Result<(Value, usize), DecodeError> {
    let mut reader = Reader::new(input);
    let value = reader.value(0)?;
    Ok((value, reader.position))
}

/// Decodes one value from the front of `input`, rejecting trailing bytes.
pub fn decode_value_exact(input: &[u8]) -> Result<Value, DecodeError> {
    let (value, used) = decode_value(input)?;
    if used != input.len() {
        return Err(DecodeError::TrailingBytes(input.len() - used));
    }
    Ok(value)
}

/// Decodes a list *body* from the front of `input`.
///
/// A list body is an element count followed by the elements; there is no
/// `QVariant` header in front of the count.
pub fn decode_list_body(input: &[u8]) -> Result<(Vec<Value>, usize), DecodeError> {
    let mut reader = Reader::new(input);
    let values = reader.list_body(Reader::value, 0)?;
    Ok((values, reader.position))
}

/// Decodes a list body from the front of `input`, rejecting trailing bytes.
pub fn decode_list_body_exact(input: &[u8]) -> Result<Vec<Value>, DecodeError> {
    let (values, used) = decode_list_body(input)?;
    if used != input.len() {
        return Err(DecodeError::TrailingBytes(input.len() - used));
    }
    Ok(values)
}

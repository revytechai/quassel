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
The value model of the Quassel DataStream protocol.

The Quassel core speaks a Qt-flavoured variant of `QDataStream`: every value on
the wire is a `QVariant`, which is a 32 bit type id, an 8 bit null flag and a
type specific payload.

The type ids are the ones the C++ core uses
(`src/common/serializers/serializers.h`, `enum class VariantType`).
*/

use std::cmp::Ordering;

/// The `QVariant` type ids the Quassel core understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VariantType {
    /// `QVariant::Void`.
    Void = 0,
    /// `QVariant::Bool`.
    Bool = 1,
    /// `QVariant::Int`.
    Int = 2,
    /// `QVariant::UInt`.
    UInt = 3,
    /// `QVariant::QChar`.
    QChar = 7,
    /// `QVariant::QVariantMap`.
    QVariantMap = 8,
    /// `QVariant::QVariantList`.
    QVariantList = 9,
    /// `QVariant::QString`.
    QString = 10,
    /// `QVariant::QStringList`.
    QStringList = 11,
    /// `QVariant::QByteArray`.
    QByteArray = 12,
    /// `QVariant::QDate`.
    QDate = 14,
    /// `QVariant::QTime`.
    QTime = 15,
    /// `QVariant::QDateTime`.
    QDateTime = 16,
    /// `QVariant::LongLong`.
    Long = 129,
    /// `QVariant::Short`.
    Short = 130,
    /// `QVariant::Char`.
    Char = 131,
    /// `QVariant::ULongLong`.
    ULong = 132,
    /// `QVariant::UShort`.
    UShort = 133,
    /// `QVariant::UChar`.
    UChar = 134,
    /// `QVariant::QVariant` (a nested variant).
    QVariant = 138,
    /// `QVariant::UserType`, followed by the type name and the payload.
    UserType = 127,
}

impl VariantType {
    /// The wire id of this type.
    pub const fn id(self) -> u32 {
        self as u32
    }

    /// Maps a wire id back to a type, if the core knows it.
    ///
    /// The types the core knows are the ones in `KNOWN_TYPES`, the table the
    /// codec works from.
    pub fn from_id(id: u32) -> Option<Self> {
        KNOWN_TYPES
            .iter()
            .find(|(wire, _)| *wire == id)
            .map(|(_, variant)| *variant)
    }
}

/// An ordered, key-sorted map, mirroring Qt's `QVariantMap`.
///
/// Qt's `QVariantMap` is a `QMap`, i.e. it keeps its keys sorted, and the core
/// writes them in that order.  Keeping the same invariant here means a value
/// that came off the wire re-encodes to the very same bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    entries: Vec<(String, Value)>,
}

/// Compares two strings the way Qt's `QMap<QString, _>` does: by UTF-16 code
/// unit, not by UTF-8 byte.  The two agree for the Basic Multilingual Plane
/// but differ for characters above it, because UTF-16 surrogate pairs sort
/// before `U+E000`.
fn qt_string_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.encode_utf16();
    let mut right = b.encode_utf16();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x == y => continue,
            (Some(x), Some(y)) => return x.cmp(&y),
        }
    }
}

impl Map {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a value, replacing an existing entry with the same key.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> &mut Self {
        let key = key.into();
        match self
            .entries
            .binary_search_by(|(k, _)| qt_string_cmp(k, &key))
        {
            Ok(index) => self.entries[index].1 = value,
            Err(index) => self.entries.insert(index, (key, value)),
        }
        self
    }

    /// Looks a key up.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .binary_search_by(|(k, _)| qt_string_cmp(k, key))
            .ok()
            .map(|index| &self.entries[index].1)
    }

    /// Iterates over the entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<K: Into<String>> FromIterator<(K, Value)> for Map {
    fn from_iter<T: IntoIterator<Item = (K, Value)>>(iter: T) -> Self {
        let mut map = Map::new();
        for (key, value) in iter {
            map.insert(key, value);
        }
        map
    }
}

/// Generates the name handling of a user type enum.
///
/// The wire name is part of the protocol, and the same name is written and
/// read, so both directions live in one place.
macro_rules! named_user_types {
    ( $type:ident { $( $variant:ident => $name:literal ),* $(,)? } ) => {
        impl $type {
            /// The wire name of the type.
            pub const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name, )*
                }
            }

            /// Maps a wire name back to the type, if the core knows it.
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $( $name => Some(Self::$variant), )*
                    _ => None,
                }
            }
        }
    };
}

/// The wire names of the user types that carry a 32 bit identifier.
///
/// The name is part of the wire format, and the core rejects names it does
/// not know.  Naming the type with an enum rather than with a string keeps
/// the value model closed: there is no way to build a user type the decoder
/// would reject.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdType {
    /// `BufferId`.
    BufferId,
    /// `NetworkId`.
    NetworkId,
    /// `IdentityId`.
    IdentityId,
}

named_user_types!(IdType {
    BufferId => "BufferId",
    NetworkId => "NetworkId",
    IdentityId => "IdentityId",
});

/// The wire names of the user types that carry a map.
///
/// The server list type is called `Network::Server`, **not**
/// `Network_Server`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapType {
    /// `Identity`.
    Identity,
    /// `NetworkInfo`.
    NetworkInfo,
    /// `Network::Server`.
    NetworkServer,
}

named_user_types!(MapType {
    Identity => "Identity",
    NetworkInfo => "NetworkInfo",
    NetworkServer => "Network::Server",
});

/// A value carrying a Quassel user type.
///
/// The core names user types on the wire; the name is part of the format and
/// the core rejects names it does not know ("Type name is not valid").  The
/// server list type is called `Network::Server`, **not** `Network_Server`.
///
/// The names are the [`IdType`] and [`MapType`] enums, so a value of this
/// type always names a user type the decoder reads back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserType {
    /// A 32 bit identifier: `BufferId`, `NetworkId` or `IdentityId`.
    Id {
        /// The wire name of the type.
        name: IdType,
        /// The identifier.
        value: i32,
    },
    /// A 64 bit message identifier.
    MsgId {
        /// The identifier.
        value: i64,
    },
    /// A `BufferInfo`: which buffer of which network a message belongs to.
    BufferInfo {
        /// The buffer id.
        buffer_id: i32,
        /// The network id.
        network_id: i32,
        /// The buffer type (status, channel, query, ...).
        buffer_type: i16,
        /// The group id the buffer belongs to.
        group_id: i32,
        /// The buffer name, as UTF-8 bytes (Qt stores it as a `QByteArray`).
        name: Vec<u8>,
    },
    /// A map shaped payload: `Identity`, `NetworkInfo` or `Network::Server`.
    Map {
        /// The wire name of the type.
        name: MapType,
        /// The payload fields.
        fields: Map,
    },
}

impl UserType {
    /// The wire name of the type.
    pub fn name(&self) -> &str {
        match self {
            Self::Id { name, .. } => name.name(),
            Self::Map { name, .. } => name.name(),
            Self::MsgId { .. } => crate::wire::MSG_ID,
            Self::BufferInfo { .. } => crate::wire::BUFFER_INFO,
        }
    }
}

/// A value on the Quassel DataStream wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// `QVariant::Void`.
    Void,
    /// A boolean.
    Bool(bool),
    /// A signed 32 bit integer.
    Int(i32),
    /// An unsigned 32 bit integer.
    UInt(u32),
    /// A UTF-16 code unit.
    QChar(u16),
    /// A string map.
    Map(Map),
    /// A list of values.
    List(Vec<Value>),
    /// A string; `None` is a null `QString`.
    Str(Option<String>),
    /// A list of strings.
    StringList(Vec<Option<String>>),
    /// A byte array; `None` is a null `QByteArray`.
    Bytes(Option<Vec<u8>>),
    /// A date, as a Julian day number.
    Date(i32),
    /// A time, as milliseconds since midnight.
    Time(u32),
    /// A date and a time.
    DateTime(i32, u32),
    /// A signed 64 bit integer.
    Long(i64),
    /// A signed 16 bit integer.
    Short(i16),
    /// A signed 8 bit integer.
    SignedChar(i8),
    /// An unsigned 64 bit integer.
    ULong(u64),
    /// An unsigned 16 bit integer.
    UShort(u16),
    /// An unsigned 8 bit integer.
    UChar(u8),
    /// A nested variant.
    Variant(Box<Value>),
    /// A user type.
    UserType(UserType),
}

/// The kinds of value and the Qt type each of them is written with.
///
/// The table is a macro, because a kind of value is a pattern and a list of
/// patterns cannot be written as data.  It is the one place that says which
/// kind of value is written with which type: the encoder writes the wire id
/// of that type, and the decoder reads the kind back from the id.
macro_rules! value_types {
    ($( $pattern:pat => $type:ident ),* $(,)?) => {
        impl Value {
            /// The type this value is written with.
            pub fn variant_type(&self) -> VariantType {
                match self {
                    $( $pattern => VariantType::$type, )*
                }
            }
        }

        /// Every type the core knows, with the wire id it is written with.
        const KNOWN_TYPES: &[(u32, VariantType)] = &[
            $( (VariantType::$type as u32, VariantType::$type), )*
        ];
    };
}

value_types! {
    Value::Void => Void,
    Value::Bool(_) => Bool,
    Value::Int(_) => Int,
    Value::UInt(_) => UInt,
    Value::QChar(_) => QChar,
    Value::Map(_) => QVariantMap,
    Value::List(_) => QVariantList,
    Value::Str(_) => QString,
    Value::StringList(_) => QStringList,
    Value::Bytes(_) => QByteArray,
    Value::Date(_) => QDate,
    Value::Time(_) => QTime,
    Value::DateTime(_, _) => QDateTime,
    Value::Long(_) => Long,
    Value::Short(_) => Short,
    Value::SignedChar(_) => Char,
    Value::ULong(_) => ULong,
    Value::UShort(_) => UShort,
    Value::UChar(_) => UChar,
    Value::Variant(_) => QVariant,
    Value::UserType(_) => UserType,
}

impl Value {
    /// A non-null string.
    pub fn string(text: impl Into<String>) -> Self {
        Self::Str(Some(text.into()))
    }

    /// A non-null byte array.
    pub fn bytes(data: impl Into<Vec<u8>>) -> Self {
        Self::Bytes(Some(data.into()))
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self::UInt(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Long(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::string(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Str(Some(value))
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(Some(value))
    }
}

impl From<Map> for Value {
    fn from(value: Map) -> Self {
        Self::Map(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Self::List(value)
    }
}

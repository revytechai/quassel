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

//! Property tests: whatever goes in comes out again, byte for byte.

use proptest::prelude::*;
use qs_datastream::{
    decode_list_body_exact, decode_value_exact, encode_list_body_to_vec, encode_value_to_vec,
    IdType, Map, MapType, UserType, Value,
};

/// The user types that carry a 32 bit identifier.
fn arb_id_type() -> impl Strategy<Value = IdType> {
    prop_oneof![
        Just(IdType::BufferId),
        Just(IdType::NetworkId),
        Just(IdType::IdentityId),
    ]
}

/// The user types that carry a map.
fn arb_map_type() -> impl Strategy<Value = MapType> {
    prop_oneof![
        Just(MapType::Identity),
        Just(MapType::NetworkInfo),
        Just(MapType::NetworkServer),
    ]
}

fn arb_leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Void),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(Value::Int),
        any::<u32>().prop_map(Value::UInt),
        any::<u16>().prop_map(Value::QChar),
        any::<i64>().prop_map(Value::Long),
        any::<i16>().prop_map(Value::Short),
        any::<i8>().prop_map(Value::SignedChar),
        any::<u64>().prop_map(Value::ULong),
        any::<u16>().prop_map(Value::UShort),
        any::<u8>().prop_map(Value::UChar),
        any::<i32>().prop_map(Value::Date),
        any::<u32>().prop_map(Value::Time),
        proptest::collection::vec(any::<u8>(), 0..24).prop_map(|data| Value::Bytes(Some(data))),
        Just(Value::Bytes(None)),
        Just(Value::Str(None)),
        "[^\\x00]{0,24}".prop_map(Value::string),
        proptest::collection::vec(any::<u16>(), 0..12)
            .prop_map(|units| Value::string(String::from_utf16_lossy(&units))),
    ]
}

fn arb_value() -> impl Strategy<Value = Value> {
    let leaf = arb_leaf();
    leaf.prop_recursive(4, 40, 12, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..5).prop_map(Value::List),
            proptest::collection::vec(("[\u{20}-\u{7e}]{0,6}", inner.clone()), 0..5).prop_map(
                |entries| {
                    let mut map = Map::new();
                    for (key, value) in entries {
                        map.insert(key, value);
                    }
                    Value::Map(map)
                }
            ),
            proptest::collection::vec(inner.clone(), 0..4).prop_map(|items| {
                Value::StringList(
                    items
                        .into_iter()
                        .map(|item| match item {
                            Value::Str(text) => text,
                            _ => None,
                        })
                        .collect(),
                )
            }),
            (arb_id_type(), any::<i32>())
                .prop_map(|(name, value)| { Value::UserType(UserType::Id { name, value }) }),
            any::<i64>().prop_map(|value| Value::UserType(UserType::MsgId { value })),
            (
                any::<i32>(),
                any::<i32>(),
                any::<i16>(),
                any::<i32>(),
                proptest::collection::vec(any::<u8>(), 0..8),
            )
                .prop_map(|(buffer_id, network_id, buffer_type, group_id, name)| {
                    Value::UserType(UserType::BufferInfo {
                        buffer_id,
                        network_id,
                        buffer_type,
                        group_id,
                        name,
                    })
                }),
            (
                arb_map_type(),
                proptest::collection::vec(("[\u{20}-\u{7e}]{0,6}", inner.clone()), 0..4),
            )
                .prop_map(|(name, entries)| {
                    let mut map = Map::new();
                    for (key, value) in entries {
                        map.insert(key, value);
                    }
                    Value::UserType(UserType::Map { name, fields: map })
                }),
            inner.prop_map(|inner| Value::Variant(Box::new(inner))),
        ]
    })
}

proptest! {
    #[test]
    fn value_round_trips(value in arb_value()) {
        let encoded = encode_value_to_vec(&value);
        let decoded = decode_value_exact(&encoded).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(decoded, value);
    }

    #[test]
    fn encoding_is_canonical(value in arb_value()) {
        // Whatever comes off the wire re-encodes to the very same bytes.
        let encoded = encode_value_to_vec(&value);
        let decoded = decode_value_exact(&encoded).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let re_encoded = encode_value_to_vec(&decoded);
        prop_assert_eq!(re_encoded, encoded);
    }

    #[test]
    fn list_body_round_trips(values in proptest::collection::vec(arb_value(), 0..8)) {
        let encoded = encode_list_body_to_vec(&values);
        let decoded = decode_list_body_exact(&encoded).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(decoded, values);
    }

    #[test]
    fn a_truncated_value_is_never_accepted(value in arb_value()) {
        let encoded = encode_value_to_vec(&value);
        if encoded.is_empty() {
            return Ok(());
        }
        // Every proper prefix must either fail or decode to something else.
        for cut in 1..encoded.len() {
            if let Ok(decoded) = decode_value_exact(&encoded[..cut]) {
                prop_assert_ne!(encode_value_to_vec(&decoded), encoded[..cut].to_vec());
            }
        }
    }
}

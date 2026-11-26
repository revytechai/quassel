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

//! Round trip tests for the Quassel DataStream codec.
//!
//! The expected byte strings are the ones the C++ core and the scripted
//! client (`quassel-client.py`) put on the wire, so a change that breaks the
//! compatibility with the real core breaks a test here.
//!
//! The one place where this crate is deliberately stricter than the wire is
//! the null flag: the core reads it and throws it away, the null-ness of a
//! string or byte array comes from the `0xffffffff` length.  This codec
//! writes the flag as `1` for a null value, so the bytes differ from a client
//! that leaves the flag at `0` while meaning the same thing.

use qs_datastream::{
    buffer_id, decode_list_body_exact, decode_value_exact, encode_list_body_to_vec,
    encode_value_to_vec, identity, identity_id, network_id, network_info, network_server, IdType,
    Map, MapType, UserType, Value,
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn string_is_a_type_id_a_null_flag_and_utf16be() {
    assert_eq!(
        hex(&encode_value_to_vec(&Value::string("A"))),
        "0000000a00000000020041"
    );
}

#[test]
fn byte_array_is_a_type_id_a_null_flag_and_the_bytes() {
    assert_eq!(
        hex(&encode_value_to_vec(&Value::Bytes(Some(vec![1, 2, 3])))),
        "0000000c0000000003010203"
    );
}

#[test]
fn a_null_string_is_the_null_length() {
    assert_eq!(
        hex(&encode_value_to_vec(&Value::Str(None))),
        "0000000a01ffffffff"
    );
}

#[test]
fn a_null_byte_array_is_the_null_length() {
    assert_eq!(
        hex(&encode_value_to_vec(&Value::Bytes(None))),
        "0000000c01ffffffff"
    );
}

#[test]
fn a_map_key_is_a_plain_string_without_a_type_header() {
    let mut map = Map::new();
    map.insert("k", Value::Int(7));
    assert_eq!(
        hex(&encode_value_to_vec(&Value::Map(map))),
        "00000008000000000100000002006b000000020000000007"
    );
}

#[test]
fn a_string_list_element_is_a_plain_string() {
    assert_eq!(
        hex(&encode_value_to_vec(&Value::StringList(vec![Some(
            "a".to_owned()
        )]))),
        "0000000b0000000001000000020061"
    );
}

#[test]
fn a_user_type_carries_its_name_and_payload() {
    let value = Value::UserType(UserType::Id {
        name: IdType::NetworkId,
        value: 42,
    });
    assert_eq!(
        hex(&encode_value_to_vec(&value)),
        "0000007f000000000a4e6574776f726b4964000000002a"
    );
}

#[test]
fn the_server_list_type_is_named_with_a_double_colon() {
    let value = network_server("irc.example.org", 6697, true, false);
    let encoded = hex(&encode_value_to_vec(&value));
    // The name length counts the NUL: "Network::Server" is 15 bytes.
    assert!(encoded.starts_with("0000007f00000000104e6574776f726b3a3a53657276657200"));
    // The core rejects "Network_Server", so the name must not contain one.
    assert!(!encoded.contains("4e6574776f726b5f536572766572"));
}

#[test]
fn a_list_body_has_no_variant_header_in_front_of_the_count() {
    assert_eq!(
        hex(&encode_list_body_to_vec(&[Value::string("A")])),
        "000000010000000a00000000020041"
    );

    // A QVariant-wrapped list starts with the type id 9 instead.
    assert_eq!(
        hex(&encode_value_to_vec(&Value::List(vec![Value::string("A")]))),
        "0000000900000000010000000a00000000020041"
    );
}

#[test]
fn values_round_trip() {
    let mut map = Map::new();
    map.insert("NetworkName", Value::string("EFnet"));
    map.insert("Port", Value::UInt(6697));

    let values = vec![
        Value::Void,
        Value::Bool(true),
        Value::Int(-5),
        Value::UInt(4000),
        Value::QChar(0x263a),
        Value::Map(map),
        Value::List(vec![Value::Int(1), Value::string("two")]),
        Value::string("hello"),
        Value::Str(None),
        Value::StringList(vec![Some("a".to_owned()), None]),
        Value::Bytes(Some(vec![0, 255])),
        Value::Bytes(None),
        Value::Date(2460000),
        Value::Time(3661000),
        Value::DateTime(2460000, 3661000),
        Value::Long(i64::MIN),
        Value::Short(-32768),
        Value::SignedChar(-128),
        Value::ULong(u64::MAX),
        Value::UShort(65535),
        Value::UChar(255),
        Value::Variant(Box::new(Value::Int(3))),
        Value::UserType(UserType::Id {
            name: IdType::NetworkId,
            value: 7,
        }),
        Value::UserType(UserType::MsgId { value: 1_234_567 }),
        Value::UserType(UserType::BufferInfo {
            buffer_id: 1,
            network_id: 2,
            buffer_type: 3,
            group_id: 4,
            name: b"#chan".to_vec(),
        }),
    ];

    for value in values {
        let encoded = encode_value_to_vec(&value);
        let decoded = decode_value_exact(&encoded)
            .unwrap_or_else(|error| panic!("decoding {value:?} failed: {error}"));
        assert_eq!(decoded, value, "round trip changed the value");
    }
}

#[test]
fn a_network_info_round_trips() {
    let value = network_info(
        "EFnet",
        vec![network_server("irc.efnet.org", 6697, true, false)],
        1,
        Some(b"UTF-8".to_vec()),
        Some(b"UTF-8".to_vec()),
        Some(b"UTF-8".to_vec()),
    );
    let encoded = encode_value_to_vec(&value);
    let decoded = decode_value_exact(&encoded).unwrap();
    assert_eq!(decoded, value);

    let Value::UserType(UserType::Map { name, fields }) = &decoded else {
        panic!("expected a map shaped user type");
    };
    assert_eq!(name, &MapType::NetworkInfo);
    assert_eq!(fields.get("NetworkName"), Some(&Value::string("EFnet")));
    assert_eq!(
        fields.get("CodecForEncoding"),
        Some(&Value::Bytes(Some(b"UTF-8".to_vec())))
    );
}

#[test]
fn an_identity_round_trips() {
    let value = identity(1, "alice");
    let encoded = encode_value_to_vec(&value);
    assert_eq!(decode_value_exact(&encoded).unwrap(), value);
}

#[test]
fn a_list_body_round_trips() {
    let values = vec![
        Value::string("ClientLogin"),
        Value::string("alice"),
        Value::string("secret"),
    ];
    let body = encode_list_body_to_vec(&values);
    assert_eq!(decode_list_body_exact(&body).unwrap(), values);
}

#[test]
fn an_unknown_variant_type_is_rejected() {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(&99u32.to_be_bytes());
    encoded.push(0);
    assert!(decode_value_exact(&encoded).is_err());
}

#[test]
fn an_unknown_user_type_name_is_rejected() {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(&127u32.to_be_bytes());
    encoded.push(0);
    encoded.extend_from_slice(&11u32.to_be_bytes());
    encoded.extend_from_slice(b"Network_Srv");
    encoded.push(0);
    assert!(decode_value_exact(&encoded).is_err());
}

/// Every user type the crate can build must survive a round trip.
///
/// The name of a user type is part of the wire format, and the decoder only
/// reads the names the core knows.  The names are typed, so a value built
/// here always names a user type the decoder reads back.
#[test]
fn every_user_type_constructor_round_trips() {
    let mut fields = Map::new();
    fields.insert("Key", Value::Int(1));

    let values = vec![
        network_id(1),
        identity_id(2),
        buffer_id(3),
        network_server("irc.example.org", 6697, true, false),
        network_info("EFnet", Vec::new(), 1, None, None, None),
        identity(4, "alice"),
        Value::UserType(UserType::Id {
            name: IdType::BufferId,
            value: 5,
        }),
        Value::UserType(UserType::Id {
            name: IdType::NetworkId,
            value: 6,
        }),
        Value::UserType(UserType::Id {
            name: IdType::IdentityId,
            value: 7,
        }),
        Value::UserType(UserType::Map {
            name: MapType::Identity,
            fields: fields.clone(),
        }),
        Value::UserType(UserType::Map {
            name: MapType::NetworkInfo,
            fields: fields.clone(),
        }),
        Value::UserType(UserType::Map {
            name: MapType::NetworkServer,
            fields,
        }),
        Value::UserType(UserType::MsgId { value: 8 }),
        Value::UserType(UserType::BufferInfo {
            buffer_id: 1,
            network_id: 2,
            buffer_type: 3,
            group_id: 4,
            name: b"#chan".to_vec(),
        }),
    ];

    for value in values {
        let encoded = encode_value_to_vec(&value);
        let decoded = decode_value_exact(&encoded)
            .unwrap_or_else(|error| panic!("decoding {value:?} failed: {error}"));
        assert_eq!(decoded, value, "round trip changed the value");
    }
}

/// The encoder writes the names the decoder reads.
///
/// A name the decoder does not know is a name the encoder cannot write: the
/// name of a user type is an enum, not a string.
#[test]
fn the_encoder_writes_the_names_the_decoder_reads() {
    for name in [IdType::BufferId, IdType::NetworkId, IdType::IdentityId] {
        let value = Value::UserType(UserType::Id { name, value: 1 });
        let encoded = encode_value_to_vec(&value);
        assert!(
            encoded
                .windows(name.name().len())
                .any(|window| window == name.name().as_bytes()),
            "{name:?} is not written on the wire"
        );
        assert_eq!(decode_value_exact(&encoded).unwrap(), value);
    }

    for name in [
        MapType::Identity,
        MapType::NetworkInfo,
        MapType::NetworkServer,
    ] {
        let value = Value::UserType(UserType::Map {
            name,
            fields: Map::new(),
        });
        let encoded = encode_value_to_vec(&value);
        assert!(
            encoded
                .windows(name.name().len())
                .any(|window| window == name.name().as_bytes()),
            "{name:?} is not written on the wire"
        );
        assert_eq!(decode_value_exact(&encoded).unwrap(), value);
    }
}

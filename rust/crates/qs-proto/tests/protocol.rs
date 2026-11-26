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

//! Tests for the connection handshake and the signal proxy layer.

use qs_datastream::{decode_list_body, encode_list_body_to_vec, Value};
use qs_proto::{
    frame, select_protocol, ConnectionFeatures, Features, FrameDecoder, Handshake,
    HandshakeMessage, HandshakeReply, ProtocolDescriptor, ProtocolType, SignalMessage,
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_handshake_is_the_probe_the_scripted_client_sends() {
    // quassel-client.py sends the magic with the encryption bit, then the
    // DataStream protocol, then the legacy protocol marked as the last one.
    let bytes = hex(&Handshake {
        features: ConnectionFeatures::ENCRYPTION,
        protocols: vec![
            ProtocolDescriptor::new(ProtocolType::DataStream, 0),
            ProtocolDescriptor {
                protocol: ProtocolType::Legacy,
                features: 0,
                last: true,
            },
        ],
    }
    .encode());
    assert_eq!(bytes, "42b33f010000000280000001");
}

#[test]
fn a_handshake_round_trips() {
    let handshake = Handshake {
        features: ConnectionFeatures::ENCRYPTION | ConnectionFeatures::COMPRESSION,
        protocols: vec![
            ProtocolDescriptor::new(ProtocolType::DataStream, 0),
            ProtocolDescriptor {
                protocol: ProtocolType::Legacy,
                features: 0,
                last: true,
            },
        ],
    };
    let encoded = handshake.encode();
    let (decoded, used) = Handshake::decode(&encoded).unwrap();
    assert_eq!(used, encoded.len());
    assert_eq!(decoded, handshake);
}

#[test]
fn a_handshake_that_is_not_one_is_rejected() {
    let bytes = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02];
    assert!(Handshake::decode(&bytes).is_err());
}

#[test]
fn the_core_picks_the_first_protocol_it_speaks() {
    let offered = vec![
        ProtocolDescriptor::new(ProtocolType::DataStream, 0),
        ProtocolDescriptor::new(ProtocolType::Legacy, 0),
    ];
    assert_eq!(select_protocol(&offered), Some(ProtocolType::DataStream));

    // The core has no legacy codec, so a client that offers only the legacy
    // protocol is not answered as if the core had understood it.
    let legacy_only = vec![ProtocolDescriptor::new(ProtocolType::Legacy, 0)];
    assert_eq!(select_protocol(&legacy_only), None);

    assert_eq!(select_protocol(&[]), None);
}

#[test]
fn the_core_reply_is_one_word() {
    let reply = HandshakeReply {
        protocol: ProtocolType::DataStream,
        features: 0,
        connection_features: ConnectionFeatures::from_bits(ConnectionFeatures::ENCRYPTION),
    };
    let bytes = reply.encode();
    assert_eq!(hex(&bytes), "01000002");
    assert_eq!(HandshakeReply::decode(bytes).unwrap(), reply);
    assert!(reply.connection_features.encryption());
    assert!(!reply.connection_features.compression());
}

#[test]
fn the_features_of_a_client_are_all_features() {
    let features = Features::all();
    assert!(features.has("ExtendedFeatures"));
    assert!(features.has("SkipIrcCaps"));
    // 0x0100 was the DCC file transfer bit, which is never in use.
    assert_eq!(features.to_legacy(), 0xfeff);
    assert!(features.unknown().is_empty());
}

#[test]
fn the_legacy_feature_bits_survive_a_round_trip() {
    let features = Features::from_legacy(0xffff);
    // 0x0100 was the DCC file transfer bit, which is never in use.
    assert_eq!(features.to_legacy(), 0xfeff);
    assert!(features.has("SynchronizedMarkerLine"));
    assert!(features.has("ExtendedFeatures"));
    // The features past the legacy ones have no bit, so they are not in the
    // legacy set.
    assert!(!features.has("LongTime"));
}

#[test]
fn a_client_init_carries_the_feature_list() {
    let message = HandshakeMessage::ClientInit {
        features: Features::all(),
        client_version: "0.14.50".to_owned(),
        client_date: "2026-11-26".to_owned(),
    };
    let encoded = message.encode();
    let decoded = HandshakeMessage::decode(&encoded).unwrap();
    assert_eq!(decoded, message);

    // The wire form is a flat list of byte array keys and values.
    let (items, _) = decode_list_body(&encoded).unwrap();
    assert!(items.len() % 2 == 0);
    assert_eq!(
        items[0],
        Value::Bytes(Some(b"MsgType".to_vec())),
        "the keys are byte arrays"
    );
}

#[test]
fn a_login_carries_the_user_and_the_password() {
    let message = HandshakeMessage::ClientLogin {
        user: "alice".to_owned(),
        password: "secret".to_owned(),
    };
    let encoded = message.encode();
    assert_eq!(HandshakeMessage::decode(&encoded).unwrap(), message);
}

#[test]
fn a_session_init_carries_the_state() {
    let message = HandshakeMessage::SessionInit {
        session_state: qs_proto::SessionState {
            identities: vec![Value::string("alice")],
            buffer_infos: vec![Value::Int(1)],
            network_ids: vec![Value::Int(2)],
        },
    };
    let encoded = message.encode();
    assert_eq!(HandshakeMessage::decode(&encoded).unwrap(), message);
}

#[test]
fn a_sync_call_round_trips() {
    let message = SignalMessage::Sync {
        class_name: "BufferSyncer".to_owned(),
        object_name: "1".to_owned(),
        slot_name: "requestRemoveBuffer".to_owned(),
        params: vec![Value::Int(1)],
    };
    let encoded = message.encode();
    assert_eq!(SignalMessage::decode(&encoded).unwrap(), message);
}

#[test]
fn an_rpc_call_round_trips() {
    let message = SignalMessage::RpcCall {
        signal_name: "2createNetwork(NetworkInfo,QStringList)".to_owned(),
        params: vec![
            Value::string("EFnet"),
            Value::StringList(vec![Some("#chan".to_owned())]),
        ],
    };
    let encoded = message.encode();
    assert_eq!(SignalMessage::decode(&encoded).unwrap(), message);
}

#[test]
fn an_init_data_round_trips() {
    let mut init_data = qs_datastream::Map::new();
    init_data.insert("networkName", Value::string("EFnet"));
    let message = SignalMessage::InitData {
        class_name: "CoreInfo".to_owned(),
        object_name: "CoreInfo".to_owned(),
        init_data,
    };
    let encoded = message.encode();
    assert_eq!(SignalMessage::decode(&encoded).unwrap(), message);
}

#[test]
fn a_message_is_length_prefixed() {
    let payload = vec![1u8, 2, 3];
    let framed = frame(&payload);
    assert_eq!(hex(&framed), "00000003010203");

    let mut decoder = FrameDecoder::new();
    decoder.push(&framed[..2]);
    assert!(
        decoder.next_frame().unwrap().is_none(),
        "not enough data yet"
    );
    decoder.push(&framed[2..]);
    assert_eq!(decoder.next_frame().unwrap(), Some(payload));
    assert!(decoder.next_frame().unwrap().is_none());
    assert!(decoder.is_empty());
}

#[test]
fn an_oversized_message_is_refused() {
    let mut decoder = FrameDecoder::with_max_size(16);
    decoder.push(&[0xff, 0xff, 0xff, 0xff]);
    assert!(decoder.next_frame().is_err());
}

#[test]
fn a_client_init_with_only_the_legacy_mask_keeps_its_features() {
    // An older client sends the legacy mask and no feature list.  The mask
    // is then the whole feature set; dropping it loses what the client
    // negotiated.
    let items = vec![
        Value::bytes(b"MsgType".to_vec()),
        Value::string("ClientInit"),
        Value::bytes(b"Features".to_vec()),
        Value::UInt(0x0002),
        Value::bytes(b"ClientVersion".to_vec()),
        Value::string("0.14.50"),
    ];
    let encoded = encode_list_body_to_vec(&items);
    let decoded = HandshakeMessage::decode(&encoded).unwrap();
    let HandshakeMessage::ClientInit { features, .. } = decoded else {
        panic!("expected a ClientInit");
    };
    assert!(features.has("SaslAuthentication"));
    assert!(!features.has("SynchronizedMarkerLine"));
}

#[test]
fn an_init_data_with_the_pairs_in_one_list_is_read() {
    // The C++ core sends the pairs as one list
    // (src/common/protocols/datastream/datastreampeer.cpp).
    let items = vec![
        Value::Int(4),
        Value::bytes(b"CoreInfo".to_vec()),
        Value::bytes(b"CoreInfo".to_vec()),
        Value::List(vec![
            Value::bytes(b"networkName".to_vec()),
            Value::string("EFnet"),
        ]),
    ];
    let encoded = encode_list_body_to_vec(&items);
    let decoded = SignalMessage::decode(&encoded).unwrap();
    let SignalMessage::InitData { init_data, .. } = decoded else {
        panic!("expected an InitData");
    };
    assert_eq!(init_data.get("networkName"), Some(&Value::string("EFnet")));
}

#[test]
fn an_init_data_is_encoded_as_the_flat_pairs_the_cpp_client_reads() {
    // The C++ client reads the data as a flat list of key/value pairs, so
    // that is the form the core writes.
    let mut init_data = qs_datastream::Map::new();
    init_data.insert("networkName", Value::string("EFnet"));
    let message = SignalMessage::InitData {
        class_name: "CoreInfo".to_owned(),
        object_name: "CoreInfo".to_owned(),
        init_data,
    };
    let encoded = message.encode();
    let (items, _) = decode_list_body(&encoded).unwrap();
    assert_eq!(items.len(), 5, "type, class, object, key, value");
    assert_eq!(items[3], Value::bytes(b"networkName".to_vec()));
}

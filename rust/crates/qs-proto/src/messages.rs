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
The messages of the handshake phase.

A handshake message is a flat list of alternating byte array keys and values,
the way the C++ core writes them:

```text
QByteArray "MsgType"  QVariant "ClientInit"  QByteArray "Features"  ...
```

The keys are byte arrays, not strings; the core turns them into strings with
`QString::fromUtf8`.
*/

use qs_datastream::{decode_list_body, encode_list_body_to_vec, Map, Value};

use crate::error::ProtoError;
use crate::features::Features;

/// The state the core sends after a successful login.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionState {
    /// The user's identities.
    pub identities: Vec<Value>,
    /// The user's buffers.
    pub buffer_infos: Vec<Value>,
    /// The user's networks.
    pub network_ids: Vec<Value>,
}

/// A message of the handshake phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandshakeMessage {
    /// The client announces itself.
    ClientInit {
        /// The features the client supports.
        features: Features,
        /// The client's version.
        client_version: String,
        /// The client's build date.
        client_date: String,
    },
    /// The core refuses the client.
    ClientInitReject {
        /// Why.
        error: String,
    },
    /// The core accepts the client.
    ClientInitAck {
        /// The features the core supports.
        features: Features,
        /// Whether the core is configured.
        configured: bool,
        /// The storage backends the core offers.
        storage_backends: Vec<Value>,
        /// The authenticators the core offers.
        authenticators: Vec<Value>,
    },
    /// The core asks the client to set it up.
    CoreSetupData {
        /// The admin user name.
        admin_user: String,
        /// The admin password.
        admin_password: String,
        /// The backend to use.
        backend: String,
        /// The backend's setup data.
        setup_data: Map,
        /// The authenticator to use.
        authenticator: String,
        /// The authenticator's setup data.
        auth_setup_data: Map,
    },
    /// The core refuses the setup.
    CoreSetupReject {
        /// Why.
        error: String,
    },
    /// The setup is done.
    CoreSetupAck,
    /// The client logs in.
    ClientLogin {
        /// The user name.
        user: String,
        /// The password.
        password: String,
    },
    /// The core refuses the login.
    ClientLoginReject {
        /// Why.
        error: String,
    },
    /// The login succeeded.
    ClientLoginAck,
    /// The core sends the session state.
    SessionInit {
        /// The session state.
        session_state: SessionState,
    },
}

impl HandshakeMessage {
    /// The wire form of the message.
    pub fn encode(&self) -> Vec<u8> {
        let fields = match self {
            Self::ClientInit {
                features,
                client_version,
                client_date,
            } => client_init_fields(features, client_version, client_date),
            Self::ClientInitReject { error } => error_fields("ClientInitReject", error),
            Self::ClientInitAck {
                features,
                configured,
                storage_backends,
                authenticators,
            } => client_init_ack_fields(features, *configured, storage_backends, authenticators),
            Self::CoreSetupData {
                admin_user,
                admin_password,
                backend,
                setup_data,
                authenticator,
                auth_setup_data,
            } => core_setup_data_fields(
                admin_user,
                admin_password,
                backend,
                setup_data,
                authenticator,
                auth_setup_data,
            ),
            Self::CoreSetupReject { error } => error_fields("CoreSetupReject", error),
            Self::CoreSetupAck => message_type_fields("CoreSetupAck"),
            Self::ClientLogin { user, password } => client_login_fields(user, password),
            Self::ClientLoginReject { error } => error_fields("ClientLoginReject", error),
            Self::ClientLoginAck => message_type_fields("ClientLoginAck"),
            Self::SessionInit { session_state } => session_init_fields(session_state),
        };
        encode_fields(&fields)
    }

    /// Reads a handshake message.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtoError> {
        let (items, _) = decode_list_body(bytes)?;
        let fields = decode_fields(&items)?;
        let message_type = fields
            .get("MsgType")
            .and_then(Value::as_str)
            .ok_or(ProtoError::MalformedHandshake)?
            .to_owned();
        match message_type.as_str() {
            "ClientInit" => decode_client_init(&fields),
            "ClientInitReject" => Ok(Self::ClientInitReject {
                error: error_of(&fields),
            }),
            "ClientInitAck" => decode_client_init_ack(&fields),
            "CoreSetupData" => decode_core_setup_data(&fields),
            "CoreSetupReject" => Ok(Self::CoreSetupReject {
                error: error_of(&fields),
            }),
            "CoreSetupAck" => Ok(Self::CoreSetupAck),
            "ClientLogin" => decode_client_login(&fields),
            "ClientLoginReject" => Ok(Self::ClientLoginReject {
                error: error_of(&fields),
            }),
            "ClientLoginAck" => Ok(Self::ClientLoginAck),
            "SessionInit" => decode_session_init(&fields),
            other => Err(ProtoError::Protocol(format!(
                "unknown protocol message of type {other}"
            ))),
        }
    }
}

/// The fields of a `ClientInit` message.
fn client_init_fields(
    features: &Features,
    client_version: &str,
    client_date: &str,
) -> Vec<(&'static str, Value)> {
    vec![
        ("MsgType", Value::string("ClientInit")),
        ("Features", Value::UInt(features.to_legacy())),
        (
            "FeatureList",
            Value::StringList(features.to_names().into_iter().map(Some).collect()),
        ),
        ("ClientVersion", Value::string(client_version.to_owned())),
        ("ClientDate", Value::string(client_date.to_owned())),
    ]
}

/// The fields of a `ClientInitAck` message.
fn client_init_ack_fields(
    features: &Features,
    configured: bool,
    storage_backends: &[Value],
    authenticators: &[Value],
) -> Vec<(&'static str, Value)> {
    vec![
        ("MsgType", Value::string("ClientInitAck")),
        (
            "FeatureList",
            Value::StringList(features.to_names().into_iter().map(Some).collect()),
        ),
        ("CoreFeatures", Value::UInt(features.to_legacy())),
        ("LoginEnabled", Value::Bool(configured)),
        ("Configured", Value::Bool(configured)),
        ("StorageBackends", Value::List(storage_backends.to_vec())),
        ("Authenticators", Value::List(authenticators.to_vec())),
    ]
}

/// The fields of a `CoreSetupData` message.
fn core_setup_data_fields(
    admin_user: &str,
    admin_password: &str,
    backend: &str,
    setup_data: &Map,
    authenticator: &str,
    auth_setup_data: &Map,
) -> Vec<(&'static str, Value)> {
    let mut inner = Map::new();
    inner.insert("AdminUser", Value::string(admin_user.to_owned()));
    inner.insert("AdminPasswd", Value::string(admin_password.to_owned()));
    inner.insert("Backend", Value::string(backend.to_owned()));
    inner.insert("ConnectionProperties", Value::Map(setup_data.clone()));
    inner.insert("Authenticator", Value::string(authenticator.to_owned()));
    inner.insert("AuthProperties", Value::Map(auth_setup_data.clone()));
    vec![
        ("MsgType", Value::string("CoreSetupData")),
        ("SetupData", Value::Map(inner)),
    ]
}

/// The fields of a `ClientLogin` message.
fn client_login_fields(user: &str, password: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("MsgType", Value::string("ClientLogin")),
        ("User", Value::string(user.to_owned())),
        ("Password", Value::string(password.to_owned())),
    ]
}

/// The fields of a `SessionInit` message.
fn session_init_fields(session_state: &SessionState) -> Vec<(&'static str, Value)> {
    let mut state = Map::new();
    state.insert(
        "BufferInfos",
        Value::List(session_state.buffer_infos.clone()),
    );
    state.insert("NetworkIds", Value::List(session_state.network_ids.clone()));
    state.insert("Identities", Value::List(session_state.identities.clone()));
    vec![
        ("MsgType", Value::string("SessionInit")),
        ("SessionState", Value::Map(state)),
    ]
}

/// The fields of a message that only says what it is.
fn message_type_fields(message_type: &str) -> Vec<(&'static str, Value)> {
    vec![("MsgType", Value::string(message_type))]
}

/// The fields of a message that refuses something.
fn error_fields(message_type: &str, error: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("MsgType", Value::string(message_type)),
        ("Error", Value::string(error.to_owned())),
    ]
}

/// Reads a `ClientInit` message.
fn decode_client_init(fields: &Map) -> Result<HandshakeMessage, ProtoError> {
    Ok(HandshakeMessage::ClientInit {
        features: client_init_features(fields),
        client_version: fields
            .get("ClientVersion")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        client_date: fields
            .get("ClientDate")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// The features a `ClientInit` message carries.
///
/// A client that speaks the extended negotiation sends the feature names in
/// `FeatureList`.  An older client sends only the legacy mask in `Features`;
/// that mask is then the whole feature set, and dropping it would lose what
/// the client negotiated.
fn client_init_features(fields: &Map) -> Features {
    match fields.get("FeatureList").and_then(Value::as_string_list) {
        Some(names) => Features::from_names(names),
        None => Features::from_legacy(fields.get("Features").and_then(Value::as_u32).unwrap_or(0)),
    }
}

/// Reads a `ClientInitAck` message.
fn decode_client_init_ack(fields: &Map) -> Result<HandshakeMessage, ProtoError> {
    let names = fields
        .get("FeatureList")
        .and_then(Value::as_string_list)
        .unwrap_or_default();
    let features = if names.is_empty() {
        Features::from_legacy(
            fields
                .get("CoreFeatures")
                .and_then(Value::as_u32)
                .unwrap_or(0),
        )
    } else {
        Features::from_names(names)
    };
    Ok(HandshakeMessage::ClientInitAck {
        features,
        configured: fields
            .get("Configured")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        storage_backends: fields
            .get("StorageBackends")
            .and_then(Value::as_list)
            .unwrap_or_default(),
        authenticators: fields
            .get("Authenticators")
            .and_then(Value::as_list)
            .unwrap_or_default(),
    })
}

/// Reads a `CoreSetupData` message.
fn decode_core_setup_data(fields: &Map) -> Result<HandshakeMessage, ProtoError> {
    let inner = fields
        .get("SetupData")
        .and_then(Value::as_map)
        .ok_or(ProtoError::MalformedHandshake)?;
    Ok(HandshakeMessage::CoreSetupData {
        admin_user: inner
            .get("AdminUser")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        admin_password: inner
            .get("AdminPasswd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        backend: inner
            .get("Backend")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        setup_data: inner
            .get("ConnectionProperties")
            .and_then(Value::as_map)
            .unwrap_or_default(),
        authenticator: inner
            .get("Authenticator")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        auth_setup_data: inner
            .get("AuthProperties")
            .and_then(Value::as_map)
            .unwrap_or_default(),
    })
}

/// Reads a `ClientLogin` message.
fn decode_client_login(fields: &Map) -> Result<HandshakeMessage, ProtoError> {
    Ok(HandshakeMessage::ClientLogin {
        user: fields
            .get("User")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        password: fields
            .get("Password")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Reads a `SessionInit` message.
fn decode_session_init(fields: &Map) -> Result<HandshakeMessage, ProtoError> {
    let state = fields
        .get("SessionState")
        .and_then(Value::as_map)
        .ok_or(ProtoError::MalformedHandshake)?;
    Ok(HandshakeMessage::SessionInit {
        session_state: SessionState {
            identities: state
                .get("Identities")
                .and_then(Value::as_list)
                .unwrap_or_default(),
            buffer_infos: state
                .get("BufferInfos")
                .and_then(Value::as_list)
                .unwrap_or_default(),
            network_ids: state
                .get("NetworkIds")
                .and_then(Value::as_list)
                .unwrap_or_default(),
        },
    })
}

/// The error text of a message, if it has one.
fn error_of(fields: &Map) -> String {
    fields
        .get("Error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Encodes a list of key/value pairs the way the core writes handshake
/// messages: the keys are byte arrays.
fn encode_fields(fields: &[(&str, Value)]) -> Vec<u8> {
    let mut items = Vec::with_capacity(fields.len() * 2);
    for (key, value) in fields {
        items.push(Value::bytes(key.as_bytes().to_vec()));
        items.push(value.clone());
    }
    encode_list_body_to_vec(&items)
}

/// Reads a list of key/value pairs the way the core writes handshake
/// messages.
fn decode_fields(items: &[Value]) -> Result<Map, ProtoError> {
    if items.len() % 2 != 0 {
        return Err(ProtoError::MalformedHandshake);
    }
    let mut map = Map::new();
    for pair in items.chunks_exact(2) {
        let key = match &pair[0] {
            Value::Bytes(Some(data)) => String::from_utf8_lossy(data).into_owned(),
            Value::Str(Some(text)) => text.clone(),
            _ => return Err(ProtoError::MalformedHandshake),
        };
        map.insert(key, pair[1].clone());
    }
    Ok(map)
}

trait ValueExt {
    fn as_str(&self) -> Option<&str>;
    fn as_bool(&self) -> Option<bool>;
    fn as_u32(&self) -> Option<u32>;
    fn as_list(&self) -> Option<Vec<Value>>;
    fn as_map(&self) -> Option<Map>;
    fn as_string_list(&self) -> Option<Vec<String>>;
}

impl ValueExt for Value {
    fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(Some(text)) => Some(text),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    fn as_u32(&self) -> Option<u32> {
        match self {
            Value::UInt(value) => Some(*value),
            Value::Int(value) => Some(*value as u32),
            _ => None,
        }
    }

    fn as_list(&self) -> Option<Vec<Value>> {
        match self {
            Value::List(items) => Some(items.clone()),
            _ => None,
        }
    }

    fn as_map(&self) -> Option<Map> {
        match self {
            Value::Map(map) => Some(map.clone()),
            _ => None,
        }
    }

    fn as_string_list(&self) -> Option<Vec<String>> {
        match self {
            Value::StringList(items) => Some(
                items
                    .iter()
                    .map(|item| item.clone().unwrap_or_default())
                    .collect(),
            ),
            _ => None,
        }
    }
}

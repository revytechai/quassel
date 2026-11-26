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
The user types of Quassel, and builders for the common payloads.

The names are part of the protocol: the C++ core looks a user type up by name
and rejects names it does not know with "Type name is not valid".  In
particular the server list type is `Network::Server`; `Network_Server` is not
accepted.

The names that carry a payload are the [`IdType`] and [`MapType`] enums, so
a value built here always names a user type the decoder reads back.  The two
types whose name is fixed by their variant are plain strings.
*/

use crate::value::{IdType, Map, MapType, UserType, Value};

/// The network id type.
pub const NETWORK_ID: IdType = IdType::NetworkId;
/// The server list type.
pub const NETWORK_SERVER: MapType = MapType::NetworkServer;
/// The identity type.
pub const IDENTITY: MapType = MapType::Identity;
/// The wire name of the buffer info type.
pub const BUFFER_INFO: &str = "BufferInfo";
/// The network info type.
pub const NETWORK_INFO: MapType = MapType::NetworkInfo;
/// The identity id type.
pub const IDENTITY_ID: IdType = IdType::IdentityId;
/// The buffer id type.
pub const BUFFER_ID: IdType = IdType::BufferId;
/// The wire name of the message id type.
pub const MSG_ID: &str = "MsgId";

/// Builds a network id value.
pub fn network_id(value: i32) -> Value {
    Value::UserType(UserType::Id {
        name: NETWORK_ID,
        value,
    })
}

/// Builds an identity id value.
pub fn identity_id(value: i32) -> Value {
    Value::UserType(UserType::Id {
        name: IDENTITY_ID,
        value,
    })
}

/// Builds a buffer id value.
pub fn buffer_id(value: i32) -> Value {
    Value::UserType(UserType::Id {
        name: BUFFER_ID,
        value,
    })
}

/// Builds one entry of a network's server list.
pub fn network_server(
    host: impl Into<String>,
    port: u32,
    use_ssl: bool,
    ssl_verify: bool,
) -> Value {
    let mut fields = Map::new();
    fields.insert("Host", Value::string(host.into()));
    fields.insert("Port", Value::UInt(port));
    fields.insert("Password", Value::string(""));
    fields.insert("UseSSL", Value::Bool(use_ssl));
    fields.insert("sslVerify", Value::Bool(ssl_verify));
    fields.insert("sslVersion", Value::Int(0));
    fields.insert("UseProxy", Value::Bool(false));
    fields.insert("ProxyType", Value::Int(0));
    fields.insert("ProxyHost", Value::string("localhost"));
    fields.insert("ProxyPort", Value::UInt(8080));
    fields.insert("ProxyUser", Value::string(""));
    fields.insert("ProxyPass", Value::string(""));
    Value::UserType(UserType::Map {
        name: NETWORK_SERVER,
        fields,
    })
}

/// Builds a network info payload.
pub fn network_info(
    name: impl Into<String>,
    servers: Vec<Value>,
    identity: i32,
    encoding_codec: Option<Vec<u8>>,
    decoding_codec: Option<Vec<u8>>,
    server_codec: Option<Vec<u8>>,
) -> Value {
    let mut fields = Map::new();
    fields.insert("NetworkId", network_id(0));
    fields.insert("NetworkName", Value::string(name.into()));
    fields.insert("Identity", identity_id(identity));
    fields.insert("CodecForServer", Value::Bytes(server_codec));
    fields.insert("CodecForEncoding", Value::Bytes(encoding_codec));
    fields.insert("CodecForDecoding", Value::Bytes(decoding_codec));
    fields.insert("ServerList", Value::List(servers));
    fields.insert("Perform", Value::StringList(Vec::new()));
    fields.insert("SkipCaps", Value::StringList(Vec::new()));
    fields.insert("AutoIdentifyService", Value::string("NickServ"));
    fields.insert("AutoIdentifyPassword", Value::string(""));
    fields.insert("SaslAccount", Value::string(""));
    fields.insert("SaslPassword", Value::string(""));
    fields.insert("UseRandomServer", Value::Bool(false));
    fields.insert("UseAutoIdentify", Value::Bool(false));
    fields.insert("UseSasl", Value::Bool(false));
    fields.insert("UseAutoReconnect", Value::Bool(true));
    fields.insert("AutoReconnectInterval", Value::UInt(60));
    fields.insert("AutoReconnectRetries", Value::UInt(20));
    fields.insert("UnlimitedReconnectRetries", Value::Bool(false));
    fields.insert("RejoinChannels", Value::Bool(true));
    fields.insert("UseCustomMessageRate", Value::Bool(false));
    fields.insert("MessageRateBurstSize", Value::UInt(5));
    fields.insert("MessageRateDelay", Value::UInt(2200));
    fields.insert("UnlimitedMessageRate", Value::Bool(false));
    Value::UserType(UserType::Map {
        name: NETWORK_INFO,
        fields,
    })
}

/// Builds an identity payload.
pub fn identity(identity_id: i32, identity_name: impl Into<String>) -> Value {
    let mut fields = Map::new();
    fields.insert("IdentityId", identity_id_value(identity_id));
    fields.insert("IdentityName", Value::string(identity_name.into()));
    fields.insert("RealName", Value::string(""));
    fields.insert("AwayNick", Value::string(""));
    fields.insert("AwayNickEnabled", Value::Bool(false));
    fields.insert("AutoAwayEnabled", Value::Bool(false));
    fields.insert("AutoAwayTime", Value::Int(0));
    fields.insert("AutoAwayReason", Value::string(""));
    fields.insert("AutoAwayMessage", Value::string(""));
    fields.insert("Idents", Value::Map(Map::new()));
    fields.insert("KickReason", Value::string(""));
    fields.insert("PartReason", Value::string(""));
    fields.insert("QuitReason", Value::string(""));
    fields.insert("AwayReason", Value::string(""));
    fields.insert("AwayMessage", Value::string(""));
    fields.insert("CollapseState", Value::Bool(false));
    fields.insert("SslCert", Value::Bytes(None));
    fields.insert("SslKey", Value::Bytes(None));
    fields.insert("UserModes", Value::string(""));
    fields.insert("UserMode", Value::string(""));
    fields.insert("SASLAccount", Value::string(""));
    fields.insert("SASLPassword", Value::string(""));
    Value::UserType(UserType::Map {
        name: IDENTITY,
        fields,
    })
}

/// Builds an identity id value (private helper of [`identity`]).
fn identity_id_value(value: i32) -> Value {
    Value::UserType(UserType::Id {
        name: IDENTITY_ID,
        value,
    })
}

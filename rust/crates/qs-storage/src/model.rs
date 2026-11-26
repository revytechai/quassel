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
The things the storage layer keeps.
*/

/// A user of the core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    /// The user's id.
    pub userid: i64,
    /// The user's name.
    pub username: String,
    /// The authenticator the user is checked against.
    pub authenticator: String,
}

/// A network, as the database stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    /// The network's id.
    pub networkid: i64,
    /// The id of the user the network belongs to.
    pub userid: i64,
    /// The network's name.
    pub networkname: String,
    /// The id of the identity the network uses.
    pub identityid: i64,
    /// The codec the network encodes with.
    pub encodingcodec: String,
    /// The codec the network decodes with.
    pub decodingcodec: String,
    /// The codec the network uses on the server connection.
    pub servercodec: String,
    /// Whether the network is marked as connected.
    pub connected: bool,
    /// Whether the core reconnects on its own.
    pub useautoreconnect: bool,
    /// The seconds between two reconnect attempts.
    pub autoreconnectinterval: i64,
    /// How often the core retries before giving up.
    pub autoreconnectretries: i64,
    /// Whether the core rejoins the channels.
    pub rejoinchannels: bool,
    /// The IRCv3 capabilities the core must not negotiate.
    pub skipcaps: Option<String>,
}

/// A network, as a client asks for it.
///
/// The codecs are byte arrays, exactly as the client sends them.  An empty
/// codec is stored as an empty string and **never** as SQL NULL: the columns
/// are `NOT NULL`, and the C++ core binds a null `QByteArray` as SQL NULL,
/// which is what makes "NOT NULL constraint failed: network.encodingcodec".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    /// The network's name.
    pub networkname: String,
    /// The id of the identity the network uses.
    pub identityid: i64,
    /// The codec the network encodes with; empty means the default.
    pub encodingcodec: Option<Vec<u8>>,
    /// The codec the network decodes with; empty means the default.
    pub decodingcodec: Option<Vec<u8>>,
    /// The codec the network uses on the server connection.
    pub servercodec: Option<Vec<u8>>,
    /// Whether the core reconnects on its own.
    pub useautoreconnect: bool,
    /// The seconds between two reconnect attempts.
    pub autoreconnectinterval: i64,
    /// How often the core retries before giving up.
    pub autoreconnectretries: i64,
    /// Whether the core rejoins the channels.
    pub rejoinchannels: bool,
    /// The IRCv3 capabilities the core must not negotiate.
    pub skipcaps: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            networkname: String::new(),
            identityid: 1,
            encodingcodec: None,
            decodingcodec: None,
            servercodec: None,
            useautoreconnect: true,
            autoreconnectinterval: 60,
            autoreconnectretries: 20,
            rejoinchannels: true,
            skipcaps: None,
        }
    }
}

/// Turns a codec byte array into the text the database holds.
///
/// A codec that is not there becomes an empty string, never SQL NULL.
pub fn codec_to_text(codec: Option<&[u8]>) -> String {
    match codec {
        Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        None => String::new(),
    }
}

/// An identity, as the database stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The identity's id.
    pub identityid: i64,
    /// The user's id.
    pub userid: i64,
    /// The identity's name.
    pub identityname: String,
    /// The identity's real name.
    pub realname: String,
    /// The nicknames the identity uses.
    pub nicks: Vec<String>,
    /// The nickname the identity goes away under.
    pub awaynick: Option<String>,
    /// Whether the identity goes away under another nickname.
    pub awaynickenabled: bool,
    /// The reason the identity is away.
    pub awayreason: Option<String>,
    /// Whether the away reason is shown.
    pub awayreasonenabled: bool,
    /// Whether the identity goes away automatically.
    pub autoawayenabled: bool,
    /// The minutes of inactivity after which the identity goes away.
    pub autoawaytime: i64,
    /// The reason the identity goes away automatically.
    pub autoawayreason: Option<String>,
    /// Whether the automatic away reason is shown.
    pub autoawayreasonenabled: bool,
    /// Whether the identity goes away when it is detached.
    pub detachawayenabled: bool,
    /// The reason the identity is away when it is detached.
    pub detachawayreason: Option<String>,
    /// Whether the detached away reason is shown.
    pub detachawayreasonenabled: bool,
    /// The ident the identity answers with.
    pub ident: Option<String>,
    /// The reason for kicking.
    pub kickreason: Option<String>,
    /// The reason for parting.
    pub partreason: Option<String>,
    /// The reason for quitting.
    pub quitreason: Option<String>,
}

/// A buffer, as the database stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferInfo {
    /// The buffer's id.
    pub bufferid: i64,
    /// The user's id.
    pub userid: i64,
    /// The id of the network the buffer belongs to.
    pub networkid: i64,
    /// The buffer's name.
    pub buffername: String,
    /// The buffer's type.
    pub buffertype: i64,
    /// The id of the group the buffer belongs to.
    pub groupid: Option<i64>,
}

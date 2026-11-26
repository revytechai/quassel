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
The session of one client.

A session starts with the handshake messages — `ClientInit`, `ClientLogin`
— and continues with the signal proxy messages.  When the login succeeds,
the core sends the state of the session: the user's identities, buffers
and networks.
*/

use std::net::SocketAddr;
use std::sync::Arc;

use qs_datastream::{decode_list_body, Map, UserType, Value};
use qs_proto::{Features, FrameDecoder, HandshakeMessage, SessionState, SignalMessage};
use qs_storage::Storage;

use crate::logging;
use crate::login::LoginKey;
use crate::server::{read_message, write_message, Stream};

/// What the session is waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The client has to say hello.
    AwaitingInit,
    /// The client has to log in.
    AwaitingLogin,
    /// The client is in.
    LoggedIn,
}

/// One client.
pub struct Session {
    core: Arc<crate::server::Core>,
    peer: SocketAddr,
    phase: Phase,
    userid: Option<i64>,
}

impl Session {
    /// Starts a session.
    pub fn new(core: Arc<crate::server::Core>, peer: SocketAddr) -> Self {
        Self {
            core,
            peer,
            phase: Phase::AwaitingInit,
            userid: None,
        }
    }

    /// Runs the session until the client goes away.
    pub async fn run(&mut self, stream: &mut Box<dyn Stream>) -> anyhow::Result<()> {
        let mut decoder = FrameDecoder::new();
        loop {
            let payload = read_message(stream, &mut decoder).await?;
            let (items, _) = decode_list_body(&payload)?;
            if items.is_empty() {
                continue;
            }
            match items[0] {
                // A handshake message starts with a key, a signal proxy
                // message with the request type.
                Value::Bytes(_) | Value::Str(_) => {
                    let message = HandshakeMessage::decode(&payload)?;
                    if !self.handle_handshake(stream, message).await? {
                        return Ok(());
                    }
                }
                _ => {
                    if self.phase != Phase::LoggedIn {
                        anyhow::bail!("a signal proxy message before the login");
                    }
                    let message = SignalMessage::decode(&payload)?;
                    self.handle_signal(message);
                }
            }
        }
    }

    /// Handles a handshake message.
    ///
    /// Returns `false` when the session is over.
    async fn handle_handshake(
        &mut self,
        stream: &mut Box<dyn Stream>,
        message: HandshakeMessage,
    ) -> anyhow::Result<bool> {
        match message {
            HandshakeMessage::ClientInit { .. } => {
                if self.phase != Phase::AwaitingInit {
                    anyhow::bail!("a second ClientInit");
                }
                let reply = HandshakeMessage::ClientInitAck {
                    features: Features::all(),
                    configured: true,
                    storage_backends: backend_info(self.core.storage.backend_id()),
                    authenticators: authenticator_info(),
                };
                write_message(stream, &reply.encode()).await?;
                self.phase = Phase::AwaitingLogin;
                Ok(true)
            }
            HandshakeMessage::ClientLogin { user, password } => {
                if self.phase != Phase::AwaitingLogin {
                    anyhow::bail!("a login before the client introduced itself");
                }
                // A core that answers every wrong password at once can be
                // used to try a whole dictionary against one account.
                let key = LoginKey::new(&user, self.peer.ip());
                let wait = self.core.limiter.wait_time(&key);
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
                let stored = self.core.storage.validate_user(&user, &password).await?;
                let Some(stored) = stored else {
                    self.core.limiter.record_failure(&key);
                    let reply = HandshakeMessage::ClientLoginReject {
                        error: "<b>Invalid username or password!</b><br>The username/password \
                               combination you supplied could not be found in the database."
                            .to_owned(),
                    };
                    write_message(stream, &reply.encode()).await?;
                    return Ok(false);
                };
                self.core.limiter.record_success(&key);
                let reply = HandshakeMessage::ClientLoginAck;
                write_message(stream, &reply.encode()).await?;
                logging::info(&format!(
                    "{}: logged in as \"{}\" (user id {})",
                    self.peer, stored.username, stored.userid
                ));
                self.userid = Some(stored.userid);
                self.phase = Phase::LoggedIn;

                let state = self.session_state(stored.userid).await?;
                let reply = HandshakeMessage::SessionInit {
                    session_state: state,
                };
                write_message(stream, &reply.encode()).await?;
                Ok(true)
            }
            other => {
                // The message is described, not printed: it can hold the
                // password of the administrator.
                logging::warning(&format!(
                    "{}: unexpected handshake message {}",
                    self.peer,
                    describe_handshake(&other)
                ));
                Ok(true)
            }
        }
    }

    /// Builds the state of the session: the identities, the buffers and the
    /// networks of the user.
    async fn session_state(&self, userid: i64) -> anyhow::Result<SessionState> {
        let identities = self
            .core
            .storage
            .identities(userid)
            .await?
            .into_iter()
            .map(identity_value)
            .collect();
        let buffers = self
            .core
            .storage
            .buffers(userid)
            .await?
            .into_iter()
            .map(buffer_value)
            .collect();
        let networks = self
            .core
            .storage
            .networks(userid)
            .await?
            .into_iter()
            .map(|network| {
                Value::UserType(UserType::Id {
                    name: qs_datastream::NETWORK_ID.to_owned(),
                    value: network.networkid as i32,
                })
            })
            .collect();
        Ok(SessionState {
            identities,
            buffer_infos: buffers,
            network_ids: networks,
        })
    }

    /// Handles a signal proxy message.
    ///
    /// Round 1 does not implement the live state yet; the message is counted
    /// and logged.
    fn handle_signal(&mut self, message: SignalMessage) {
        // The message is described, not printed: its parameters can hold the
        // passwords of the servers and of the user.
        logging::debug(&format!("{}: {}", self.peer, describe_signal(&message)));
    }
}

/// The name of a handshake message, without its values.
pub fn describe_handshake(message: &HandshakeMessage) -> &'static str {
    match message {
        HandshakeMessage::ClientInit { .. } => "ClientInit",
        HandshakeMessage::ClientInitReject { .. } => "ClientInitReject",
        HandshakeMessage::ClientInitAck { .. } => "ClientInitAck",
        HandshakeMessage::CoreSetupData { .. } => "CoreSetupData",
        HandshakeMessage::CoreSetupReject { .. } => "CoreSetupReject",
        HandshakeMessage::CoreSetupAck => "CoreSetupAck",
        HandshakeMessage::ClientLogin { .. } => "ClientLogin",
        HandshakeMessage::ClientLoginReject { .. } => "ClientLoginReject",
        HandshakeMessage::ClientLoginAck => "ClientLoginAck",
        HandshakeMessage::SessionInit { .. } => "SessionInit",
    }
}

/// A description of a signal proxy message that holds no secret.
///
/// The parameters of a signal message can hold the passwords of the servers
/// and of the user, so they are not written to the log.
pub fn describe_signal(message: &SignalMessage) -> String {
    match message {
        SignalMessage::Sync {
            class_name,
            object_name,
            slot_name,
            params,
        } => format!(
            "sync {class_name} {object_name} {slot_name} with {} parameters",
            params.len()
        ),
        SignalMessage::RpcCall {
            signal_name,
            params,
        } => {
            format!("rpc {signal_name} with {} parameters", params.len())
        }
        SignalMessage::InitRequest {
            class_name,
            object_name,
        } => format!("init request {class_name} {object_name}"),
        SignalMessage::InitData {
            class_name,
            object_name,
            ..
        } => format!("init data {class_name} {object_name}"),
        SignalMessage::HeartBeat { .. } => "heartbeat".to_owned(),
        SignalMessage::HeartBeatReply { .. } => "heartbeat reply".to_owned(),
    }
}

/// The storage backends the core offers, the way the C++ core describes
/// them.
///
/// The backend the core runs on is the default one: the client has to be
/// able to see which backend it is talking to.
fn backend_info(running: &str) -> Vec<Value> {
    let mut backends = Vec::new();
    for (id, name, description) in [
        (
            "SQLite",
            "SQLite",
            "SQLite is a file-based database engine that does not require any setup.",
        ),
        (
            "PostgreSQL",
            "PostgreSQL",
            "PostgreSQL is a database server that several cores can share.",
        ),
    ] {
        let mut entry = Map::new();
        entry.insert("BackendId", Value::string(id));
        entry.insert("DisplayName", Value::string(name));
        entry.insert("Description", Value::string(description));
        entry.insert("SetupData", Value::Map(Map::new()));
        entry.insert("SetupKeys", Value::StringList(Vec::new()));
        entry.insert("SetupDefaults", Value::Map(Map::new()));
        entry.insert("IsDefault", Value::Bool(id == running));
        backends.push(Value::Map(entry));
    }
    backends
}

/// The authenticators the core offers.
fn authenticator_info() -> Vec<Value> {
    let mut entry = Map::new();
    entry.insert("BackendId", Value::string("Database"));
    entry.insert("DisplayName", Value::string("Database"));
    entry.insert(
        "Description",
        Value::string("The users are held in the core's own database."),
    );
    entry.insert("SetupData", Value::Map(Map::new()));
    vec![Value::Map(entry)]
}

/// The wire form of an identity.
fn identity_value(identity: qs_storage::Identity) -> Value {
    let mut fields = Map::new();
    fields.insert(
        "IdentityId",
        Value::UserType(UserType::Id {
            name: qs_datastream::IDENTITY_ID.to_owned(),
            value: identity.identityid as i32,
        }),
    );
    fields.insert("IdentityName", Value::string(identity.identityname));
    fields.insert("RealName", Value::string(identity.realname));
    fields.insert(
        "Nicks",
        Value::StringList(identity.nicks.iter().cloned().map(Some).collect()),
    );
    fields.insert(
        "AwayNick",
        match identity.awaynick {
            Some(nick) => Value::string(nick),
            None => Value::Str(None),
        },
    );
    fields.insert("AwayNickEnabled", Value::Bool(identity.awaynickenabled));
    fields.insert(
        "AwayReason",
        match identity.awayreason {
            Some(reason) => Value::string(reason),
            None => Value::Str(None),
        },
    );
    fields.insert("AwayReasonEnabled", Value::Bool(identity.awayreasonenabled));
    fields.insert("AutoAwayEnabled", Value::Bool(identity.autoawayenabled));
    fields.insert("AutoAwayTime", Value::Int(identity.autoawaytime as i32));
    fields.insert(
        "AutoAwayReason",
        match identity.autoawayreason {
            Some(reason) => Value::string(reason),
            None => Value::Str(None),
        },
    );
    fields.insert(
        "AutoAwayReasonEnabled",
        Value::Bool(identity.autoawayreasonenabled),
    );
    fields.insert("DetachAwayEnabled", Value::Bool(identity.detachawayenabled));
    fields.insert(
        "DetachAwayReason",
        match identity.detachawayreason {
            Some(reason) => Value::string(reason),
            None => Value::Str(None),
        },
    );
    fields.insert(
        "DetachAwayReasonEnabled",
        Value::Bool(identity.detachawayreasonenabled),
    );
    fields.insert(
        "Ident",
        match identity.ident {
            Some(ident) => Value::string(ident),
            None => Value::Str(None),
        },
    );
    fields.insert(
        "KickReason",
        match identity.kickreason {
            Some(reason) => Value::string(reason),
            None => Value::Str(None),
        },
    );
    fields.insert(
        "PartReason",
        match identity.partreason {
            Some(reason) => Value::string(reason),
            None => Value::Str(None),
        },
    );
    fields.insert(
        "QuitReason",
        match identity.quitreason {
            Some(reason) => Value::string(reason),
            None => Value::Str(None),
        },
    );
    Value::UserType(UserType::Map {
        name: qs_datastream::IDENTITY.to_owned(),
        fields,
    })
}

/// The wire form of a buffer.
fn buffer_value(buffer: qs_storage::BufferInfo) -> Value {
    Value::UserType(UserType::BufferInfo {
        buffer_id: buffer.bufferid as i32,
        network_id: buffer.networkid as i32,
        buffer_type: buffer.buffertype as i16,
        group_id: buffer.groupid.unwrap_or(0) as i32,
        name: buffer.buffername.into_bytes(),
    })
}

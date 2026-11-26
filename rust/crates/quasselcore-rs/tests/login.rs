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

//! The handshake and the login, end to end.
//!
//! The test starts the core, talks the protocol to it the way a client
//! does, and checks what comes back.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use qs_datastream::Value;
use qs_proto::{
    ConnectionFeatures, FrameDecoder, Handshake, HandshakeMessage, HandshakeReply,
    ProtocolDescriptor, ProtocolType, SessionState,
};
use qs_storage::Storage;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::DigitallySignedStruct;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

mod common;

use common::{test_core, test_core_on};

/// A verifier that accepts every certificate.
///
/// The test server uses a certificate the test made itself, so there is
/// nobody to verify it against.
#[derive(Debug)]
struct NoVerification;

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A client that speaks the protocol.
struct Client {
    stream: tokio_rustls::client::TlsStream<TcpStream>,
    decoder: FrameDecoder,
}

impl Client {
    /// Connects to the core and does the handshake.
    async fn connect(address: SocketAddr) -> anyhow::Result<Self> {
        let mut stream = TcpStream::connect(address).await?;

        // The handshake is plaintext: the core reads it before the
        // connection becomes a TLS one.
        let handshake = Handshake {
            features: ConnectionFeatures::ENCRYPTION,
            protocols: vec![
                ProtocolDescriptor::new(ProtocolType::DataStream, 0),
                ProtocolDescriptor {
                    protocol: ProtocolType::Legacy,
                    features: 0,
                    last: true,
                },
            ],
        };
        stream.write_all(&handshake.encode()).await?;
        stream.flush().await?;

        let mut reply = [0u8; 4];
        stream.read_exact(&mut reply).await?;
        let reply = HandshakeReply::decode(reply)
            .expect("the core answers with a protocol the client knows");
        assert_eq!(reply.protocol, ProtocolType::DataStream);
        assert!(
            reply.connection_features.encryption(),
            "the core must offer encryption"
        );

        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerification))
        .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let name = ServerName::try_from(address.ip().to_string())?;
        let stream = connector.connect(name, stream).await?;

        Ok(Self {
            stream,
            decoder: FrameDecoder::new(),
        })
    }

    /// Sends a message.
    async fn send(&mut self, message: &HandshakeMessage) -> anyhow::Result<()> {
        self.stream
            .write_all(&qs_proto::frame(&message.encode()))
            .await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Reads the next message.
    async fn receive(&mut self) -> anyhow::Result<HandshakeMessage> {
        let payload = loop {
            if let Some(message) = self.decoder.next_frame()? {
                break message;
            }
            let mut chunk = [0u8; 8192];
            let read = self.stream.read(&mut chunk).await?;
            assert!(read > 0, "the core closed the connection");
            self.decoder.push(&chunk[..read]);
        };
        Ok(HandshakeMessage::decode(&payload)?)
    }
}

/// A test directory that goes away when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("quasselcore-rs-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("creating the test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn a_client_logs_in_and_gets_the_session_state() {
    let dir = TempDir::new("login");
    let core = test_core(dir.path(), "alice", "secret").await;
    let address = core.address;

    let mut client = Client::connect(address).await.expect("the handshake");

    client
        .send(&HandshakeMessage::ClientInit {
            features: qs_proto::Features::all(),
            client_version: "0.14.50".to_owned(),
            client_date: "2026-11-26".to_owned(),
        })
        .await
        .unwrap();
    let reply = client.receive().await.unwrap();
    let HandshakeMessage::ClientInitAck {
        features,
        configured,
        ..
    } = &reply
    else {
        panic!("expected a ClientInitAck, got {reply:?}");
    };
    assert!(configured, "the core is configured");
    assert!(features.has("ExtendedFeatures"));

    client
        .send(&HandshakeMessage::ClientLogin {
            user: "alice".to_owned(),
            password: "secret".to_owned(),
        })
        .await
        .unwrap();
    let reply = client.receive().await.unwrap();
    assert!(
        matches!(reply, HandshakeMessage::ClientLoginAck),
        "{reply:?}"
    );

    let reply = client.receive().await.unwrap();
    let HandshakeMessage::SessionInit { session_state } = reply else {
        panic!("expected a SessionInit, got {reply:?}");
    };
    assert_eq!(
        session_state,
        SessionState {
            identities: Vec::new(),
            buffer_infos: Vec::new(),
            network_ids: Vec::new(),
        }
    );
}

#[tokio::test]
async fn a_wrong_password_is_refused() {
    let dir = TempDir::new("wrong");
    let core = test_core(dir.path(), "alice", "secret").await;
    let address = core.address;

    let mut client = Client::connect(address).await.expect("the handshake");
    client
        .send(&HandshakeMessage::ClientInit {
            features: qs_proto::Features::all(),
            client_version: "0.14.50".to_owned(),
            client_date: "2026-11-26".to_owned(),
        })
        .await
        .unwrap();
    assert!(matches!(
        client.receive().await.unwrap(),
        HandshakeMessage::ClientInitAck { .. }
    ));

    client
        .send(&HandshakeMessage::ClientLogin {
            user: "alice".to_owned(),
            password: "wrong".to_owned(),
        })
        .await
        .unwrap();
    let reply = client.receive().await.unwrap();
    let HandshakeMessage::ClientLoginReject { error } = reply else {
        panic!("expected a ClientLoginReject, got {reply:?}");
    };
    assert!(error.contains("Invalid username or password"), "{error}");
}

#[tokio::test]
async fn a_client_that_wants_no_encryption_gets_none() {
    let dir = TempDir::new("plain");
    let core = test_core(dir.path(), "alice", "secret").await;
    let address = core.address;

    let mut stream = TcpStream::connect(address).await.unwrap();
    let handshake = Handshake {
        features: 0,
        protocols: vec![ProtocolDescriptor {
            protocol: ProtocolType::DataStream,
            features: 0,
            last: true,
        }],
    };
    stream.write_all(&handshake.encode()).await.unwrap();
    stream.flush().await.unwrap();

    let mut reply = [0u8; 4];
    stream.read_exact(&mut reply).await.unwrap();
    let reply =
        HandshakeReply::decode(reply).expect("the core answers with a protocol the client knows");
    assert_eq!(reply.protocol, ProtocolType::DataStream);
    assert!(!reply.connection_features.encryption());
}

#[tokio::test]
async fn the_session_state_holds_the_networks_of_the_user() {
    let dir = TempDir::new("networks");
    let core = test_core(dir.path(), "alice", "secret").await;
    let address = core.address;

    // Give the user a network with empty codecs, the way the C++ core's
    // client does.
    let user = core
        .storage
        .validate_user("alice", "secret")
        .await
        .unwrap()
        .unwrap();
    core.storage
        .create_network(
            user.userid,
            &qs_storage::NetworkConfig {
                networkname: "EFnet".to_owned(),
                encodingcodec: Some(Vec::new()),
                decodingcodec: Some(Vec::new()),
                servercodec: Some(Vec::new()),
                ..qs_storage::NetworkConfig::default()
            },
        )
        .await
        .unwrap();

    let mut client = Client::connect(address).await.expect("the handshake");
    client
        .send(&HandshakeMessage::ClientInit {
            features: qs_proto::Features::all(),
            client_version: "0.14.50".to_owned(),
            client_date: "2026-11-26".to_owned(),
        })
        .await
        .unwrap();
    client.receive().await.unwrap();
    client
        .send(&HandshakeMessage::ClientLogin {
            user: "alice".to_owned(),
            password: "secret".to_owned(),
        })
        .await
        .unwrap();
    client.receive().await.unwrap();
    let reply = client.receive().await.unwrap();
    let HandshakeMessage::SessionInit { session_state } = reply else {
        panic!("expected a SessionInit, got {reply:?}");
    };
    assert_eq!(session_state.network_ids.len(), 1);
    assert!(matches!(session_state.network_ids[0], Value::UserType(_)));
}

/// Logs in and returns the answer of the core.
async fn login(address: SocketAddr, user: &str, password: &str) -> HandshakeMessage {
    let mut client = Client::connect(address).await.expect("the handshake");
    client
        .send(&HandshakeMessage::ClientInit {
            features: qs_proto::Features::all(),
            client_version: "0.14.50".to_owned(),
            client_date: "2026-11-26".to_owned(),
        })
        .await
        .unwrap();
    client.receive().await.unwrap();
    client
        .send(&HandshakeMessage::ClientLogin {
            user: user.to_owned(),
            password: password.to_owned(),
        })
        .await
        .unwrap();
    client.receive().await.unwrap()
}

#[tokio::test]
async fn every_address_the_core_listens_on_is_served() {
    let dir = TempDir::new("listeners");
    let core = test_core_on(
        dir.path(),
        "alice",
        "secret",
        &[
            SocketAddr::from(([127, 0, 0, 1], 0)),
            SocketAddr::from(([127, 0, 0, 1], 0)),
        ],
    )
    .await;
    assert_eq!(core.addresses.len(), 2);
    // Every address the core listens on answers a handshake.
    for address in &core.addresses {
        // The handshake is answered on every address.
        let mut client = Client::connect(*address).await.expect("the handshake");
        client
            .send(&HandshakeMessage::ClientInit {
                features: qs_proto::Features::all(),
                client_version: "0.14.50".to_owned(),
                client_date: "2026-11-26".to_owned(),
            })
            .await
            .unwrap();
        assert!(matches!(
            client.receive().await.unwrap(),
            HandshakeMessage::ClientInitAck { .. }
        ));
    }
}

#[tokio::test]
async fn a_client_that_offers_an_unknown_protocol_is_refused() {
    let dir = TempDir::new("protocol");
    let core = test_core(dir.path(), "alice", "secret").await;

    let mut stream = TcpStream::connect(core.address).await.unwrap();
    // The magic, then a protocol the core does not know.
    let mut bytes = qs_proto::MAGIC.to_be_bytes().to_vec();
    bytes.extend_from_slice(&0x8000_00ffu32.to_be_bytes());
    stream.write_all(&bytes).await.unwrap();
    stream.flush().await.unwrap();

    // The core does not answer a protocol it does not speak: it closes the
    // connection.
    let mut reply = [0u8; 4];
    let read = stream.read(&mut reply).await;
    assert!(
        matches!(read, Ok(0)) || read.is_err(),
        "the core must not answer a protocol it does not know: {read:?}"
    );
}

#[tokio::test]
async fn too_many_wrong_passwords_are_slowed_down() {
    let dir = TempDir::new("throttle");
    let core = test_core(dir.path(), "alice", "secret").await;
    let address = core.address;

    // The first wrong passwords are answered at once.
    for _ in 0..3 {
        let reply = login(address, "alice", "wrong").await;
        assert!(
            matches!(reply, HandshakeMessage::ClientLoginReject { .. }),
            "{reply:?}"
        );
    }

    // The next one has to wait: a core that answers every guess at once can
    // be used to try a whole dictionary.
    let start = std::time::Instant::now();
    let reply = login(address, "alice", "wrong").await;
    assert!(
        matches!(reply, HandshakeMessage::ClientLoginReject { .. }),
        "{reply:?}"
    );
    assert!(
        start.elapsed() >= std::time::Duration::from_secs(1),
        "the fourth attempt was not slowed down: {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn the_connected_networks_carry_the_network_id() {
    let dir = TempDir::new("connected-networks");
    let core = test_core(dir.path(), "alice", "secret").await;
    let user = core
        .storage
        .validate_user("alice", "secret")
        .await
        .unwrap()
        .unwrap();
    let networkid = core
        .storage
        .create_network(
            user.userid,
            &qs_storage::NetworkConfig {
                networkname: "EFnet".to_owned(),
                ..qs_storage::NetworkConfig::default()
            },
        )
        .await
        .unwrap();
    core.storage
        .set_network_connected(networkid, true)
        .await
        .unwrap();

    let connected = core.core.connected_networks().await.unwrap();
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].network_id, networkid);
    assert_eq!(connected[0].user_id, user.userid);
    assert_eq!(connected[0].networkname, "EFnet");
}

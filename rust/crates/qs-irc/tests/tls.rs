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

//! Tests for the IRC layer.
//!
//! The interesting one is the certificate check: a self-signed server must be
//! refused when the verification is on, and accepted when its fingerprint is
//! pinned down.

use std::sync::Arc;
use std::time::Duration;

use qs_irc::{Fingerprint, IrcConnection, IrcServer, ServerVerification};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// A self-signed TLS server the tests talk to.
struct TestServer {
    host: String,
    port: u16,
    fingerprint: Fingerprint,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl TestServer {
    /// Starts a TLS server with a self-signed certificate.
    async fn start() -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("generating a certificate");
        let certificate = certified.cert.der().clone();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            certified.signing_key.serialize_der(),
        ));
        let config = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .expect("the protocol versions")
            .with_no_client_auth()
            .with_single_cert(vec![certificate.clone()], key)
            .expect("the certificate and the key");
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binding");
        let address = listener.local_addr().expect("the address");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(stream).await {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        // The greeting comes first, so the client has
                        // something to read without having to write.
                        let _ = tls.write_all(b"NOTICE :hello\r\n").await;
                        let mut buffer = [0u8; 256];
                        let _ = tls.read(&mut buffer).await;
                    }
                });
            }
        });
        Self {
            host: address.ip().to_string(),
            port: address.port(),
            fingerprint: Fingerprint::of_certificate(certificate.as_ref()),
            provider,
        }
    }

    /// The server description with the given verification.
    fn server(&self, verification: ServerVerification) -> IrcServer {
        IrcServer {
            host: self.host.clone(),
            port: self.port,
            use_ssl: true,
            verification,
            password: None,
        }
    }
}

#[tokio::test]
async fn a_pinned_self_signed_server_is_accepted() {
    let test_server = TestServer::start().await;
    let server = test_server.server(ServerVerification::PinnedSha256(test_server.fingerprint));
    let mut connection = IrcConnection::connect(&server, test_server.provider)
        .await
        .expect("the pinned certificate must be accepted");
    assert_eq!(connection.fingerprint(), Some(test_server.fingerprint));
    let line = connection.read_line().await.unwrap();
    assert_eq!(line.as_deref(), Some("NOTICE :hello"));
}

#[tokio::test]
async fn a_self_signed_server_is_refused_when_the_verification_is_on() {
    let test_server = TestServer::start().await;
    let server = test_server.server(ServerVerification::SystemRoots);
    let result = IrcConnection::connect(&server, test_server.provider).await;
    assert!(
        result.is_err(),
        "a self-signed certificate must not pass the system roots"
    );
}

#[tokio::test]
async fn a_wrong_fingerprint_is_refused() {
    let test_server = TestServer::start().await;
    let wrong =
        Fingerprint::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap();
    let server = test_server.server(ServerVerification::PinnedSha256(wrong));
    let result = IrcConnection::connect(&server, test_server.provider).await;
    assert!(result.is_err(), "the wrong fingerprint must be refused");
}

#[tokio::test]
async fn the_verification_can_be_turned_off() {
    let test_server = TestServer::start().await;
    let server = test_server.server(ServerVerification::Disabled);
    let mut connection = IrcConnection::connect(&server, test_server.provider)
        .await
        .expect("without verification the certificate is accepted");
    connection.write_line("NICK tester").await.unwrap();
}

#[test]
fn a_fingerprint_is_read_and_written_in_hex() {
    let fingerprint = Fingerprint::of_certificate(b"a certificate");
    let text = fingerprint.to_hex();
    assert_eq!(text.len(), 64);
    assert_eq!(Fingerprint::from_hex(&text), Some(fingerprint));
    assert!(Fingerprint::from_hex("too short").is_none());
    assert!(Fingerprint::from_hex("zzzz").is_none());
}

#[test]
fn a_non_ascii_fingerprint_is_refused() {
    let text = "€".repeat(21) + "x";
    assert_eq!(Fingerprint::from_hex(&text), None);
}

#[test]
fn the_reconnect_policy_counts_the_attempts() {
    let policy = qs_irc::ReconnectPolicy {
        enabled: true,
        interval_seconds: 60,
        retries: 3,
        unlimited_retries: false,
    };
    let waited = Duration::from_secs(60);
    assert!(policy.should_retry(0, waited));
    assert!(policy.should_retry(2, waited));
    assert!(!policy.should_retry(3, waited));

    let unlimited = qs_irc::ReconnectPolicy {
        unlimited_retries: true,
        ..policy
    };
    assert!(unlimited.should_retry(1000, waited));

    let off = qs_irc::ReconnectPolicy {
        enabled: false,
        ..policy
    };
    assert!(!off.should_retry(0, waited));
}

#[test]
fn the_reconnect_policy_waits_for_its_interval() {
    let policy = qs_irc::ReconnectPolicy {
        enabled: true,
        interval_seconds: 3600,
        retries: 3,
        unlimited_retries: false,
    };
    assert!(
        !policy.should_retry(0, Duration::from_secs(3599)),
        "the interval has not passed yet"
    );
    assert!(
        policy.should_retry(0, Duration::from_secs(3600)),
        "the interval has passed"
    );
}

#[test]
fn the_reconnect_policy_uses_the_defaults_of_the_crate() {
    let policy = qs_irc::ReconnectPolicy::default();
    assert_eq!(
        policy.interval_seconds,
        qs_irc::DEFAULT_RECONNECT_INTERVAL_SECONDS
    );
    assert_eq!(policy.retries, qs_irc::DEFAULT_RECONNECT_RETRIES);
    assert_eq!(
        policy.interval(),
        Duration::from_secs(qs_irc::DEFAULT_RECONNECT_INTERVAL_SECONDS)
    );
}

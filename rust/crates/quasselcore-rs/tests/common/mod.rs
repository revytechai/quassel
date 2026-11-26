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

//! What the integration tests need to start a core.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use qs_storage::{Backend, Storage};
use quasselcore_rs::server::Core;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// A core the tests talk to.
pub struct TestCore {
    /// The address the core listens on.
    pub address: SocketAddr,
    /// Every address the core listens on.
    pub addresses: Vec<SocketAddr>,
    /// The storage of the core.
    pub storage: Backend,
    /// The core itself.
    pub core: Arc<Core>,
}

/// Starts a core with a user and a self-signed certificate.
pub async fn test_core(directory: &Path, user: &str, password: &str) -> TestCore {
    test_core_on(
        directory,
        user,
        password,
        &[SocketAddr::from(([127, 0, 0, 1], 0))],
    )
    .await
}

/// Starts a core that listens on the given addresses.
pub async fn test_core_on(
    directory: &Path,
    user: &str,
    password: &str,
    addresses: &[SocketAddr],
) -> TestCore {
    let (certificate, key) = write_certificate(directory);
    let storage = Backend::open_sqlite(&directory.join("quassel-storage.sqlite"))
        .await
        .expect("opening the storage");
    storage
        .add_user(user, password)
        .await
        .expect("adding the user");

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("the protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .expect("the certificate and the key");

    let core = Arc::new(Core {
        storage: storage.clone(),
        tls: Some(Arc::new(config)),
        require_tls: false,
        restore: false,
        handshake_timeout: std::time::Duration::from_secs(5),
        max_connections: 16,
        limiter: Arc::new(quasselcore_rs::login::LoginLimiter::new()),
    });
    let listeners = Core::bind(addresses).await.expect("binding");
    let bound: Vec<SocketAddr> = listeners
        .iter()
        .map(|listener| listener.local_addr().expect("the address"))
        .collect();
    let serving = (*core).clone();
    tokio::spawn(async move {
        let _ = serving.serve(listeners).await;
    });

    TestCore {
        address: bound[0],
        addresses: bound,
        storage,
        core,
    }
}

/// Writes a self-signed certificate, the way `quasselCert.pem` holds it:
/// the private key first, then the certificate.
fn write_certificate(directory: &Path) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()])
        .expect("generating a certificate");
    let certificate = certified.cert.der().clone();
    let pem = format!(
        "{}{}",
        certified.signing_key.serialize_pem(),
        certified.cert.pem()
    );
    std::fs::write(directory.join("quasselCert.pem"), pem).expect("writing the certificate");
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        certified.signing_key.serialize_der(),
    ));
    (certificate, key)
}

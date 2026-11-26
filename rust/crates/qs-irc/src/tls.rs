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
The TLS side of an IRC connection.

The C++ core has two settings for an IRC server: _use SSL_ and _verify
certificate_.  With verification off, the core accepts any certificate, which
is what most self-signed IRC servers need — but then nobody knows whether
they are talking to the server they think they are.

This crate adds a third option, which is what the IRCv3 `certfp` idea
promises: pin the SHA-256 fingerprint of the server's certificate.  The
certificate is then exactly the one the administrator wrote down, even
though it is self-signed and nobody signed it.

Verification is __on__ by default; a fingerprint pin does not turn it off, it
replaces the chain check with an exact match on the leaf certificate.
*/

use std::sync::Arc;

use rustls::client::danger::{ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, RootCertStore};
use sha2::{Digest, Sha256};

use crate::error::IrcError;

/// The length of a SHA-256 fingerprint.
pub const FINGERPRINT_LEN: usize = 32;

/// The SHA-256 fingerprint of a certificate, as the `certfp` specification
/// writes it: 64 lower case hex characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint([u8; FINGERPRINT_LEN]);

impl Fingerprint {
    /// The fingerprint of a DER encoded certificate.
    pub fn of_certificate(der: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(der);
        let digest = hasher.finalize();
        let mut bytes = [0u8; FINGERPRINT_LEN];
        bytes.copy_from_slice(&digest);
        Self(bytes)
    }

    /// Parses a fingerprint from its hex form.
    ///
    /// Returns `None` when the text is not exactly 64 hexadecimal
    /// characters.  Anything else — a text that is too short, a text that is
    /// too long, a text with a character that is not a hexadecimal digit —
    /// is refused, and nothing is guessed.
    pub fn from_hex(text: &str) -> Option<Self> {
        let text = text.trim().replace([':', ' '], "");
        if text.len() != FINGERPRINT_LEN * 2 {
            return None;
        }
        // The text is cut into pairs of characters below, and a character
        // that is not ASCII would be cut in the middle.
        if !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let mut bytes = [0u8; FINGERPRINT_LEN];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// The raw bytes of the fingerprint.
    pub fn as_bytes(&self) -> &[u8; FINGERPRINT_LEN] {
        &self.0
    }

    /// The hex form of the fingerprint.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// How the certificate of an IRC server is checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServerVerification {
    /// Check the certificate against the system's trusted roots.
    ///
    /// This is the default, and it is what "verification on" means.
    #[default]
    SystemRoots,
    /// Accept the certificate only when its SHA-256 fingerprint is the pinned
    /// one.
    ///
    /// This is how a self-signed server is used without turning the
    /// verification off.
    PinnedSha256(Fingerprint),
    /// Accept any certificate.
    ///
    /// The C++ core does this when the network's "verify certificate" is off.
    Disabled,
}

/// A verifier that pins the certificate's fingerprint.
#[derive(Debug)]
struct PinnedServerVerifier {
    inner: Arc<WebPkiServerVerifier>,
    provider: Arc<CryptoProvider>,
    pinned: Option<Fingerprint>,
    disabled: bool,
}

impl ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        if self.disabled {
            // The user asked for no verification at all; this is what the
            // C++ core does when the network's "verify certificate" is off.
            return Ok(ServerCertVerified::assertion());
        }
        if let Some(pinned) = self.pinned {
            // The pin replaces the chain check: the certificate has to be
            // exactly the one that was pinned down.
            if Fingerprint::of_certificate(end_entity.as_ref()) == pinned {
                return Ok(ServerCertVerified::assertion());
            }
            return Err(TlsError::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ));
        }
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// The roots the verifier trusts.
fn system_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    // The system's certificates are a nice-to-have: a core that cannot read
    // them can still pin a fingerprint, and it never trusts anything it does
    // not know.
    let certificates = rustls_native_certs::load_native_certs();
    for certificate in certificates.certs {
        let _ = roots.add(certificate);
    }
    roots
}

/// Builds the client configuration for an IRC server.
pub fn client_config(
    provider: Arc<CryptoProvider>,
    verification: ServerVerification,
) -> Result<rustls::ClientConfig, IrcError> {
    let inner = WebPkiServerVerifier::builder_with_provider(
        Arc::new(system_roots()),
        Arc::clone(&provider),
    )
    .build()
    .map_err(|error| IrcError::Tls(error.to_string()))?;
    let verifier = PinnedServerVerifier {
        inner,
        provider: Arc::clone(&provider),
        pinned: match verification {
            ServerVerification::SystemRoots | ServerVerification::Disabled => None,
            ServerVerification::PinnedSha256(fingerprint) => Some(fingerprint),
        },
        disabled: matches!(verification, ServerVerification::Disabled),
    };
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| IrcError::Tls(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(config)
}

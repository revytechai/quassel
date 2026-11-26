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
The IRC connection.

A connection to an IRC server is a TCP connection, optionally wrapped in TLS.
IRC is a line based protocol: every message is a line that ends in CRLF.
*/

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::error::IrcError;
use crate::tls::{client_config, Fingerprint, ServerVerification};

/// The biggest line the core reads from an IRC server.
pub const MAX_LINE: usize = 16 * 1024;

/// The default number of seconds between two reconnect attempts.
pub const DEFAULT_RECONNECT_INTERVAL_SECONDS: u64 = 60;

/// The default number of reconnect attempts before the core gives up.
pub const DEFAULT_RECONNECT_RETRIES: u32 = 20;

/// Checks that a line may be written to a server.
///
/// IRC is a line based protocol: a line ends with CRLF.  A line that holds a
/// carriage return or a line feed would end the command early, and
/// everything behind it would reach the server as another command — so
/// whoever writes the text of a line (a user, a nickname, a topic) could
/// make the core send commands nobody asked for.
///
/// The check is public so that a caller can test a line before it is put
/// together.
pub fn check_line(line: &str) -> Result<(), IrcError> {
    if line.contains(['\r', '\n']) {
        return Err(IrcError::LineBreak);
    }
    Ok(())
}

/// A stream the core reads lines from and writes lines to.
trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

/// An IRC server the core can talk to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcServer {
    /// The server's host name.
    pub host: String,
    /// The server's port.
    pub port: u16,
    /// Whether the connection is encrypted.
    pub use_ssl: bool,
    /// How the server's certificate is checked.
    pub verification: ServerVerification,
    /// The password for the server, if it needs one.
    pub password: Option<String>,
}

impl IrcServer {
    /// A server that is reached without encryption.
    pub fn plain(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            use_ssl: false,
            verification: ServerVerification::default(),
            password: None,
        }
    }

    /// A server that is reached over TLS, with the certificate checked
    /// against the system's roots.
    pub fn tls(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            use_ssl: true,
            verification: ServerVerification::SystemRoots,
            password: None,
        }
    }

    /// A server that is reached over TLS and whose certificate is pinned.
    pub fn tls_pinned(host: impl Into<String>, port: u16, fingerprint: Fingerprint) -> Self {
        Self {
            host: host.into(),
            port,
            use_ssl: true,
            verification: ServerVerification::PinnedSha256(fingerprint),
            password: None,
        }
    }

    /// Sets the password the server needs.
    ///
    /// The core sends the password as a `PASS` line when it authenticates.
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }
}

/// A connection to an IRC server.
pub struct IrcConnection {
    stream: Box<dyn Stream>,
    buffer: Vec<u8>,
    fingerprint: Option<Fingerprint>,
}

impl IrcConnection {
    /// Connects to a server.
    pub async fn connect(
        server: &IrcServer,
        provider: Arc<rustls::crypto::CryptoProvider>,
    ) -> Result<Self, IrcError> {
        let stream = TcpStream::connect((server.host.as_str(), server.port))
            .await
            .map_err(|error| IrcError::Connect {
                host: server.host.clone(),
                port: server.port,
                reason: error.to_string(),
            })?;
        if !server.use_ssl {
            return Ok(Self {
                stream: Box::new(stream),
                buffer: Vec::new(),
                fingerprint: None,
            });
        }

        let config = client_config(Arc::clone(&provider), server.verification)?;
        let connector = TlsConnector::from(Arc::new(config));
        let name = rustls::pki_types::ServerName::try_from(server.host.clone())
            .map_err(|_| IrcError::Tls(format!("{} is not a valid host name", server.host)))?;
        let tls = connector
            .connect(name, stream)
            .await
            .map_err(|error| IrcError::Tls(error.to_string()))?;

        let fingerprint = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certificates| certificates.first())
            .map(|certificate| Fingerprint::of_certificate(certificate.as_ref()));

        Ok(Self {
            stream: Box::new(tls),
            buffer: Vec::new(),
            fingerprint,
        })
    }

    /// The fingerprint of the server's certificate, when the connection is
    /// encrypted.
    pub fn fingerprint(&self) -> Option<Fingerprint> {
        self.fingerprint
    }

    /// Writes a line to the server.
    ///
    /// A line that holds a carriage return or a line feed is refused: it
    /// would send more than one command.
    pub async fn write_line(&mut self, line: &str) -> Result<(), IrcError> {
        check_line(line)?;
        let mut payload = Vec::with_capacity(line.len() + 2);
        payload.extend_from_slice(line.as_bytes());
        payload.extend_from_slice(b"\r\n");
        self.stream
            .write_all(&payload)
            .await
            .map_err(|error| IrcError::Io(error.to_string()))
    }

    /// Authenticates with the server, when the server needs a password.
    ///
    /// The core calls this after the connection is up and before it
    /// registers, so the `PASS` line reaches the server before the `NICK`
    /// and `USER` lines.  The password is not sent from `connect`: the order
    /// of the lines the core sends is the caller's business.
    pub async fn authenticate(&mut self, server: &IrcServer) -> Result<(), IrcError> {
        let Some(password) = server.password.as_deref() else {
            return Ok(());
        };
        self.write_line(&format!("PASS {password}")).await
    }

    /// Reads the next line from the server.
    ///
    /// Returns `Ok(None)` when the server closed the connection.
    pub async fn read_line(&mut self) -> Result<Option<String>, IrcError> {
        loop {
            if let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
                // The line has to fit before it is taken out of the buffer:
                // a line that is too long is refused, even when it ends in
                // the very chunk that carried it.
                if line_length(&self.buffer, position) > MAX_LINE {
                    return Err(IrcError::LineTooLong(MAX_LINE));
                }
                let line: Vec<u8> = self.buffer.drain(..=position).collect();
                let text = String::from_utf8_lossy(&line)
                    .trim_end_matches(['\r', '\n'])
                    .to_owned();
                return Ok(Some(text));
            }
            if self.buffer.len() > MAX_LINE {
                return Err(IrcError::LineTooLong(MAX_LINE));
            }
            let mut chunk = [0u8; 4096];
            let read = self
                .stream
                .read(&mut chunk)
                .await
                .map_err(|error| IrcError::Io(error.to_string()))?;
            if read == 0 {
                return Ok(None);
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    /// Closes the connection.
    pub async fn close(&mut self) -> Result<(), IrcError> {
        self.stream
            .shutdown()
            .await
            .map_err(|error| IrcError::Io(error.to_string()))
    }
}

/// The length of the line that ends at `position`, without its CR.
///
/// The position is the place of the line feed that ends the line.
fn line_length(buffer: &[u8], position: usize) -> usize {
    if position > 0 && buffer[position - 1] == b'\r' {
        position - 1
    } else {
        position
    }
}

/// When the core reconnects to a server on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectPolicy {
    /// Whether the core reconnects at all.
    pub enabled: bool,
    /// The seconds between two attempts.
    pub interval_seconds: u64,
    /// How many attempts the core makes before it gives up.
    pub retries: u32,
    /// Whether the core never gives up.
    pub unlimited_retries: bool,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: DEFAULT_RECONNECT_INTERVAL_SECONDS,
            retries: DEFAULT_RECONNECT_RETRIES,
            unlimited_retries: false,
        }
    }
}

impl ReconnectPolicy {
    /// The time the core waits between two attempts.
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_seconds)
    }

    /// Whether the core should try again after `attempt` failed attempts.
    ///
    /// `elapsed` is the time that passed since the last attempt.  The core
    /// waits `interval_seconds` between two attempts, so a retry that comes
    /// too early is refused, and a policy with a long interval really does
    /// wait.
    pub fn should_retry(&self, attempt: u32, elapsed: Duration) -> bool {
        if !self.enabled {
            return false;
        }
        if !self.unlimited_retries && attempt >= self.retries {
            return false;
        }
        elapsed >= self.interval()
    }
}

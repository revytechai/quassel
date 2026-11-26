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

//! Tests for the IRC connection.
//!
//! The server here is a plain TCP server that records what the core sends,
//! so the tests can look at the bytes on the wire.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use qs_irc::{check_line, IrcConnection, IrcError, IrcServer, MAX_LINE};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A plain IRC server that records what the core sends.
struct RecordingServer {
    host: String,
    port: u16,
    raw: Arc<Mutex<Vec<u8>>>,
    lines: Arc<Mutex<Vec<String>>>,
    sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

impl RecordingServer {
    /// Starts a server that waits for one connection.
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binding");
        let address = listener.local_addr().expect("the address");
        let raw = Arc::new(Mutex::new(Vec::new()));
        let lines = Arc::new(Mutex::new(Vec::new()));
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let task_raw = Arc::clone(&raw);
        let task_lines = Arc::clone(&lines);

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accepting");
            let (mut reader, mut writer) = stream.into_split();

            let reader_task = tokio::spawn(async move {
                let mut pending = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let read = match reader.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => read,
                    };
                    task_raw
                        .lock()
                        .expect("the lock")
                        .extend_from_slice(&chunk[..read]);
                    pending.extend_from_slice(&chunk[..read]);
                    while let Some(position) = pending.iter().position(|byte| *byte == b'\n') {
                        let line: Vec<u8> = pending.drain(..=position).collect();
                        let text = String::from_utf8_lossy(&line)
                            .trim_end_matches('\n')
                            .trim_end_matches('\r')
                            .to_owned();
                        task_lines.lock().expect("the lock").push(text);
                    }
                }
            });

            while let Some(chunk) = receiver.recv().await {
                if writer.write_all(&chunk).await.is_err() {
                    break;
                }
            }
            reader_task.abort();
        });

        Self {
            host: address.ip().to_string(),
            port: address.port(),
            raw,
            lines,
            sender,
        }
    }

    /// The server description the core connects to.
    fn server(&self) -> IrcServer {
        IrcServer::plain(self.host.clone(), self.port)
    }

    /// Sends bytes to the core.
    fn send(&self, bytes: &[u8]) {
        self.sender.send(bytes.to_vec()).expect("the sender");
    }

    /// Waits until the server has seen `count` lines.
    async fn wait_for_lines(&self, count: usize) -> Vec<String> {
        for _ in 0..500 {
            let lines = self.lines.lock().expect("the lock").clone();
            if lines.len() >= count {
                return lines;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the server did not receive {count} lines");
    }

    /// The bytes the server received.
    fn raw_bytes(&self) -> Vec<u8> {
        self.raw.lock().expect("the lock").clone()
    }
}

/// Connects to a plain server.
async fn connect(server: &RecordingServer) -> IrcConnection {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    IrcConnection::connect(&server.server(), provider)
        .await
        .expect("connecting")
}

/// Connects to a plain server and authenticates with it.
async fn connect_and_authenticate(
    server: &RecordingServer,
    password: Option<&str>,
) -> IrcConnection {
    let mut irc_server = server.server();
    if let Some(password) = password {
        irc_server = irc_server.with_password(password);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut connection = IrcConnection::connect(&irc_server, provider)
        .await
        .expect("connecting");
    connection
        .authenticate(&irc_server)
        .await
        .expect("authenticating");
    connection
}

#[tokio::test]
async fn a_line_with_a_line_break_is_refused() {
    let server = RecordingServer::start().await;
    let mut connection = connect(&server).await;
    let result = connection
        .write_line("PRIVMSG #x :hello\r\nJOIN #secret")
        .await;
    assert!(
        matches!(result, Err(IrcError::LineBreak)),
        "a line with a line break must be refused"
    );
    // The valid line behind the refused one shows that nothing of the
    // refused line reached the server.
    connection.write_line("PING :one").await.expect("writing");
    let lines = server.wait_for_lines(1).await;
    assert_eq!(lines, vec!["PING :one".to_string()]);
    assert_eq!(server.raw_bytes(), b"PING :one\r\n".to_vec());
}

#[tokio::test]
async fn a_line_with_a_line_feed_is_refused() {
    let server = RecordingServer::start().await;
    let mut connection = connect(&server).await;
    let result = connection.write_line("JOIN #secret\nQUIT").await;
    assert!(
        matches!(result, Err(IrcError::LineBreak)),
        "a line with a line feed must be refused"
    );
}

#[test]
fn the_line_check_is_public() {
    assert!(check_line("PRIVMSG #channel :hello").is_ok());
    assert!(matches!(
        check_line("PRIVMSG #channel :hello\r\nJOIN #secret"),
        Err(IrcError::LineBreak)
    ));
    assert!(matches!(
        check_line("PRIVMSG #channel :hello\nJOIN #secret"),
        Err(IrcError::LineBreak)
    ));
}

#[tokio::test]
async fn a_valid_line_is_written_once() {
    let server = RecordingServer::start().await;
    let mut connection = connect(&server).await;
    connection.write_line("PING :one").await.expect("writing");
    let lines = server.wait_for_lines(1).await;
    assert_eq!(lines, vec!["PING :one".to_string()]);
    assert_eq!(server.raw_bytes(), b"PING :one\r\n".to_vec());
}

#[tokio::test]
async fn a_line_that_is_too_long_is_refused() {
    let server = RecordingServer::start().await;
    let mut connection = connect(&server).await;
    let mut payload = vec![b'a'; MAX_LINE + 1];
    payload.extend_from_slice(b"\r\n");
    server.send(&payload);
    let result = connection.read_line().await;
    assert!(
        matches!(result, Err(IrcError::LineTooLong(_))),
        "a line of more than {MAX_LINE} bytes must be refused"
    );
}

#[tokio::test]
async fn a_line_of_the_biggest_size_is_read() {
    let server = RecordingServer::start().await;
    let mut connection = connect(&server).await;
    let mut payload = vec![b'a'; MAX_LINE];
    payload.extend_from_slice(b"\r\n");
    server.send(&payload);
    let line = connection.read_line().await.expect("reading");
    assert_eq!(line.map(|text| text.len()), Some(MAX_LINE));
}

#[tokio::test]
async fn the_password_is_sent_when_the_core_authenticates() {
    let server = RecordingServer::start().await;
    connect_and_authenticate(&server, Some("secret")).await;
    let lines = server.wait_for_lines(1).await;
    assert_eq!(lines, vec!["PASS secret".to_string()]);
}

#[tokio::test]
async fn a_server_without_a_password_authenticates_without_a_pass_line() {
    let server = RecordingServer::start().await;
    let mut connection = connect_and_authenticate(&server, None).await;
    // The first line the server sees is the one the core writes after the
    // authentication, so no PASS line was sent.
    connection.write_line("PING :one").await.expect("writing");
    let lines = server.wait_for_lines(1).await;
    assert_eq!(lines, vec!["PING :one".to_string()]);
    assert_eq!(server.raw_bytes(), b"PING :one\r\n".to_vec());
}

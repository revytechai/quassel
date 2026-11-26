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

//! The core's server: the listener, the handshake and the sessions.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use qs_proto::{ConnectionFeatures, FrameDecoder, Handshake, HandshakeReply, MAGIC};
use qs_storage::{Backend, Storage};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

use crate::logging;
use crate::login::LoginLimiter;
use crate::session::Session;

/// A stream the core reads messages from and writes messages to.
pub trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

/// The core.
#[derive(Clone)]
pub struct Core {
    /// Where the data lives.
    pub storage: Backend,
    /// The TLS configuration, when the core has a certificate.
    pub tls: Option<Arc<rustls::ServerConfig>>,
    /// Whether the core refuses clients that do not use TLS.
    pub require_tls: bool,
    /// Whether the core restores the state of the last run.
    pub restore: bool,
    /// How long a client has to send its handshake.
    pub handshake_timeout: Duration,
    /// How many clients may be connected at the same time.
    pub max_connections: usize,
    /// The limiter of the login attempts.
    pub limiter: Arc<LoginLimiter>,
}

/// A network that was connected when the core stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectedNetwork {
    /// The id of the network.  This is the id a restore needs, not the id of
    /// the user the network belongs to.
    pub network_id: i64,
    /// The id of the user the network belongs to.
    pub user_id: i64,
    /// The name of the network.
    pub networkname: String,
}

impl Core {
    /// The networks that were connected when the core stopped.
    ///
    /// The C++ core reconnects to them when it starts again; the IRC layer
    /// is not in place yet, so the core only reports them.
    pub async fn connected_networks(&self) -> anyhow::Result<Vec<ConnectedNetwork>> {
        let mut networks = Vec::new();
        for user in self.storage.users().await? {
            for network in self.storage.networks(user.userid).await? {
                if network.connected {
                    networks.push(ConnectedNetwork {
                        network_id: network.networkid,
                        user_id: user.userid,
                        networkname: network.networkname,
                    });
                }
            }
        }
        Ok(networks)
    }
}

impl Core {
    /// Binds the given addresses.
    ///
    /// The listeners are returned, so that a caller (a test, for instance)
    /// can learn the port the core ended up on.
    pub async fn bind(addresses: &[SocketAddr]) -> anyhow::Result<Vec<TcpListener>> {
        let mut listeners = Vec::with_capacity(addresses.len());
        for address in addresses {
            let listener = TcpListener::bind(address).await?;
            logging::info(&format!(
                "Listening for GUI clients on {} port {}",
                address.ip(),
                address.port()
            ));
            listeners.push(listener);
        }
        Ok(listeners)
    }

    /// Serves the clients on the given listeners.
    ///
    /// Every listener gets its own accept loop: the core may listen on
    /// several addresses, and a client that connects to any of them has to
    /// be served.
    pub async fn serve(self, listeners: Vec<TcpListener>) -> anyhow::Result<()> {
        let acceptor = self.tls.clone().map(TlsAcceptor::from);
        let core = Arc::new(self);
        let limit = Arc::new(Semaphore::new(core.max_connections.max(1)));

        let mut loops = Vec::with_capacity(listeners.len());
        for listener in listeners {
            let core = Arc::clone(&core);
            let acceptor = acceptor.clone();
            let limit = Arc::clone(&limit);
            loops.push(tokio::spawn(async move {
                accept_loop(listener, core, acceptor, limit).await;
            }));
        }
        for handle in loops {
            let _ = handle.await;
        }
        Ok(())
    }
}

/// Accepts the clients of one listener until the listener goes away.
async fn accept_loop(
    listener: TcpListener,
    core: Arc<Core>,
    acceptor: Option<TlsAcceptor>,
    limit: Arc<Semaphore>,
) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(error) => {
                logging::warning(&format!("cannot accept a client: {error}"));
                return;
            }
        };
        let permit = match limit.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                logging::warning(&format!(
                    "{peer}: too many clients are connected; this one is refused"
                ));
                continue;
            }
        };
        let core = Arc::clone(&core);
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            // The permit is held as long as the client is served.
            let _permit = permit;
            if let Err(error) = handle_connection(core, stream, peer, acceptor).await {
                logging::warning(&format!("{peer}: {error}"));
            }
        });
    }
}

/// Listens on the given addresses and serves the clients.
pub async fn run(core: Core, addresses: &[SocketAddr]) -> anyhow::Result<()> {
    let listeners = Core::bind(addresses).await?;
    core.serve(listeners).await
}

/// Serves one client.
async fn handle_connection(
    core: Arc<Core>,
    mut stream: TcpStream,
    peer: SocketAddr,
    acceptor: Option<TlsAcceptor>,
) -> anyhow::Result<()> {
    stream.set_nodelay(true).ok();

    // The handshake is plaintext: the core reads it before the connection
    // becomes a TLS one.  A client that sends a TLS ClientHello right away
    // is read as a legacy protocol packet and dropped.
    //
    // The client has to send its handshake in time: a connection that stays
    // silent must not hold a task and a socket for ever.
    let handshake = tokio::time::timeout(core.handshake_timeout, read_handshake(&mut stream))
        .await
        .map_err(|_| anyhow::anyhow!("the client did not send its handshake in time"))??;
    if handshake.features & ConnectionFeatures::ENCRYPTION == 0 && core.require_tls {
        anyhow::bail!("TLS is required, but the client did not ask for it");
    }

    let protocol = qs_proto::select_protocol(&handshake.protocols)
        .ok_or_else(|| anyhow::anyhow!("the client speaks no protocol this core knows"))?;

    let encryption = acceptor.is_some() && handshake.features & ConnectionFeatures::ENCRYPTION != 0;
    let connection_features = if encryption {
        ConnectionFeatures::from_bits(ConnectionFeatures::ENCRYPTION)
    } else {
        ConnectionFeatures::none()
    };
    let reply = HandshakeReply {
        protocol,
        features: 0,
        connection_features,
    };
    stream.write_all(&reply.encode()).await?;
    stream.flush().await?;
    logging::info(&format!("{peer}: using the {} protocol", protocol.name()));

    let mut boxed: Box<dyn Stream> = if encryption {
        let acceptor = acceptor.expect("the acceptor is there when encryption is on");
        let tls = acceptor.accept(stream).await?;
        Box::new(tls)
    } else {
        Box::new(stream)
    };

    Session::new(core, peer).run(&mut boxed).await
}

/// Reads the plaintext handshake.
///
/// The bytes are read exactly, so that nothing of the TLS handshake that
/// follows is swallowed.
async fn read_handshake(stream: &mut TcpStream) -> anyhow::Result<Handshake> {
    let mut magic = [0u8; 4];
    stream.read_exact(&mut magic).await?;
    let magic = u32::from_be_bytes(magic);
    if magic & 0xffff_ff00 != MAGIC {
        anyhow::bail!("not a Quassel handshake (magic {magic:#010x})");
    }
    let features = (magic & 0xff) as u8;

    let mut protocols = Vec::new();
    loop {
        let mut word = [0u8; 4];
        stream.read_exact(&mut word).await?;
        let word = u32::from_be_bytes(word);
        let descriptor = qs_proto::ProtocolDescriptor::decode(word)?;
        let last = descriptor.last;
        protocols.push(descriptor);
        if last {
            break;
        }
        if protocols.len() >= 16 {
            anyhow::bail!("the client sent more than sixteen protocols");
        }
    }
    Ok(Handshake {
        features,
        protocols,
    })
}

/// Reads the next message out of a stream.
pub async fn read_message(
    stream: &mut Box<dyn Stream>,
    decoder: &mut FrameDecoder,
) -> anyhow::Result<Vec<u8>> {
    loop {
        if let Some(message) = decoder.next_frame()? {
            return Ok(message);
        }
        let mut chunk = [0u8; 8192];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("the client closed the connection");
        }
        decoder.push(&chunk[..read]);
    }
}

/// Writes a message to a stream.
pub async fn write_message(stream: &mut Box<dyn Stream>, payload: &[u8]) -> anyhow::Result<()> {
    stream.write_all(&qs_proto::frame(payload)).await?;
    stream.flush().await?;
    Ok(())
}

/// Turns the `--listen` values into the addresses the core binds to.
pub fn listen_addresses(listen: &[String], port: u16) -> anyhow::Result<Vec<SocketAddr>> {
    let mut addresses = Vec::new();
    for entry in listen {
        let entry = entry.trim();
        match entry {
            "" | "*" => {
                addresses.push(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port));
                addresses.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port));
            }
            "::" => addresses.push(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port)),
            "0.0.0.0" => addresses.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)),
            other => {
                let ip: IpAddr = other.parse().map_err(|_| {
                    anyhow::anyhow!("{other} is not an address the core can listen on")
                })?;
                addresses.push(SocketAddr::new(ip, port));
            }
        }
    }
    if addresses.is_empty() {
        addresses.push(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port));
    }
    Ok(addresses)
}

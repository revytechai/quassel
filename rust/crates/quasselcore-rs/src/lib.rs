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

//! The Quassel core, in Rust.
//!
//! The core holds the IRC connections and the chat history; the clients
//! connect to it and talk the Quassel protocol to it.
//!
//! The first thing a client does is the handshake: it sends the magic number
//! and the list of protocols it speaks, and the core answers with the
//! protocol it picked and the connection features it offers.  Only then does
//! the connection become a TLS one.
//!
//! The binary is a thin layer around this library: it reads the command
//! line, opens the storage backend and the certificate, and serves the
//! clients.

#![deny(missing_docs)]

pub mod cli;
pub mod config;
pub mod logging;
pub mod login;
pub mod postgres;
pub mod server;
pub mod session;

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
Errors of the IRC layer.
*/

/// Everything that can go wrong while talking to an IRC server.
#[derive(Debug, thiserror::Error)]
pub enum IrcError {
    /// The server could not be reached.
    #[error("cannot connect to {host}:{port}: {reason}")]
    Connect {
        /// The server's host name.
        host: String,
        /// The server's port.
        port: u16,
        /// Why it failed.
        reason: String,
    },
    /// The TLS handshake failed.
    #[error("TLS error: {0}")]
    Tls(String),
    /// The certificate did not match the pinned fingerprint.
    #[error("the server's certificate does not match the pinned fingerprint")]
    FingerprintMismatch,
    /// The connection broke.
    #[error("connection error: {0}")]
    Io(String),
    /// The server sent a line that is too long.
    #[error("the server sent a line of more than {0} bytes")]
    LineTooLong(usize),
    /// The line to write contained a line break.
    ///
    /// Writing it would have ended the command early and sent everything
    /// after the break as another command.
    #[error("the line contains a line break and would send more than one command")]
    LineBreak,
}

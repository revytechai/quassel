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

//! The command line of the core.
//!
//! The flags are the ones the C++ core has, so that the same rc script and
//! the same habits work.

use std::path::PathBuf;

use clap::Parser;

/// The Quassel core, in Rust.
#[derive(Debug, Parser)]
#[command(
    name = "quasselcore-rs",
    version,
    about = "Quassel IRC core, in Rust",
    long_about = "Quassel IRC core, in Rust.\n\n\
                  The core holds the IRC connections and the chat history; the \
                  clients connect to it and talk the Quassel protocol to it."
)]
pub struct Cli {
    /// The directory holding the configuration files, the database and the
    /// TLS certificate.
    #[arg(short = 'c', long = "configdir", value_name = "path")]
    pub configdir: Option<PathBuf>,

    /// The addresses the core listens on.
    #[arg(
        long = "listen",
        value_name = "address",
        default_value = "::,0.0.0.0",
        value_delimiter = ','
    )]
    pub listen: Vec<String>,

    /// The port the core listens at.
    #[arg(
        short = 'p',
        long = "port",
        value_name = "port",
        default_value_t = 4242
    )]
    pub port: u16,

    /// Do not restore the last core's state.
    ///
    /// This option is reserved: the core does not restore the state of the
    /// last run yet, and it says so when the option is used.
    #[arg(short = 'n', long = "norestore")]
    pub norestore: bool,

    /// Use this storage backend for this run.
    ///
    /// The data is not migrated: a database of the other backend is not
    /// touched, and the core does not move anything into it.
    #[arg(long = "select-backend", value_name = "backendidentifier")]
    pub select_backend: Option<String>,

    /// Select the authentication backend.
    ///
    /// The only authenticator there is, is `Database`.
    #[arg(long = "select-authenticator", value_name = "authidentifier")]
    pub select_authenticator: Option<String>,

    /// Add a new core user.
    #[arg(long = "add-user", conflicts_with = "change_userpass")]
    pub add_user: bool,

    /// Change the password of a user.
    #[arg(long = "change-userpass", value_name = "username")]
    pub change_userpass: Option<String>,

    /// The user name for `--add-user`.
    ///
    /// The name of the user to change the password of is the one of
    /// `--change-userpass`; the two options cannot be used together.
    ///
    /// The name is not a secret; the password never is, and it is never
    /// taken from the command line.
    #[arg(
        long = "user",
        value_name = "username",
        conflicts_with = "change_userpass"
    )]
    pub user: Option<String>,

    /// Read the password from this file instead of asking for it.
    ///
    /// The file holds the password on its first line; a second line is
    /// ignored.
    #[arg(long = "password-file", value_name = "path")]
    pub password_file: Option<PathBuf>,

    /// Require TLS for the client connections.
    #[arg(long = "require-tls")]
    pub require_tls: bool,

    /// Require TLS for the client connections (the old name of the flag).
    #[arg(long = "require-ssl")]
    pub require_ssl: bool,

    /// The path to the TLS certificate.
    #[arg(long = "tls-cert", value_name = "path")]
    pub tls_cert: Option<PathBuf>,

    /// The path to the TLS key.
    #[arg(long = "tls-key", value_name = "path")]
    pub tls_key: Option<PathBuf>,

    /// The path to the TLS certificate (the old name of the flag).
    #[arg(long = "ssl-cert", value_name = "path")]
    pub ssl_cert: Option<PathBuf>,

    /// The path to the TLS key (the old name of the flag).
    #[arg(long = "ssl-key", value_name = "path")]
    pub ssl_key: Option<PathBuf>,

    /// Use the user's name as the ident reply.
    ///
    /// This option is reserved: the core does not answer ident requests yet,
    /// and it refuses the option.
    #[arg(long = "strict-ident")]
    pub strict_ident: bool,

    /// The log level: Debug, Info, Warning or Error.
    #[arg(
        short = 'L',
        long = "loglevel",
        value_name = "level",
        default_value = "Info"
    )]
    pub loglevel: String,

    /// Log to this file.
    #[arg(short = 'l', long = "logfile", value_name = "path")]
    pub logfile: Option<PathBuf>,

    /// Enable the debug output.
    #[arg(short = 'd', long = "debug")]
    pub debug: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_name_and_the_user_to_change_cannot_be_used_together() {
        // The user name of `--add-user` and the user of
        // `--change-userpass` must not compete with each other.
        assert!(
            Cli::try_parse_from([
                "quasselcore-rs",
                "--change-userpass",
                "alice",
                "--user",
                "bob"
            ])
            .is_err(),
            "the two user names must not be used together"
        );
    }

    #[test]
    fn adding_a_user_and_changing_a_password_cannot_be_used_together() {
        assert!(
            Cli::try_parse_from(["quasselcore-rs", "--add-user", "--change-userpass", "alice"])
                .is_err(),
            "the two operations must not be used together"
        );
    }

    #[test]
    fn the_addresses_the_core_listens_on_are_read() {
        let cli = Cli::try_parse_from([
            "quasselcore-rs",
            "--listen",
            "127.0.0.1,::1",
            "--port",
            "4243",
        ])
        .expect("the options are valid");
        assert_eq!(cli.listen, vec!["127.0.0.1".to_owned(), "::1".to_owned()]);
        assert_eq!(cli.port, 4243);
    }

    #[test]
    fn the_backend_of_the_command_line_is_read() {
        let cli = Cli::try_parse_from(["quasselcore-rs", "--select-backend", "PostgreSQL"])
            .expect("the options are valid");
        assert_eq!(cli.select_backend.as_deref(), Some("PostgreSQL"));
    }
}

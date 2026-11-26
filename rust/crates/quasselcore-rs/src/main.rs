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

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use qs_storage::{Backend, Storage};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use quasselcore_rs::cli::Cli;
use quasselcore_rs::config::ConfigFile;
use quasselcore_rs::logging;
use quasselcore_rs::postgres::PostgresProperties;
use quasselcore_rs::server::{self, Core};

/// The name of the configuration file in the configuration directory.
const CONFIG_FILE: &str = "quasselcore.conf";
/// The name of the SQLite database in the configuration directory.
const DATABASE_FILE: &str = "quassel-storage.sqlite";
/// The name of the certificate in the configuration directory.
const CERTIFICATE_FILE: &str = "quasselCert.pem";
/// How long a client has to send its handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// How many clients may be connected to the core at the same time.
const MAX_CONNECTIONS: usize = 128;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    logging::set_level(&cli.loglevel);
    logging::set_file(cli.logfile.clone())?;
    if cli.debug {
        logging::set_level("Debug");
    }
    check_options(&cli)?;
    run(&cli).await
}

/// Starts the core.
async fn run(cli: &Cli) -> anyhow::Result<()> {
    let config_dir = config_dir(cli);
    // The directory holds the database, the configuration and the
    // certificate: only the user of the core may read them.
    ConfigFile::create_directory(&config_dir)?;
    let mut config = ConfigFile::load(&config_dir.join(CONFIG_FILE))?;

    let storage = open_storage(cli, &config, &config_dir).await?;
    write_backend_settings(&mut config, &config_dir, &storage)?;

    if cli.add_user {
        return add_user(&storage, cli).await;
    }
    if let Some(username) = cli.change_userpass.clone() {
        return change_userpass(&storage, &username, cli).await;
    }
    serve(cli, storage, &config, &config_dir).await
}

/// Serves the clients.
async fn serve(
    cli: &Cli,
    storage: Backend,
    config: &ConfigFile,
    config_dir: &Path,
) -> anyhow::Result<()> {
    let users = storage.users().await?;
    if users.is_empty() {
        anyhow::bail!(
            "the core is not configured yet: add a user with \
             \"quasselcore-rs --add-user --configdir {}\" first",
            config_dir.display()
        );
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let certificate = certificate_paths(cli, config, config_dir);
    let tls = load_tls(&certificate, &provider).await?;
    if tls.is_none() {
        logging::warning(&format!(
            "no certificate found at {}: the connections are not encrypted. \
             Put the certificate and the key there, or tell the core where they \
             are with --tls-cert and --tls-key",
            certificate.0.display()
        ));
    }
    let require_tls = cli.require_tls || cli.require_ssl;
    if require_tls && tls.is_none() {
        anyhow::bail!(
            "TLS is required, but there is no certificate to serve it with: \
             the core expects one at {}",
            certificate.0.display()
        );
    }

    let addresses = server::listen_addresses(&cli.listen, cli.port)?;
    let core = Core {
        storage,
        tls: tls.map(Arc::new),
        require_tls,
        restore: !cli.norestore,
        handshake_timeout: HANDSHAKE_TIMEOUT,
        max_connections: MAX_CONNECTIONS,
        limiter: Arc::new(quasselcore_rs::login::LoginLimiter::new()),
    };
    if core.restore {
        for network in core.connected_networks().await? {
            logging::info(&format!(
                "the network {} was connected and will be reconnected",
                network.networkname
            ));
        }
    }
    logging::info("Quassel core (Rust) is ready");
    if let Err(error) = server::run(core, &addresses).await {
        logging::error(&format!("{error}"));
        return Err(error);
    }
    Ok(())
}

/// Checks the options the core does not implement.
///
/// An option that is accepted but does nothing is worse than an option that
/// is refused: the administrator believes the core does something it does
/// not do.
fn check_options(cli: &Cli) -> anyhow::Result<()> {
    if let Some(authenticator) = &cli.select_authenticator {
        if !authenticator.eq_ignore_ascii_case("database") {
            anyhow::bail!(
                "the authenticator \"{authenticator}\" is not implemented: \
                 the only authenticator there is, is Database"
            );
        }
    }
    if cli.strict_ident {
        anyhow::bail!(
            "--strict-ident is not implemented: the core does not answer ident requests yet"
        );
    }
    if cli.norestore {
        logging::warning(
            "--norestore is reserved: the core does not restore the state of the last run yet",
        );
    }
    Ok(())
}

/// The configuration directory.
fn config_dir(cli: &Cli) -> PathBuf {
    if let Some(path) = &cli.configdir {
        return path.clone();
    }
    // The same place Qt's QSettings would put it, so that the configuration
    // of the C++ core and of this core can live side by side.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("quassel.org").join("quasselcore")
}

/// Opens the storage backend.
async fn open_storage(
    cli: &Cli,
    config: &ConfigFile,
    config_dir: &Path,
) -> anyhow::Result<Backend> {
    let backend = cli
        .select_backend
        .clone()
        .or_else(|| config.core("StorageSettings\\Backend").map(str::to_owned))
        .unwrap_or_else(|| "SQLite".to_owned());

    match backend.to_ascii_lowercase().as_str() {
        "sqlite" | "sqlite3" => {
            let database = config
                .core("StorageSettings\\ConnectionProperties\\Database")
                .map(PathBuf::from)
                .unwrap_or_else(|| config_dir.join(DATABASE_FILE));
            let path = if database.is_absolute() {
                database
            } else {
                config_dir.join(database)
            };
            logging::info(&format!("Using the SQLite backend at {}", path.display()));
            Ok(Backend::open_sqlite(&path).await?)
        }
        "postgresql" | "postgres" | "pgsql" => {
            let url = postgres_properties(config).url();
            logging::info("Using the PostgreSQL backend");
            Ok(Backend::open_postgres(&url).await?)
        }
        other => anyhow::bail!(
            "unknown storage backend \"{other}\": the core knows SQLite and PostgreSQL"
        ),
    }
}

/// Writes the storage settings into the configuration file.
///
/// The C++ core writes them there too, so that the next start does not have
/// to ask.  The backend the core runs on is written, so that a backend that
/// was asked for on the command line is used again on the next start.  The
/// database that is already in the file is kept: the core must not open
/// another database on the next start than the one it was told to use.
fn write_backend_settings(
    config: &mut ConfigFile,
    config_dir: &Path,
    storage: &Backend,
) -> anyhow::Result<()> {
    config.set("Core", "StorageSettings\\Backend", storage.backend_id());
    if let Backend::Sqlite(_) = storage {
        if config
            .core("StorageSettings\\ConnectionProperties\\Database")
            .is_none()
        {
            config.set(
                "Core",
                "StorageSettings\\ConnectionProperties\\Database",
                DATABASE_FILE,
            );
        }
    }
    if config.core("AuthSettings\\Authenticator").is_none() {
        config.set("Core", "AuthSettings\\Authenticator", "Database");
    }
    config
        .save(&config_dir.join(CONFIG_FILE))
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// The connection properties of the PostgreSQL backend.
fn postgres_properties(config: &ConfigFile) -> PostgresProperties {
    let property = |name: &str| {
        config
            .core(&format!("StorageSettings\\ConnectionProperties\\{name}"))
            .map(str::to_owned)
    };
    PostgresProperties {
        host: property("Host").unwrap_or_else(|| "localhost".to_owned()),
        port: property("Port").unwrap_or_else(|| "5432".to_owned()),
        database: property("Database").unwrap_or_else(|| "quassel".to_owned()),
        user: property("User").unwrap_or_default(),
        password: property("Password").unwrap_or_default(),
    }
}

/// Where the certificate and the key are.
fn certificate_paths(cli: &Cli, config: &ConfigFile, config_dir: &Path) -> (PathBuf, PathBuf) {
    let certificate = cli
        .tls_cert
        .clone()
        .or_else(|| cli.ssl_cert.clone())
        .or_else(|| config.get("Core\\SSL", "Certificate").map(PathBuf::from))
        .unwrap_or_else(|| config_dir.join(CERTIFICATE_FILE));
    let key = cli
        .tls_key
        .clone()
        .or_else(|| cli.ssl_key.clone())
        .or_else(|| config.get("Core\\SSL", "Key").map(PathBuf::from))
        .unwrap_or_else(|| certificate.clone());
    (certificate, key)
}

/// Reads the certificate and the key.
async fn load_tls(
    paths: &(PathBuf, PathBuf),
    provider: &Arc<CryptoProvider>,
) -> anyhow::Result<Option<rustls::ServerConfig>> {
    let (certificate_path, key_path) = paths;
    let Ok(certificate_file) = std::fs::File::open(certificate_path) else {
        logging::warning(&format!(
            "cannot open the certificate file {}",
            certificate_path.display()
        ));
        return Ok(None);
    };
    let mut reader = std::io::BufReader::new(certificate_file);
    let certificates: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if certificates.is_empty() {
        logging::warning(&format!(
            "the certificate file {} holds no certificate",
            certificate_path.display()
        ));
        return Ok(None);
    }

    let key_file = std::fs::File::open(key_path).map_err(|error| {
        anyhow::anyhow!("cannot open the key file {}: {error}", key_path.display())
    })?;
    let mut reader = std::io::BufReader::new(key_file);
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| anyhow::anyhow!("the key file {} holds no key", key_path.display()))?;

    let config = rustls::ServerConfig::builder_with_provider(Arc::clone(provider))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certificates, key)?;
    Ok(Some(config))
}

/// Adds a user.
///
/// The password is read from the terminal or from a file; it is never taken
/// from the command line, where everybody could read it.
async fn add_user(storage: &Backend, cli: &Cli) -> anyhow::Result<()> {
    println!("Add a new user:");
    let username = match cli.user.clone() {
        Some(username) => username,
        None => {
            print!("Username: ");
            std::io::stdout().flush().ok();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            line.trim().to_owned()
        }
    };
    if username.is_empty() {
        anyhow::bail!("the user name is empty");
    }

    let password = match read_password(cli)? {
        Some(password) => password,
        None => {
            let first = read_secret("Password: ")?;
            let second = read_secret("Repeat Password: ")?;
            if first != second {
                anyhow::bail!("the passwords do not match");
            }
            first
        }
    };
    if password.is_empty() {
        anyhow::bail!("the password is empty");
    }

    match storage.add_user(&username, &password).await {
        Ok(_) => {
            println!("Added user {username} successfully!");
            Ok(())
        }
        Err(error) => anyhow::bail!(
            "cannot add the user {username}: {error}. \
             To change the password of a user, use \
             \"quasselcore-rs --change-userpass {username}\""
        ),
    }
}

/// Changes the password of a user.
async fn change_userpass(storage: &Backend, username: &str, cli: &Cli) -> anyhow::Result<()> {
    let users = storage.users().await?;
    if !users.iter().any(|user| user.username == username) {
        anyhow::bail!("there is no user called {username}");
    }
    let password = match read_password(cli)? {
        Some(password) => password,
        None => {
            let first = read_secret("Password: ")?;
            let second = read_secret("Repeat Password: ")?;
            if first != second {
                anyhow::bail!("the passwords do not match");
            }
            first
        }
    };
    // The storage backend does the work; the core does not have to know
    // which database it is talking to.
    storage.set_user_password(username, &password).await?;
    println!("Changed the password of {username} successfully!");
    Ok(())
}

/// The password, when it comes from a file.
fn read_password(cli: &Cli) -> anyhow::Result<Option<String>> {
    let Some(path) = &cli.password_file else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", path.display()))?;
    let password = text.lines().next().unwrap_or_default().to_owned();
    Ok(Some(password))
}

/// Reads a secret from the terminal, without echoing it.
fn read_secret(prompt: &str) -> anyhow::Result<String> {
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let restore = disable_echo();
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    if let Some(restore) = restore {
        restore();
    }
    println!();
    read?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// Turns the echo of the terminal off, and gives back the way to turn it on
/// again.
fn disable_echo() -> Option<impl Fn()> {
    #[cfg(unix)]
    {
        // SAFETY: the termios calls work on the terminal's own descriptor.
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) != 0 {
                return None;
            }
            let saved = termios;
            termios.c_lflag &= !libc::ECHO;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &termios) != 0 {
                return None;
            }
            Some(move || {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
            })
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}

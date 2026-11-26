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
`qs-storage` — the storage backends of the Quassel core.

Two backends are there: __SQLite__ (the default, a file in the config
directory) and __PostgreSQL__.  Both read and migrate the databases the C++
core writes, unchanged: the schema is the one from `src/core/SQL/`, and the
schema version lives in the `coreinfo` table, exactly as the core keeps it.

# The empty codec rule

The `network` table's three codec columns are `NOT NULL`.  The C++ core binds
whatever the client sent, and a client that leaves "use custom encodings"
off sends a _null_ byte array, which Qt turns into a null string, which the
SQLite driver binds as SQL NULL — and the insert fails with

```text
NOT NULL constraint failed: network.encodingcodec
```

This crate never writes SQL NULL into those columns.  A codec that is not
there, or that is empty, is stored as an empty string.

# Passwords

Passwords are hashed with a versioned key derivation function, so that a
database that leaks does not give the passwords away with a single hash.  A
database that still holds the old SHA-1 hashes is recognised and upgraded on
the first successful login.
*/

#![deny(missing_docs)]

mod backend;
mod error;
mod hash;
mod model;
mod postgres;
mod schema;
mod sqlite;

pub use backend::{Dialect, StorageBackend};
pub use error::StorageError;
pub use hash::{check_password, hash_password, HASH_VERSION_SHA1, HASH_VERSION_SHA2_512};
pub use model::{codec_to_text, BufferInfo, Identity, Network, NetworkConfig, User};
pub use postgres::{PostgresDialect, PostgresStorage};
pub use schema::{
    postgres_schema_version, postgres_upgrade_versions, sqlite_schema_version,
    sqlite_upgrade_versions,
};
pub use sqlite::{create_private_file, SqliteDialect, SqliteStorage};

/// What every storage backend can do.
///
/// The trait is what the core talks to.  It has one implementation, on
/// [`Backend`], so that a caller never has to know which database it runs
/// on: the core does not match on the backend to change a password, to read
/// the identities of a user or to create a network.
pub trait Storage {
    /// The name of the backend, as the core knows it (`SQLite`, `PostgreSQL`).
    fn backend_id(&self) -> &'static str;

    /// The schema version this backend knows.
    fn schema_version(&self) -> i64;

    /// The version the database says it has.
    fn installed_schema_version(
        &self,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + Send;

    /// Creates the schema in an empty database.
    fn setup(&self) -> impl std::future::Future<Output = Result<(), StorageError>> + Send;

    /// Brings the database up to the schema version this backend knows.
    fn upgrade(&self) -> impl std::future::Future<Output = Result<(), StorageError>> + Send;

    /// Adds a user with the given password.
    fn add_user(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<User, StorageError>> + Send;

    /// Sets the password of a user.
    fn set_user_password(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + Send;

    /// All the users of the core.
    fn users(&self) -> impl std::future::Future<Output = Result<Vec<User>, StorageError>> + Send;

    /// Checks a password and returns the user when it is right.
    fn validate_user(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<Option<User>, StorageError>> + Send;

    /// The networks of a user.
    fn networks(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<Network>, StorageError>> + Send;

    /// Creates a network for a user and returns its id.
    fn create_network(
        &self,
        userid: i64,
        network: &NetworkConfig,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + Send;

    /// The ids of the networks that are marked as connected.
    fn connected_networks(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<i64>, StorageError>> + Send;

    /// The identities of a user.
    fn identities(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<Identity>, StorageError>> + Send;

    /// The buffers of a user.
    fn buffers(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<BufferInfo>, StorageError>> + Send;

    /// Marks a network as connected or not.
    fn set_network_connected(
        &self,
        networkid: i64,
        connected: bool,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + Send;
}

/// The storage backend the core runs on.
///
/// The core picks one of the two at start-up, so the rest of the core does
/// not have to care which one it is.
#[derive(Clone, Debug)]
pub enum Backend {
    /// The SQLite backend.
    Sqlite(SqliteStorage),
    /// The PostgreSQL backend.
    Postgres(PostgresStorage),
}

impl Backend {
    /// Opens the SQLite backend and makes sure its schema is up to date.
    pub async fn open_sqlite(path: &std::path::Path) -> Result<Self, StorageError> {
        // The database holds the chat history and the password hashes: it is
        // only readable by the user of the core.
        crate::sqlite::create_private_file(path)?;
        Ok(Self::Sqlite(SqliteStorage::open_and_migrate(path).await?))
    }

    /// Opens the PostgreSQL backend and makes sure its schema is up to date.
    pub async fn open_postgres(url: &str) -> Result<Self, StorageError> {
        Ok(Self::Postgres(
            PostgresStorage::open_and_migrate(url).await?,
        ))
    }
}

/// The one implementation of [`Storage`]: the backend the core picked.
///
/// The methods here only say which database the call goes to; the work
/// itself is written once, in [`StorageBackend`].
impl Storage for Backend {
    fn backend_id(&self) -> &'static str {
        match self {
            Self::Sqlite(storage) => storage.backend_id(),
            Self::Postgres(storage) => storage.backend_id(),
        }
    }

    fn schema_version(&self) -> i64 {
        match self {
            Self::Sqlite(storage) => storage.schema_version(),
            Self::Postgres(storage) => storage.schema_version(),
        }
    }

    fn installed_schema_version(
        &self,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.installed_schema_version().await,
                Self::Postgres(storage) => storage.installed_schema_version().await,
            }
        }
    }

    fn setup(&self) -> impl std::future::Future<Output = Result<(), StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.setup().await,
                Self::Postgres(storage) => storage.setup().await,
            }
        }
    }

    fn upgrade(&self) -> impl std::future::Future<Output = Result<(), StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.upgrade().await,
                Self::Postgres(storage) => storage.upgrade().await,
            }
        }
    }

    fn add_user(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<User, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.add_user(username, password).await,
                Self::Postgres(storage) => storage.add_user(username, password).await,
            }
        }
    }

    fn set_user_password(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.set_user_password(username, password).await,
                Self::Postgres(storage) => storage.set_user_password(username, password).await,
            }
        }
    }

    fn users(&self) -> impl std::future::Future<Output = Result<Vec<User>, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.users().await,
                Self::Postgres(storage) => storage.users().await,
            }
        }
    }

    fn validate_user(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<Option<User>, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.validate_user(username, password).await,
                Self::Postgres(storage) => storage.validate_user(username, password).await,
            }
        }
    }

    fn networks(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<Network>, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.networks(userid).await,
                Self::Postgres(storage) => storage.networks(userid).await,
            }
        }
    }

    fn create_network(
        &self,
        userid: i64,
        network: &NetworkConfig,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.create_network(userid, network).await,
                Self::Postgres(storage) => storage.create_network(userid, network).await,
            }
        }
    }

    fn connected_networks(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<i64>, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.connected_networks(userid).await,
                Self::Postgres(storage) => storage.connected_networks(userid).await,
            }
        }
    }

    fn identities(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<Identity>, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.identities(userid).await,
                Self::Postgres(storage) => storage.identities(userid).await,
            }
        }
    }

    fn buffers(
        &self,
        userid: i64,
    ) -> impl std::future::Future<Output = Result<Vec<BufferInfo>, StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.buffers(userid).await,
                Self::Postgres(storage) => storage.buffers(userid).await,
            }
        }
    }

    fn set_network_connected(
        &self,
        networkid: i64,
        connected: bool,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + Send {
        async move {
            match self {
                Self::Sqlite(storage) => storage.set_network_connected(networkid, connected).await,
                Self::Postgres(storage) => {
                    storage.set_network_connected(networkid, connected).await
                }
            }
        }
    }
}

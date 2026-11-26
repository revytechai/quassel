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
The SQLite storage backend.
*/

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::sqlite::{SqliteQueryResult, SqliteRow};
use sqlx::{Sqlite, SqlitePool};

use crate::backend::{Dialect, Query, StorageBackend};
use crate::error::StorageError;

/// The SQLite dialect.
///
/// SQLite writes a bound value as `?`, and it hands back the id of a row it
/// has just written as the id of the last insert.
#[derive(Clone, Copy, Debug)]
pub struct SqliteDialect;

/// The SQLite storage backend.
pub type SqliteStorage = StorageBackend<SqliteDialect>;

/// Creates the database file with private permissions.
///
/// The database holds the chat history and the password hashes: it is only
/// readable by the user of the core.  A database that is already there is
/// left alone.
pub fn create_private_file(path: &Path) -> Result<(), StorageError> {
    if path.exists() {
        return Ok(());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(StorageError::Connect(format!(
            "cannot create the database {}: {error}",
            path.display()
        ))),
    }
}

impl Dialect for SqliteDialect {
    type Database = Sqlite;
    type Source = Path;

    fn backend_id() -> &'static str {
        "SQLite"
    }

    fn schema_version() -> i64 {
        crate::schema::sqlite_schema_version()
    }

    fn connect(
        source: &Path,
    ) -> impl std::future::Future<Output = Result<SqlitePool, StorageError>> + Send {
        async move {
            let options = SqliteConnectOptions::new()
                .filename(source)
                .create_if_missing(true)
                .foreign_keys(true);
            SqlitePoolOptions::new()
                .max_connections(4)
                .connect_with(options)
                .await
                .map_err(|error| StorageError::Connect(error.to_string()))
        }
    }

    fn setup_statements() -> &'static [(&'static str, &'static str)] {
        crate::schema::SQLITE_SETUP
    }

    fn upgrade_statements() -> &'static [(u32, &'static str, &'static str)] {
        crate::schema::SQLITE_UPGRADES
    }

    crate::impl_dialect_transactions!(SqlitePool);

    fn placeholder(_index: usize) -> String {
        "?".to_owned()
    }

    fn query<'q>(sql: &'q str) -> Query<'q, Sqlite> {
        sqlx::query(sql)
    }

    fn fetch_all<'q>(
        query: Query<'q, Sqlite>,
        pool: &SqlitePool,
    ) -> impl std::future::Future<Output = Result<Vec<SqliteRow>, sqlx::Error>> + Send {
        query.fetch_all(pool)
    }

    fn fetch_optional<'q>(
        query: Query<'q, Sqlite>,
        pool: &SqlitePool,
    ) -> impl std::future::Future<Output = Result<Option<SqliteRow>, sqlx::Error>> + Send {
        query.fetch_optional(pool)
    }

    fn execute<'q>(
        query: Query<'q, Sqlite>,
        pool: &SqlitePool,
    ) -> impl std::future::Future<Output = Result<SqliteQueryResult, sqlx::Error>> + Send {
        query.execute(pool)
    }

    fn is_duplicate_key(error: &sqlx::Error) -> bool {
        error.to_string().contains("UNIQUE")
    }

    fn is_missing_table(error: &sqlx::Error) -> bool {
        error.to_string().contains("no such table")
    }

    fn rows_affected(result: &SqliteQueryResult) -> u64 {
        result.rows_affected()
    }

    fn insert_user_sql() -> &'static str {
        "INSERT INTO quasseluser (username, password, hashversion, authenticator) \
         VALUES (?, ?, ?, 'Database')"
    }

    fn insert_network_sql() -> &'static str {
        "INSERT INTO network (userid, networkname, identityid, encodingcodec, \
         decodingcodec, servercodec, useautoreconnect, autoreconnectinterval, \
         autoreconnectretries, rejoinchannels, connected, skipcaps) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    }

    fn insert_user<'q>(
        query: Query<'q, Sqlite>,
        pool: &SqlitePool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let result = query.execute(pool).await?;
            Ok(result.last_insert_rowid())
        }
    }

    fn insert_network<'q>(
        query: Query<'q, Sqlite>,
        pool: &SqlitePool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let result = query.execute(pool).await?;
            Ok(result.last_insert_rowid())
        }
    }
}

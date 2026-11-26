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
The PostgreSQL storage backend.
*/

use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::postgres::{PgQueryResult, PgRow};
use sqlx::{PgPool, Postgres, Row};

use crate::backend::{Dialect, Query, StorageBackend};
use crate::error::StorageError;

/// The PostgreSQL dialect.
///
/// PostgreSQL writes a bound value as `$1`, `$2` and so on, and it can hand
/// back the id of a row it has just written.
#[derive(Clone, Copy, Debug)]
pub struct PostgresDialect;

/// The PostgreSQL storage backend.
pub type PostgresStorage = StorageBackend<PostgresDialect>;

impl Dialect for PostgresDialect {
    type Database = Postgres;
    type Source = str;

    fn backend_id() -> &'static str {
        "PostgreSQL"
    }

    fn schema_version() -> i64 {
        crate::schema::postgres_schema_version()
    }

    fn connect(
        source: &str,
    ) -> impl std::future::Future<Output = Result<PgPool, StorageError>> + Send {
        async move {
            let mut options: PgConnectOptions = source
                .parse()
                .map_err(|error: sqlx::Error| StorageError::Connect(error.to_string()))?;
            // sqlx falls back to a plaintext connection when the server does
            // not offer TLS.  The core never talks to its database without
            // encryption, so the fallback is turned off.  An operator who
            // wants the certificate of the server checked writes
            // `sslmode=verify-full` and `sslrootcert=...` into the URL.
            if matches!(options.get_ssl_mode(), PgSslMode::Prefer) {
                options = options.ssl_mode(PgSslMode::Require);
            }
            PgPoolOptions::new()
                .max_connections(4)
                .connect_with(options)
                .await
                .map_err(|error| StorageError::Connect(error.to_string()))
        }
    }

    fn setup_statements() -> &'static [(&'static str, &'static str)] {
        crate::schema::POSTGRES_SETUP
    }

    fn upgrade_statements() -> &'static [(u32, &'static str, &'static str)] {
        crate::schema::POSTGRES_UPGRADES
    }

    /// The statements that keep a second core from migrating the database at
    /// the same time.
    ///
    /// The lock is an advisory lock of PostgreSQL: it is held by the
    /// connection that changes the schema, and it is free again when that
    /// connection gives it back.
    fn migration_lock_statements() -> &'static [&'static str] {
        &["SELECT pg_advisory_lock(1903228275)"]
    }

    /// The statements that release the migration lock again.
    fn migration_unlock_statements() -> &'static [&'static str] {
        &["SELECT pg_advisory_unlock(1903228275)"]
    }

    crate::impl_dialect_transactions!(PgPool);

    fn placeholder(index: usize) -> String {
        format!("${index}")
    }

    fn query<'q>(sql: &'q str) -> Query<'q, Postgres> {
        sqlx::query(sql)
    }

    fn fetch_all<'q>(
        query: Query<'q, Postgres>,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Vec<PgRow>, sqlx::Error>> + Send {
        query.fetch_all(pool)
    }

    fn fetch_optional<'q>(
        query: Query<'q, Postgres>,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<Option<PgRow>, sqlx::Error>> + Send {
        query.fetch_optional(pool)
    }

    fn execute<'q>(
        query: Query<'q, Postgres>,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<PgQueryResult, sqlx::Error>> + Send {
        query.execute(pool)
    }

    fn is_duplicate_key(error: &sqlx::Error) -> bool {
        error.to_string().contains("duplicate key")
    }

    fn is_missing_table(error: &sqlx::Error) -> bool {
        // 42P01 is `undefined_table`.
        error
            .as_database_error()
            .and_then(|error| error.code())
            .map(|code| code == "42P01")
            .unwrap_or(false)
    }

    fn rows_affected(result: &PgQueryResult) -> u64 {
        result.rows_affected()
    }

    fn insert_user_sql() -> &'static str {
        "INSERT INTO quasseluser (username, password, hashversion, authenticator) \
         VALUES (?, ?, ?, 'Database') RETURNING CAST(userid AS BIGINT)"
    }

    fn insert_network_sql() -> &'static str {
        "INSERT INTO network (userid, networkname, identityid, encodingcodec, \
         decodingcodec, servercodec, useautoreconnect, autoreconnectinterval, \
         autoreconnectretries, rejoinchannels, connected, skipcaps) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING CAST(networkid AS BIGINT)"
    }

    fn insert_user<'q>(
        query: Query<'q, Postgres>,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let row = query.fetch_one(pool).await?;
            row.try_get(0)
        }
    }

    fn insert_network<'q>(
        query: Query<'q, Postgres>,
        pool: &PgPool,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send {
        async move {
            let row = query.fetch_one(pool).await?;
            row.try_get(0)
        }
    }
}

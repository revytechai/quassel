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
The body of the storage backends.

The core keeps its state in SQLite or in PostgreSQL.  Both databases hold the
same schema and the core asks the same questions of both, so the answers are
written once here, in `StorageBackend`.  What the two databases do _not_ have
in common -- the placeholder they write, the way a database is opened, the
way the id of a row that was just inserted is learned -- is what a `Dialect`
says.

The queries are written once, with `?` as the placeholder, and each dialect
rewrites them into its own.
*/

use sqlx::{Database, Pool, Row};

use crate::error::StorageError;
use crate::hash::{check_password, hash_password};
use crate::model::{codec_to_text, BufferInfo, Identity, Network, NetworkConfig, User};

/// The query the storage layer runs.
///
/// sqlx writes its query type for the arguments of one database, so the
/// storage layer names the query it runs in its own words.
pub type Query<'q, D> = sqlx::query::Query<'q, D, <D as Database>::Arguments<'q>>;

/// The SQL a storage backend speaks.
///
/// A dialect is the small difference between two databases that hold the same
/// data: how a query is written, and how a row that was just written is read
/// back.  Everything else the storage layer does is the same for both.
pub trait Dialect: Send + Sync + 'static {
    /// The database this dialect talks to.
    type Database: Database;

    /// How a database of this kind is named: a URL for PostgreSQL, a path for
    /// SQLite.
    type Source: ?Sized;

    /// The name of the backend, as the core knows it.
    fn backend_id() -> &'static str;

    /// The version of the schema this backend knows.
    fn schema_version() -> i64;

    /// Opens a pool of connections to the database.
    fn connect(
        source: &Self::Source,
    ) -> impl std::future::Future<Output = Result<Pool<Self::Database>, StorageError>> + Send;

    /// The statements that create the schema in an empty database.
    fn setup_statements() -> &'static [(&'static str, &'static str)];

    /// The steps that bring an older schema up to date: (version, name, SQL).
    fn upgrade_statements() -> &'static [(u32, &'static str, &'static str)];

    /// The statements that lock the database while the schema is changed.
    ///
    /// An empty list means that the database has no lock of its own.
    fn migration_lock_statements() -> &'static [&'static str] {
        &[]
    }

    /// The statements that unlock the database again.
    fn migration_unlock_statements() -> &'static [&'static str] {
        &[]
    }

    /// Creates the schema in one transaction.
    ///
    /// The whole setup is one transaction: a setup that fails halfway
    /// leaves no tables behind, so that the next start can try again.
    fn setup_transaction<'a>(
        pool: &'a Pool<Self::Database>,
        statements: &'a [(&'a str, &'a str)],
        version: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), StorageError>> + Send + 'a>>;

    /// Brings the database up to date, one version at a time.
    ///
    /// Every version is one transaction, together with the write of the new
    /// schema version.  A version that fails halfway is rolled back, and the
    /// next start tries that version again with the schema it had before.
    fn upgrade_transaction<'a>(
        pool: &'a Pool<Self::Database>,
        statements: &'a [(u32, &'a str, &'a str)],
        installed: i64,
        supported: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), StorageError>> + Send + 'a>>;

    /// The placeholder the `index`-th bound value is written as, counting
    /// from one.
    fn placeholder(index: usize) -> String;

    /// The query the storage layer runs.
    fn query<'q>(sql: &'q str) -> Query<'q, Self::Database>;

    /// Runs a query that returns rows.
    fn fetch_all<'q>(
        query: Query<'q, Self::Database>,
        pool: &Pool<Self::Database>,
    ) -> impl std::future::Future<Output = Result<Vec<<Self::Database as Database>::Row>, sqlx::Error>>
           + Send;

    /// Runs a query that returns at most one row.
    fn fetch_optional<'q>(
        query: Query<'q, Self::Database>,
        pool: &Pool<Self::Database>,
    ) -> impl std::future::Future<
        Output = Result<Option<<Self::Database as Database>::Row>, sqlx::Error>,
    > + Send;

    /// Runs a query that changes rows.
    fn execute<'q>(
        query: Query<'q, Self::Database>,
        pool: &Pool<Self::Database>,
    ) -> impl std::future::Future<
        Output = Result<<Self::Database as Database>::QueryResult, sqlx::Error>,
    > + Send;

    /// Whether the database refused an insert because the row was there
    /// already.
    fn is_duplicate_key(error: &sqlx::Error) -> bool;

    /// Whether the database said that a table is not there.
    ///
    /// Only this makes a database an empty one; every other error is a
    /// broken database and has to be reported as one.
    fn is_missing_table(error: &sqlx::Error) -> bool;

    /// The number of rows a statement changed.
    fn rows_affected(result: &<Self::Database as Database>::QueryResult) -> u64;

    /// The SQL of the statement that inserts a user.
    fn insert_user_sql() -> &'static str;

    /// The SQL of the statement that inserts a network.
    fn insert_network_sql() -> &'static str;

    /// Inserts a user and returns the id it got.
    fn insert_user<'q>(
        query: Query<'q, Self::Database>,
        pool: &Pool<Self::Database>,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send;

    /// Inserts a network and returns the id it got.
    fn insert_network<'q>(
        query: Query<'q, Self::Database>,
        pool: &Pool<Self::Database>,
    ) -> impl std::future::Future<Output = Result<i64, sqlx::Error>> + Send;

    /// Rewrites the `?` placeholders of a query into the dialect's own.
    fn rewrite(sql: &str) -> String {
        let mut out = String::with_capacity(sql.len());
        let mut index = 0;
        for character in sql.chars() {
            if character == '?' {
                index += 1;
                out.push_str(&Self::placeholder(index));
            } else {
                out.push(character);
            }
        }
        out
    }
}

/// Writes the transaction handling of a dialect.
///
/// The code is written once, here, and the macro puts it into the dialect.
/// A function that is generic over the dialect cannot hold on to a connection
/// whose type it only knows when the program is compiled, and the compiler
/// then cannot promise that the work finishes in the background.
#[macro_export]
macro_rules! impl_dialect_transactions {
    ($pool:ty) => {
        /// Creates the schema in one transaction.
        ///
        /// The whole setup is one transaction: a setup that fails halfway
        /// leaves no tables behind, so that the next start can try again.
        fn setup_transaction<'a>(
            pool: &'a $pool,
            statements: &'a [(&'a str, &'a str)],
            version: i64,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<(), $crate::error::StorageError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let mut transaction = pool.begin().await.map_err(|error| {
                    $crate::error::StorageError::query("begin the setup", error)
                })?;
                for (name, statement) in statements {
                    sqlx::Executor::execute(&mut *transaction, sqlx::raw_sql(statement))
                        .await
                        .map_err(|error| $crate::error::StorageError::query(*name, error))?;
                }
                let sql = format!(
                    "INSERT INTO coreinfo (key, value) VALUES ('schemaversion', '{version}')"
                );
                sqlx::Executor::execute(&mut *transaction, sqlx::raw_sql(&sql))
                    .await
                    .map_err(|error| {
                        $crate::error::StorageError::query("set schemaversion", error)
                    })?;
                transaction
                    .commit()
                    .await
                    .map_err(|error| $crate::error::StorageError::query("commit the setup", error))
            })
        }

        /// Brings the database up to date, one version at a time.
        ///
        /// Every version is one transaction, together with the write of the
        /// new schema version.  A version that fails halfway is rolled back,
        /// and the next start tries that version again with the schema it had
        /// before.
        fn upgrade_transaction<'a>(
            pool: &'a $pool,
            statements: &'a [(u32, &'a str, &'a str)],
            installed: i64,
            supported: i64,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<(), $crate::error::StorageError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                // The lock is held by one connection while the versions are
                // changed by the others.
                let mut lock = pool.acquire().await.map_err(|error| {
                    $crate::error::StorageError::query("get a connection", error)
                })?;
                for statement in Self::migration_lock_statements() {
                    sqlx::Executor::execute(&mut *lock, sqlx::raw_sql(statement))
                        .await
                        .map_err(|error| {
                            $crate::error::StorageError::query("lock the database", error)
                        })?;
                }
                for version in (installed + 1)..=supported {
                    let mut transaction = pool.begin().await.map_err(|error| {
                        $crate::error::StorageError::query("begin the migration", error)
                    })?;
                    for (step_version, name, statement) in statements {
                        if *step_version != version as u32 {
                            continue;
                        }
                        sqlx::Executor::execute(&mut *transaction, sqlx::raw_sql(statement))
                            .await
                            .map_err(|error| $crate::error::StorageError::query(*name, error))?;
                    }
                    let sql = format!(
                        "UPDATE coreinfo SET value = '{version}' WHERE key = 'schemaversion'"
                    );
                    sqlx::Executor::execute(&mut *transaction, sqlx::raw_sql(&sql))
                        .await
                        .map_err(|error| {
                            $crate::error::StorageError::query("set schemaversion", error)
                        })?;
                    transaction.commit().await.map_err(|error| {
                        $crate::error::StorageError::query("commit the migration", error)
                    })?;
                }
                for statement in Self::migration_unlock_statements() {
                    let _ = sqlx::Executor::execute(&mut *lock, sqlx::raw_sql(statement)).await;
                }
                Ok(())
            })
        }
    };
}

/// What to do with a database whose schema is not the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Migration {
    /// The database is empty: the schema has to be created.
    Setup,
    /// The database is behind: the schema has to be upgraded.
    Upgrade,
    /// The database is up to date.
    None,
}

/// Decides what to do with a database at schema version `installed`.
pub fn migration(installed: i64, supported: i64) -> Result<Migration, StorageError> {
    if installed == -1 {
        Ok(Migration::Setup)
    } else if installed > supported {
        Err(StorageError::SchemaTooNew {
            installed,
            supported,
        })
    } else if installed < supported {
        Ok(Migration::Upgrade)
    } else {
        Ok(Migration::None)
    }
}

/// A storage backend that talks the SQL of the dialect `D`.
///
/// This is the whole storage layer: the schema, the queries and the way the
/// rows they return are read.  The dialect says what the two databases do
/// differently.
pub struct StorageBackend<D: Dialect> {
    pool: Pool<D::Database>,
}

impl<D: Dialect> Clone for StorageBackend<D> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

impl<D: Dialect> std::fmt::Debug for StorageBackend<D> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageBackend")
            .field("pool", &self.pool)
            .finish()
    }
}

/// The storage layer, written once for every database.
///
/// sqlx does not write its API for a database that is only known when the
/// code is compiled, so the bounds below say in its own words what the
/// storage layer needs from the database it talks to: the types it reads,
/// the types it binds and the connection it runs the query on.
impl<D> StorageBackend<D>
where
    D: Dialect,
    for<'r> i64: sqlx::Decode<'r, D::Database> + sqlx::Type<D::Database>,
    for<'r> String: sqlx::Decode<'r, D::Database> + sqlx::Type<D::Database>,
    for<'r> bool: sqlx::Decode<'r, D::Database> + sqlx::Type<D::Database>,
    for<'q> i64: sqlx::Encode<'q, D::Database> + sqlx::Type<D::Database>,
    for<'q> String: sqlx::Encode<'q, D::Database> + sqlx::Type<D::Database>,
    for<'q> bool: sqlx::Encode<'q, D::Database> + sqlx::Type<D::Database>,
    for<'q> Option<String>: sqlx::Encode<'q, D::Database> + sqlx::Type<D::Database>,
    for<'q> &'q str: sqlx::Encode<'q, D::Database> + sqlx::Type<D::Database>,
    usize: sqlx::ColumnIndex<<D::Database as sqlx::Database>::Row>,
    for<'c> &'c mut <D::Database as sqlx::Database>::Connection:
        sqlx::Executor<'c, Database = D::Database>,
{
    /// Opens the database.
    pub async fn open(source: &D::Source) -> Result<Self, StorageError> {
        let pool = D::connect(source).await?;
        Ok(Self { pool })
    }

    /// Opens the database and makes sure its schema is up to date.
    ///
    /// A database that is already at the right version is left alone.
    pub async fn open_and_migrate(source: &D::Source) -> Result<Self, StorageError> {
        let storage = Self::open(source).await?;
        let installed = storage.installed_schema_version().await?;
        match migration(installed, storage.schema_version())? {
            Migration::Setup => storage.setup().await?,
            Migration::Upgrade => storage.upgrade().await?,
            Migration::None => {}
        }
        Ok(storage)
    }

    /// The connection pool, for callers that need to run their own queries.
    pub fn pool(&self) -> &Pool<D::Database> {
        &self.pool
    }

    /// The name of the backend, as the core knows it.
    pub fn backend_id(&self) -> &'static str {
        D::backend_id()
    }

    /// The version of the schema this backend knows.
    pub fn schema_version(&self) -> i64 {
        D::schema_version()
    }

    /// The version the database says it has.
    ///
    /// Only a database without the `coreinfo` table is an empty database.
    /// Every other error — a permission that is missing, a table with the
    /// wrong columns, a value that is not a number — is reported, so that a
    /// broken database is not mistaken for a new one.
    pub async fn installed_schema_version(&self) -> Result<i64, StorageError> {
        let row = D::fetch_optional(
            D::query("SELECT value FROM coreinfo WHERE key = 'schemaversion'"),
            &self.pool,
        )
        .await;
        match row {
            Ok(Some(row)) => {
                let value: String = row
                    .try_get(0usize)
                    .map_err(|error| StorageError::query("select schemaversion", error))?;
                value.parse().map_err(|_| {
                    StorageError::Schema(format!(
                        "the database says its schema version is \"{value}\", which is not a number"
                    ))
                })
            }
            // The coreinfo table does not exist yet: the schema was never set
            // up, so the database needs the setup.
            Ok(None) => Ok(-1),
            Err(error) if D::is_missing_table(&error) => Ok(-1),
            Err(error) => Err(StorageError::query("select schemaversion", error)),
        }
    }

    /// Creates the schema in an empty database.
    ///
    /// The whole setup runs in one transaction.  A setup that fails halfway
    /// leaves no tables behind, so that the next start can try again.
    pub async fn setup(&self) -> Result<(), StorageError> {
        D::setup_transaction(&self.pool, D::setup_statements(), self.schema_version()).await
    }

    /// Brings the database up to the schema version this backend knows.
    ///
    /// Every version is changed in its own transaction, together with the
    /// write of the new schema version.  A version that fails halfway is
    /// therefore rolled back, and the next start tries that version again
    /// with the schema it had before.
    pub async fn upgrade(&self) -> Result<(), StorageError> {
        let installed = self.installed_schema_version().await?;
        if installed >= self.schema_version() {
            return Ok(());
        }
        D::upgrade_transaction(
            &self.pool,
            D::upgrade_statements(),
            installed,
            self.schema_version(),
        )
        .await
    }

    /// Adds a user with the given password.
    pub async fn add_user(&self, username: &str, password: &str) -> Result<User, StorageError> {
        let (hashed, hash_version) = hash_password(password)?;
        let sql = D::rewrite(D::insert_user_sql());
        let query = D::query(&sql)
            .bind(username)
            .bind(&hashed)
            .bind(hash_version);
        let userid = D::insert_user(query, &self.pool).await.map_err(|error| {
            if D::is_duplicate_key(&error) {
                StorageError::UserExists(username.to_owned())
            } else {
                StorageError::query("insert quasseluser", error)
            }
        })?;
        Ok(User {
            userid,
            username: username.to_owned(),
            authenticator: "Database".to_owned(),
        })
    }

    /// Sets the password of a user.
    pub async fn set_user_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(), StorageError> {
        let (hashed, hash_version) = hash_password(password)?;
        let sql =
            D::rewrite("UPDATE quasseluser SET password = ?, hashversion = ? WHERE username = ?");
        let query = D::query(&sql)
            .bind(&hashed)
            .bind(hash_version)
            .bind(username);
        let result = D::execute(query, &self.pool)
            .await
            .map_err(|error| StorageError::query("update quasseluser", error))?;
        if D::rows_affected(&result) == 0 {
            return Err(StorageError::NoSuchUser(username.to_owned()));
        }
        Ok(())
    }

    /// All the users of the core.
    pub async fn users(&self) -> Result<Vec<User>, StorageError> {
        let rows = D::fetch_all(
            D::query("SELECT userid, username, authenticator FROM quasseluser"),
            &self.pool,
        )
        .await
        .map_err(|error| StorageError::query("select quasseluser", error))?;
        Ok(rows
            .into_iter()
            .map(|row| User {
                userid: row.get(0usize),
                username: row.get(1usize),
                authenticator: row.get(2usize),
            })
            .collect())
    }

    /// Checks a password and returns the user when it is right.
    pub async fn validate_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<User>, StorageError> {
        let sql = D::rewrite(
            "SELECT userid, username, password, hashversion, authenticator \
             FROM quasseluser WHERE username = ?",
        );
        let row = D::fetch_optional(D::query(&sql).bind(username), &self.pool)
            .await
            .map_err(|error| StorageError::query("select quasseluser", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let hashed: String = row
            .try_get(2usize)
            .map_err(|error| StorageError::query("select quasseluser", error))?;
        let hash_version: i64 = row
            .try_get(3usize)
            .map_err(|error| StorageError::query("select quasseluser", error))?;
        if !check_password(password, &hashed, hash_version) {
            return Ok(None);
        }
        // The core upgrades an old hash on the first successful login.
        if hash_version < crate::hash::HASH_VERSION_CURRENT {
            self.set_user_password(username, password).await?;
        }
        Ok(Some(User {
            userid: row.get(0usize),
            username: row.get(1usize),
            authenticator: row.get(2usize),
        }))
    }

    /// The networks of a user.
    ///
    /// The columns that may be empty in a database the C++ core wrote are
    /// read as their empty value: an identity that was deleted leaves a null
    /// behind, and a codec that was never set is a null as well.
    pub async fn networks(&self, userid: i64) -> Result<Vec<Network>, StorageError> {
        let sql = D::rewrite(
            "SELECT networkid, userid, networkname, COALESCE(identityid, 0), \
             COALESCE(encodingcodec, ''), COALESCE(decodingcodec, ''), \
             COALESCE(servercodec, ''), connected, useautoreconnect, \
             autoreconnectinterval, autoreconnectretries, rejoinchannels, skipcaps \
             FROM network WHERE userid = ? ORDER BY networkid",
        );
        let rows = D::fetch_all(D::query(&sql).bind(userid), &self.pool)
            .await
            .map_err(|error| StorageError::query("select network", error))?;
        Ok(rows
            .into_iter()
            .map(|row| Network {
                networkid: row.get(0usize),
                userid: row.get(1usize),
                networkname: row.get(2usize),
                identityid: row.get(3usize),
                encodingcodec: row.get(4usize),
                decodingcodec: row.get(5usize),
                servercodec: row.get(6usize),
                connected: row.get(7usize),
                useautoreconnect: row.get(8usize),
                autoreconnectinterval: row.get(9usize),
                autoreconnectretries: row.get(10usize),
                rejoinchannels: row.get(11usize),
                skipcaps: row.get(12usize),
            })
            .collect())
    }

    /// Creates a network for a user and returns its id.
    pub async fn create_network(
        &self,
        userid: i64,
        network: &NetworkConfig,
    ) -> Result<i64, StorageError> {
        // The codec columns are NOT NULL.  A codec that the client left empty
        // is stored as an empty string, never as SQL NULL.
        let sql = D::rewrite(D::insert_network_sql());
        let query = D::query(&sql)
            .bind(userid)
            .bind(&network.networkname)
            .bind(network.identityid)
            .bind(codec_to_text(network.encodingcodec.as_deref()))
            .bind(codec_to_text(network.decodingcodec.as_deref()))
            .bind(codec_to_text(network.servercodec.as_deref()))
            .bind(network.useautoreconnect)
            .bind(network.autoreconnectinterval)
            .bind(network.autoreconnectretries)
            .bind(network.rejoinchannels)
            .bind(false)
            .bind(network.skipcaps.clone());
        D::insert_network(query, &self.pool)
            .await
            .map_err(|error| StorageError::query("insert network", error))
    }

    /// The ids of the networks that are marked as connected.
    pub async fn connected_networks(&self, userid: i64) -> Result<Vec<i64>, StorageError> {
        let sql = D::rewrite("SELECT networkid FROM network WHERE userid = ? AND connected = ?");
        let rows = D::fetch_all(D::query(&sql).bind(userid).bind(true), &self.pool)
            .await
            .map_err(|error| StorageError::query("select connected networks", error))?;
        Ok(rows.into_iter().map(|row| row.get(0usize)).collect())
    }

    /// The identities of a user, with their nicknames.
    pub async fn identities(&self, userid: i64) -> Result<Vec<Identity>, StorageError> {
        let sql = D::rewrite(
            "SELECT i.identityid, i.userid, i.identityname, i.realname, i.awaynick, \
             i.awaynickenabled, i.awayreason, i.awayreasonenabled, i.autoawayenabled, \
             i.autoawaytime, i.autoawayreason, i.autoawayreasonenabled, i.detachawayenabled, \
             i.detachawayreason, i.detachawayreasonenabled, i.ident, i.kickreason, \
             i.partreason, i.quitreason, COALESCE(n.nick, '') \
             FROM identity i LEFT JOIN identity_nick n ON n.identityid = i.identityid \
             WHERE i.userid = ? ORDER BY i.identityid, n.nickid",
        );
        let rows = D::fetch_all(D::query(&sql).bind(userid), &self.pool)
            .await
            .map_err(|error| StorageError::query("select identity", error))?;
        let mut identities: Vec<Identity> = Vec::new();
        for row in rows {
            let identityid: i64 = row.get(0usize);
            let nick: String = row.get(19usize);
            match identities.last_mut() {
                Some(identity) if identity.identityid == identityid => {
                    if !nick.is_empty() {
                        identity.nicks.push(nick);
                    }
                }
                _ => {
                    let mut nicks = Vec::new();
                    if !nick.is_empty() {
                        nicks.push(nick);
                    }
                    identities.push(Identity {
                        identityid,
                        userid: row.get(1usize),
                        identityname: row.get(2usize),
                        realname: row.get(3usize),
                        awaynick: row.get(4usize),
                        awaynickenabled: row.get(5usize),
                        awayreason: row.get(6usize),
                        awayreasonenabled: row.get(7usize),
                        autoawayenabled: row.get(8usize),
                        autoawaytime: row.get(9usize),
                        autoawayreason: row.get(10usize),
                        autoawayreasonenabled: row.get(11usize),
                        detachawayenabled: row.get(12usize),
                        detachawayreason: row.get(13usize),
                        detachawayreasonenabled: row.get(14usize),
                        ident: row.get(15usize),
                        kickreason: row.get(16usize),
                        partreason: row.get(17usize),
                        quitreason: row.get(18usize),
                        nicks,
                    })
                }
            }
        }
        Ok(identities)
    }

    /// The buffers of a user.
    pub async fn buffers(&self, userid: i64) -> Result<Vec<BufferInfo>, StorageError> {
        let sql = D::rewrite(
            "SELECT bufferid, userid, networkid, buffername, buffertype, groupid \
             FROM buffer WHERE userid = ? ORDER BY bufferid",
        );
        let rows = D::fetch_all(D::query(&sql).bind(userid), &self.pool)
            .await
            .map_err(|error| StorageError::query("select buffer", error))?;
        Ok(rows
            .into_iter()
            .map(|row| BufferInfo {
                bufferid: row.get(0usize),
                userid: row.get(1usize),
                networkid: row.get(2usize),
                buffername: row.get(3usize),
                buffertype: row.get(4usize),
                groupid: row.get(5usize),
            })
            .collect())
    }

    /// Marks a network as connected or not.
    pub async fn set_network_connected(
        &self,
        networkid: i64,
        connected: bool,
    ) -> Result<(), StorageError> {
        let sql = D::rewrite("UPDATE network SET connected = ? WHERE networkid = ?");
        D::execute(D::query(&sql).bind(connected).bind(networkid), &self.pool)
            .await
            .map_err(|error| StorageError::query("update network", error))?;
        Ok(())
    }
}

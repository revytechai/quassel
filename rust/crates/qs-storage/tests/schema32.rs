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

//! Tests for the storage layer.
//!
//! The database these tests work on is created with the DDL of the C++ core
//! (`src/core/SQL/SQLite`), so what is tested here is the schema the real
//! core writes.

use std::path::{Path, PathBuf};

use qs_storage::{NetworkConfig, SqliteStorage};
use sqlx::Row;

/// A directory that goes away when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("qs-storage-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("creating the test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Creates a database with the core's own DDL.
async fn schema32(path: &Path) -> SqliteStorage {
    let storage = SqliteStorage::open(path)
        .await
        .expect("opening the database");
    storage.setup().await.expect("setting the schema up");
    storage
}

#[tokio::test]
async fn a_fresh_database_is_at_schema_version_32() {
    let dir = TempDir::new("fresh");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;

    assert_eq!(storage.schema_version(), 32);
    assert_eq!(storage.installed_schema_version().await.unwrap(), 32);
}

#[tokio::test]
async fn a_fresh_database_has_the_tables_of_the_core() {
    let dir = TempDir::new("tables");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;

    let rows = sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .fetch_all(storage.pool())
        .await
        .unwrap();
    let mut names: Vec<String> = rows.iter().map(|row| row.get::<String, _>(0)).collect();
    names.retain(|name| !name.starts_with("sqlite_"));
    for table in [
        "buffer",
        "core_state",
        "coreinfo",
        "identity",
        "ircserver",
        "network",
        "quasseluser",
        "sender",
        "user_setting",
    ] {
        assert!(
            names.contains(&table.to_owned()),
            "the table {table} is missing"
        );
    }
}

#[tokio::test]
async fn the_network_table_has_the_version_32_columns() {
    let dir = TempDir::new("columns");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;

    let rows = sqlx::query("SELECT name FROM pragma_table_info('network')")
        .fetch_all(storage.pool())
        .await
        .unwrap();
    let names: Vec<String> = rows.iter().map(|row| row.get::<String, _>(0)).collect();
    for column in [
        "networkid",
        "networkname",
        "encodingcodec",
        "decodingcodec",
        "servercodec",
        "connected",
        "useautoreconnect",
        "rejoinchannels",
        "skipcaps",
    ] {
        assert!(
            names.contains(&column.to_owned()),
            "the column {column} is missing"
        );
    }
}

#[tokio::test]
async fn an_existing_database_is_read_unchanged() {
    let dir = TempDir::new("unchanged");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;
    let user = storage.add_user("alice", "secret").await.unwrap();
    storage
        .create_network(
            user.userid,
            &NetworkConfig {
                networkname: "EFnet".to_owned(),
                encodingcodec: Some(b"UTF-8".to_vec()),
                decodingcodec: Some(b"UTF-8".to_vec()),
                servercodec: Some(b"UTF-8".to_vec()),
                ..NetworkConfig::default()
            },
        )
        .await
        .unwrap();
    storage.pool().close().await;

    let before = std::fs::read(&path).unwrap();

    // Opening the database again must not change it: the schema is already
    // at the newest version, so nothing is written.
    let storage = SqliteStorage::open_and_migrate(&path).await.unwrap();
    assert_eq!(storage.installed_schema_version().await.unwrap(), 32);
    let user = storage
        .validate_user("alice", "secret")
        .await
        .unwrap()
        .unwrap();
    let networks = storage.networks(user.userid).await.unwrap();
    assert_eq!(networks.len(), 1);
    assert_eq!(networks[0].networkname, "EFnet");
    assert_eq!(networks[0].encodingcodec, "UTF-8");
    storage.pool().close().await;

    let after = std::fs::read(&path).unwrap();
    assert_eq!(before, after, "reading the database changed it");
}

#[tokio::test]
async fn a_network_with_empty_codecs_stores_empty_strings_and_never_null() {
    let dir = TempDir::new("codecs");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;
    let user = storage.add_user("alice", "secret").await.unwrap();

    // The client left the codecs empty.  The C++ core binds a null byte
    // array here, which Qt turns into SQL NULL, and the insert fails with
    // "NOT NULL constraint failed: network.encodingcodec".
    let networkid = storage
        .create_network(
            user.userid,
            &NetworkConfig {
                networkname: "Empty".to_owned(),
                encodingcodec: Some(Vec::new()),
                decodingcodec: Some(Vec::new()),
                servercodec: Some(Vec::new()),
                ..NetworkConfig::default()
            },
        )
        .await
        .expect("a network with empty codecs must be storable");
    assert!(networkid > 0);

    // A codec that is not there at all must work as well.
    storage
        .create_network(
            user.userid,
            &NetworkConfig {
                networkname: "Missing".to_owned(),
                encodingcodec: None,
                decodingcodec: None,
                servercodec: None,
                ..NetworkConfig::default()
            },
        )
        .await
        .expect("a network without codecs must be storable");

    let rows = sqlx::query(
        "SELECT networkname, typeof(encodingcodec), encodingcodec, \
         typeof(decodingcodec), decodingcodec, typeof(servercodec), servercodec \
         FROM network ORDER BY networkid",
    )
    .fetch_all(storage.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.get::<String, _>(1), "text", "the codec must be text");
        assert_eq!(row.get::<String, _>(2), "", "the codec must be empty");
        assert_eq!(row.get::<String, _>(3), "text");
        assert_eq!(row.get::<String, _>(4), "");
        assert_eq!(row.get::<String, _>(5), "text");
        assert_eq!(row.get::<String, _>(6), "");
    }
    assert_eq!(rows[0].get::<String, _>(0), "Empty");
    assert_eq!(rows[1].get::<String, _>(0), "Missing");

    // The same through the API.
    let networks = storage.networks(user.userid).await.unwrap();
    assert_eq!(networks.len(), 2);
    for network in networks {
        assert_eq!(network.encodingcodec, "");
        assert_eq!(network.decodingcodec, "");
        assert_eq!(network.servercodec, "");
    }
}

/// Creates a database at the newest schema version, with a user and a
/// network in it.
async fn database_with_a_network(path: &Path) -> qs_storage::User {
    let storage = schema32(path).await;
    let user = storage.add_user("alice", "secret").await.unwrap();
    storage
        .create_network(
            user.userid,
            &NetworkConfig {
                networkname: "EFnet".to_owned(),
                ..NetworkConfig::default()
            },
        )
        .await
        .unwrap();
    storage.pool().close().await;
    user
}

/// Takes a copy of a database and takes it back to schema version 31.
///
/// The database of version 32 has the column `skipcaps`; the database of
/// version 31 does not.
async fn copy_at_version_31(source: &Path, copy: &Path) -> SqliteStorage {
    std::fs::copy(source, copy).unwrap();
    let storage = SqliteStorage::open(copy).await.unwrap();
    sqlx::raw_sql("ALTER TABLE network DROP COLUMN skipcaps")
        .execute(storage.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE coreinfo SET value = '31' WHERE key = 'schemaversion'")
        .execute(storage.pool())
        .await
        .unwrap();
    storage
}

#[tokio::test]
async fn an_old_database_is_migrated() {
    let dir = TempDir::new("migrate");
    let path = dir.path().join("quassel-storage.sqlite");
    let user = database_with_a_network(&path).await;

    // Take a copy of the file, take it back to schema version 31 and let the
    // storage migrate it.
    let copy = dir.path().join("quassel-storage-copy.sqlite");
    let storage = copy_at_version_31(&path, &copy).await;
    storage.pool().close().await;

    let storage = SqliteStorage::open_and_migrate(&copy).await.unwrap();
    assert_eq!(storage.installed_schema_version().await.unwrap(), 32);
    let rows = sqlx::query("SELECT name FROM pragma_table_info('network')")
        .fetch_all(storage.pool())
        .await
        .unwrap();
    let names: Vec<String> = rows.iter().map(|row| row.get::<String, _>(0)).collect();
    assert!(names.contains(&"skipcaps".to_owned()));
    // The data survived the migration.
    let user = storage
        .validate_user("alice", "secret")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(storage.networks(user.userid).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_password_is_hashed_the_way_the_core_hashes_it() {
    let dir = TempDir::new("password");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;

    storage.add_user("alice", "secret").await.unwrap();
    let row = sqlx::query("SELECT password, hashversion FROM quasseluser WHERE username = 'alice'")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    let hashed: String = row.get(0);
    let hash_version: i64 = row.get(1);
    assert_eq!(
        hash_version, 2,
        "the core writes the key derivation version"
    );
    assert!(
        hashed.starts_with("pbkdf2-sha512$"),
        "the hash names the key derivation function: {hashed}"
    );
    assert!(!hashed.contains("secret"));

    assert!(storage
        .validate_user("alice", "secret")
        .await
        .unwrap()
        .is_some());
    assert!(storage
        .validate_user("alice", "wrong")
        .await
        .unwrap()
        .is_none());
    assert!(storage
        .validate_user("bob", "secret")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn the_database_is_only_readable_by_its_owner() {
    let dir = TempDir::new("permissions");
    let path = dir.path().join("quassel-storage.sqlite");
    // The core creates the database itself, with private permissions.
    qs_storage::create_private_file(&path).expect("creating the database");
    let storage = schema32(&path).await;
    storage.add_user("alice", "secret").await.unwrap();
    storage.pool().close().await;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "the database is readable by others");
    }
}

#[tokio::test]
async fn a_user_cannot_be_added_twice() {
    let dir = TempDir::new("twice");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;

    storage.add_user("alice", "secret").await.unwrap();
    assert!(storage.add_user("alice", "other").await.is_err());
}

#[tokio::test]
async fn the_connected_networks_are_the_ones_that_say_so() {
    let dir = TempDir::new("connected");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;
    let user = storage.add_user("alice", "secret").await.unwrap();

    let first = storage
        .create_network(
            user.userid,
            &NetworkConfig {
                networkname: "One".to_owned(),
                ..NetworkConfig::default()
            },
        )
        .await
        .unwrap();
    let second = storage
        .create_network(
            user.userid,
            &NetworkConfig {
                networkname: "Two".to_owned(),
                ..NetworkConfig::default()
            },
        )
        .await
        .unwrap();

    storage.set_network_connected(first, true).await.unwrap();
    assert_eq!(
        storage.connected_networks(user.userid).await.unwrap(),
        vec![first]
    );
    storage.set_network_connected(second, true).await.unwrap();
    assert_eq!(
        storage.connected_networks(user.userid).await.unwrap().len(),
        2
    );
}

#[tokio::test]
async fn a_migration_that_fails_leaves_the_schema_where_it_was() {
    let dir = TempDir::new("atomic");
    let path = dir.path().join("quassel-storage.sqlite");
    let user = database_with_a_network(&path).await;

    // Take a copy of the file and take it back to schema version 31.
    let copy = dir.path().join("quassel-storage-copy.sqlite");
    let storage = copy_at_version_31(&path, &copy).await;
    // A trigger that makes the write of the new schema version fail.
    sqlx::raw_sql(
        "CREATE TRIGGER abort_schemaversion BEFORE UPDATE ON coreinfo \
         BEGIN SELECT RAISE(ABORT, 'no version today'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    storage.pool().close().await;

    // The migration fails ...
    let storage = SqliteStorage::open(&copy).await.unwrap();
    assert!(
        storage.upgrade().await.is_err(),
        "the migration must fail when the version cannot be written"
    );
    // ... and the schema is where it was: the column of version 32 is not
    // there, the version is still 31, and the data is still there.
    let rows = sqlx::query("SELECT name FROM pragma_table_info('network')")
        .fetch_all(storage.pool())
        .await
        .unwrap();
    let names: Vec<String> = rows.iter().map(|row| row.get::<String, _>(0)).collect();
    assert!(
        !names.contains(&"skipcaps".to_owned()),
        "the migration was not rolled back"
    );
    assert_eq!(storage.installed_schema_version().await.unwrap(), 31);
    let rows = sqlx::query("SELECT networkname FROM network WHERE userid = ?")
        .bind(user.userid)
        .fetch_all(storage.pool())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "the data is still there");

    // Without the trigger the migration works and the retry does not fail
    // with "duplicate column name".
    sqlx::raw_sql("DROP TRIGGER abort_schemaversion")
        .execute(storage.pool())
        .await
        .unwrap();
    storage.pool().close().await;
    let storage = SqliteStorage::open(&copy).await.unwrap();
    storage.upgrade().await.expect("the retry must work");
    assert_eq!(storage.installed_schema_version().await.unwrap(), 32);
    assert_eq!(storage.networks(user.userid).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_identities_of_a_user_keep_their_nicks_and_their_state() {
    let dir = TempDir::new("identities");
    let path = dir.path().join("quassel-storage.sqlite");
    let storage = schema32(&path).await;
    let user = storage.add_user("alice", "secret").await.unwrap();
    sqlx::query(
        "INSERT INTO identity (userid, identityname, realname, awaynick, \
         awaynickenabled, autoawayenabled, autoawaytime, ident) \
         VALUES (?, 'Work', 'Alice Work', 'alice_away', 1, 1, 30, 'aliceident')",
    )
    .bind(user.userid)
    .execute(storage.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO identity_nick (identityid, nick) \
         VALUES ((SELECT identityid FROM identity WHERE identityname = 'Work'), 'alice')",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let identities = storage.identities(user.userid).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].identityname, "Work");
    assert_eq!(identities[0].realname, "Alice Work");
    assert_eq!(identities[0].nicks, vec!["alice".to_owned()]);
    assert_eq!(identities[0].awaynick.as_deref(), Some("alice_away"));
    assert!(identities[0].awaynickenabled);
    assert!(identities[0].autoawayenabled);
    assert_eq!(identities[0].autoawaytime, 30);
    assert_eq!(identities[0].ident.as_deref(), Some("aliceident"));
}

#[tokio::test]
async fn a_broken_coreinfo_table_is_reported_and_not_taken_for_an_empty_database() {
    let dir = TempDir::new("broken");
    let path = dir.path().join("quassel-storage.sqlite");
    // A database whose coreinfo table has the wrong columns.
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE coreinfo (key TEXT, wrong TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let storage = SqliteStorage::open(&path).await.unwrap();
    assert!(
        storage.installed_schema_version().await.is_err(),
        "a broken coreinfo table must be reported, not taken for an empty database"
    );
}

#[test]
fn the_schema_versions_are_contiguous() {
    // Every version between the first and the last one has an upgrade step.
    // A version that is missing would make the core write a version the
    // database does not have: the migration would run no statements and the
    // schema would stay behind.
    for versions in [
        qs_storage::sqlite_upgrade_versions(),
        qs_storage::postgres_upgrade_versions(),
    ] {
        assert!(!versions.is_empty(), "there are no upgrade steps at all");
        for (index, version) in versions.iter().enumerate() {
            assert_eq!(
                *version,
                versions[0] + index as u32,
                "the schema versions are not contiguous: {versions:?}"
            );
        }
    }
}

/// The SQL this crate carries must be the SQL the C++ core carries.
#[test]
fn the_sql_matches_the_cores() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("src")
        .join("core")
        .join("SQL");
    if !root.is_dir() {
        // The C++ tree is not there (the crate was copied out of the
        // repository); there is nothing to compare with.
        return;
    }
    for backend in ["SQLite", "PostgreSQL"] {
        let ours = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sql")
            .join(if backend == "SQLite" {
                "sqlite"
            } else {
                "postgresql"
            });
        let theirs = root.join(backend);
        compare_directory(&ours, &theirs, backend);
    }
}

fn compare_directory(ours: &Path, theirs: &Path, backend: &str) {
    let mut ours_files: Vec<(String, PathBuf)> = collect_sql(ours, ours);
    let mut theirs_files: Vec<(String, PathBuf)> = collect_sql(theirs, theirs);
    ours_files.sort();
    theirs_files.sort();
    assert_eq!(
        ours_files.len(),
        theirs_files.len(),
        "the {backend} SQL files differ in number"
    );
    for ((our_name, our_path), (their_name, their_path)) in
        ours_files.iter().zip(theirs_files.iter())
    {
        assert_eq!(
            our_name, their_name,
            "the {backend} SQL files differ in name"
        );
        assert_eq!(
            std::fs::read_to_string(our_path).unwrap(),
            std::fs::read_to_string(their_path).unwrap(),
            "the {backend} SQL file {our_name} differs from the core's"
        );
    }
}

/// Collects the setup statements and the upgrade steps of a backend.
///
/// The upgrade steps live in the version directories, one directory per
/// schema version; they are part of the schema and are compared as well.
///
/// The core's directory holds the named queries (select_*, insert_*, ...) as
/// well; those are the core's own business and are not part of the schema.
fn collect_sql(root: &Path, dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_sql(root, &path));
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".sql") {
            continue;
        }
        if !name.starts_with("setup_") && !name.starts_with("upgrade_") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        out.push((relative, path));
    }
    out
}

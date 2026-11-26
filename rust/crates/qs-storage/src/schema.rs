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
The SQL the schema is made of.

The statements are a copy of the C++ core's `src/core/SQL/`; the build script
lists them.  The core's own rules are followed:

- the _setup_ statements create the schema of a fresh database;
- the _upgrade_ statements live in `version/##` and bring an older database
  up to the newest version, one version at a time.
*/

#![allow(missing_docs)]

include!(concat!(env!("OUT_DIR"), "/schema_sql.rs"));

/// The schema version the SQLite backend knows.
pub fn sqlite_schema_version() -> i64 {
    SQLITE_UPGRADES
        .iter()
        .map(|(version, _, _)| i64::from(*version))
        .max()
        .unwrap_or(0)
}

/// The schema version the PostgreSQL backend knows.
pub fn postgres_schema_version() -> i64 {
    POSTGRES_UPGRADES
        .iter()
        .map(|(version, _, _)| i64::from(*version))
        .max()
        .unwrap_or(0)
}

/// The schema versions of the SQLite backend that have an upgrade step.
pub fn sqlite_upgrade_versions() -> Vec<u32> {
    versions_of(SQLITE_UPGRADES)
}

/// The schema versions of the PostgreSQL backend that have an upgrade step.
pub fn postgres_upgrade_versions() -> Vec<u32> {
    versions_of(POSTGRES_UPGRADES)
}

/// The versions of the upgrade steps, without the duplicates.
fn versions_of(upgrades: &[(u32, &str, &str)]) -> Vec<u32> {
    let mut versions: Vec<u32> = upgrades.iter().map(|(version, _, _)| *version).collect();
    versions.sort_unstable();
    versions.dedup();
    versions
}

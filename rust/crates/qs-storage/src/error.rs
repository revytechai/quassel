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
Errors of the storage layer.
*/

/// Everything that can go wrong while talking to the database.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The database refused a statement.
    #[error("database error while running {statement}: {source}")]
    Query {
        /// The name of the statement.
        statement: String,
        /// What the database said.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The database could not be opened.
    #[error("cannot open the database: {0}")]
    Connect(String),
    /// The schema of the database is newer than this core knows.
    #[error(
        "the installed schema version {installed} is newer than the supported version {supported}"
    )]
    SchemaTooNew {
        /// The version the database has.
        installed: i64,
        /// The version this core supports.
        supported: i64,
    },
    /// The schema of the database is missing.
    #[error("the storage schema is missing")]
    SchemaMissing,
    /// The schema of the database is not what the core expects.
    #[error("the storage schema is broken: {0}")]
    Schema(String),
    /// A user with that name already exists.
    #[error("the user {0} already exists")]
    UserExists(String),
    /// There is no such user.
    #[error("there is no user called {0}")]
    NoSuchUser(String),
    /// The password could not be hashed.
    #[error("cannot read random bytes: {0}")]
    Random(String),
    /// A codec could not be stored.
    #[error("cannot store the codec {0:?}")]
    Codec(String),
}

impl StorageError {
    /// Builds a query error.
    pub(crate) fn query(
        statement: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self::Query {
            statement: statement.into(),
            source: source.into(),
        }
    }
}

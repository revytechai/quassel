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

//! The core's configuration file.
//!
//! The core reads the same file the C++ core reads: `quasselcore.conf` in
//! the configuration directory, in the INI format Qt's `QSettings` writes.
//!
//! ```text
//! [Core]
//! StorageSettings\Backend=SQLite
//! StorageSettings\ConnectionProperties\Database=quassel-storage.sqlite
//! AuthSettings\Authenticator=Database
//!
//! [Core\SSL]
//! Certificate=/var/db/quasselcore/quasselCert.pem
//! Key=/var/db/quasselcore/quasselCert.pem
//! SystemCA=true
//! ```
//!
//! A key of a sub-group is written with a backslash, exactly as Qt writes it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

/// The configuration file of the core.
#[derive(Clone, Debug, Default)]
pub struct ConfigFile {
    entries: BTreeMap<String, String>,
}

impl ConfigFile {
    /// Reads a configuration file.
    ///
    /// A file that is not there is not an error: the core then runs with the
    /// defaults.  Every other error is an error: a configuration that cannot
    /// be read must not be taken for an empty one, or the core writes a new
    /// configuration over the old one.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let mut file = ConfigFile::default();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(file),
            Err(error) => {
                return Err(ConfigError::Read {
                    path: path.to_path_buf(),
                    source: error,
                })
            }
        };
        let mut group = String::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let Some(name) = rest.strip_suffix(']') else {
                    return Err(ConfigError::Syntax {
                        path: path.to_path_buf(),
                        line: number + 1,
                        text: line.to_owned(),
                    });
                };
                group = name.trim().to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ConfigError::Syntax {
                    path: path.to_path_buf(),
                    line: number + 1,
                    text: line.to_owned(),
                });
            };
            let key = key.trim();
            let value = value.trim();
            let full = if group.is_empty() {
                key.to_owned()
            } else {
                format!("{group}\\{key}")
            };
            file.entries.insert(full, value.to_owned());
        }
        Ok(file)
    }

    /// Creates the configuration directory.
    ///
    /// The directory holds the database, the configuration and the
    /// certificate: only the user of the core may read them.
    pub fn create_directory(path: &Path) -> Result<(), ConfigError> {
        let mut options = std::fs::DirBuilder::new();
        options.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            options.mode(0o700);
        }
        options.create(path).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Writes the configuration file.
    ///
    /// The file holds the password of the database and the paths of the
    /// certificates, so it is only readable by the user of the core.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let mut groups: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for (key, value) in &self.entries {
            match key.split_once('\\') {
                Some((group, name)) => groups.entry(group).or_default().push((name, value)),
                None => groups.entry("").or_default().push((key, value)),
            }
        }
        let mut text = String::new();
        for (group, entries) in groups {
            if !group.is_empty() {
                text.push_str(&format!("[{group}]\n"));
            }
            for (key, value) in entries {
                text.push_str(&format!("{key}={value}\n"));
            }
            text.push('\n');
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        file.write_all(text.as_bytes())
            .map_err(|source| ConfigError::Write {
                path: path.to_path_buf(),
                source,
            })
    }

    /// The value of a key.
    pub fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.entries
            .get(&format!("{group}\\{key}"))
            .map(String::as_str)
    }

    /// The value of a key in the `Core` group.
    pub fn core(&self, key: &str) -> Option<&str> {
        self.get("Core", key)
    }

    /// Sets a value.
    pub fn set(&mut self, group: &str, key: &str, value: impl Into<String>) {
        self.entries.insert(format!("{group}\\{key}"), value.into());
    }
}

/// What can go wrong while reading the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file has a line the core does not understand.
    #[error(
        "{path}:{line}: cannot read the configuration file: \
         \"{text}\" is neither a group ([name]) nor a setting (key=value)"
    )]
    Syntax {
        /// The file.
        path: std::path::PathBuf,
        /// The line number.
        line: usize,
        /// The line the core does not understand.
        text: String,
    },
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file.
        path: std::path::PathBuf,
        /// Why it failed.
        #[source]
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("cannot write {path}: {source}")]
    Write {
        /// The file.
        path: std::path::PathBuf,
        /// Why it failed.
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory that goes away when the test ends.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "quasselcore-rs-config-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("creating the test directory");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_file_that_is_not_there_is_not_an_error() {
        let dir = TempDir::new("missing");
        let path = dir.path().join("quasselcore.conf");
        let config = ConfigFile::load(&path).expect("a missing file is not an error");
        assert!(config.core("StorageSettings\\Backend").is_none());
    }

    #[test]
    fn a_file_that_cannot_be_read_is_an_error() {
        let dir = TempDir::new("directory");
        // A directory is not a configuration file.
        let path = dir.path().join("quasselcore.conf");
        std::fs::create_dir_all(&path).expect("creating the directory");
        let error = ConfigFile::load(&path).unwrap_err();
        assert!(error.to_string().contains("cannot read"), "{error}");
    }

    #[test]
    fn a_line_the_core_does_not_understand_is_reported_with_its_text() {
        let dir = TempDir::new("syntax");
        let path = dir.path().join("quasselcore.conf");
        std::fs::write(&path, "[Core]\ngarbage\n").expect("writing the file");
        let error = ConfigFile::load(&path).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("garbage"), "{text}");
        assert!(text.contains("key=value"), "{text}");
    }

    #[test]
    fn the_configuration_file_is_only_readable_by_its_owner() {
        let dir = TempDir::new("permissions");
        let path = dir.path().join("quasselcore.conf");
        let mut config = ConfigFile::default();
        config.set("Core", "AuthSettings\\Authenticator", "Database");
        config.save(&path).expect("writing the file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "the file is readable by others");
        }
    }
}

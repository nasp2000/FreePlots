// SPDX-License-Identifier: MIT
//! File access the desktop installs for the interface.
//!
//! `fp-ui` opens no file and reads no environment variable: it assembles the
//! path and the text, and asks the desktop for the four helpers below. The
//! desktop supplies them from `fp-app/src/platform.rs`.

use std::path::{Path, PathBuf};

/// Reads a whole file, or `None` when it is missing or unreadable.
pub type ReadFile = fn(&Path) -> Option<String>;

/// Writes a whole file, replacing what it held.
pub type WriteFile = fn(&Path, &str) -> Result<(), String>;

/// Creates a directory and every missing parent.
pub type CreateDir = fn(&Path) -> Result<(), String>;

/// The directory for temporary files.
pub type TempDir = fn() -> PathBuf;

/// The file helpers the desktop installs for the interface.
///
/// A storage without helpers touches nothing: reads find nothing and writes
/// are refused with a message, which is what a headless test wants.
#[derive(Clone, Copy)]
pub struct Storage {
    read: Option<ReadFile>,
    write: Option<WriteFile>,
    create_dir: Option<CreateDir>,
    temp_dir: Option<TempDir>,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Storage")
            .field("installed", &self.is_installed())
            .finish()
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::none()
    }
}

impl Storage {
    /// A storage that touches nothing.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            read: None,
            write: None,
            create_dir: None,
            temp_dir: None,
        }
    }

    /// The helpers the desktop provides.
    #[must_use]
    pub const fn new(
        read: ReadFile,
        write: WriteFile,
        create_dir: CreateDir,
        temp_dir: TempDir,
    ) -> Self {
        Self {
            read: Some(read),
            write: Some(write),
            create_dir: Some(create_dir),
            temp_dir: Some(temp_dir),
        }
    }

    /// Whether every helper is installed.
    #[must_use]
    pub const fn is_installed(&self) -> bool {
        self.read.is_some()
            && self.write.is_some()
            && self.create_dir.is_some()
            && self.temp_dir.is_some()
    }

    /// Reads a whole file.
    #[must_use]
    pub fn read(&self, path: &Path) -> Option<String> {
        match self.read {
            Some(read) => read(path),
            None => None,
        }
    }

    /// Writes a whole file.
    ///
    /// # Errors
    ///
    /// Returns the host message, or a message when no host is installed.
    pub fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        match self.write {
            Some(write) => write(path, text),
            None => Err("the desktop installed no file access".to_owned()),
        }
    }

    /// Creates a directory and its parents.
    ///
    /// # Errors
    ///
    /// Returns the host message, or a message when no host is installed.
    pub fn create_dir(&self, path: &Path) -> Result<(), String> {
        match self.create_dir {
            Some(create_dir) => create_dir(path),
            None => Err("the desktop installed no file access".to_owned()),
        }
    }

    /// The temporary directory, when the desktop installed one.
    #[must_use]
    pub fn temp_dir(&self) -> Option<PathBuf> {
        self.temp_dir.map(|temp_dir| temp_dir())
    }
}

/// The storage the interface starts with.
///
/// The desktop replaces it with the real helpers at startup. Tests act as the
/// desktop, so they keep reading and writing ordinary files.
#[cfg(not(test))]
pub(crate) const fn default_storage() -> Storage {
    Storage::none()
}

/// See [`default_storage`]: the tests' desktop.
#[cfg(test)]
pub(crate) fn default_storage() -> Storage {
    Storage::new(test_read, test_write, test_create_dir, test_temp_dir)
}

#[cfg(test)]
fn test_read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
fn test_write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
fn test_create_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
fn test_temp_dir() -> PathBuf {
    std::env::temp_dir()
}

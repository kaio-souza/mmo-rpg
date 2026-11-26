//! Desktop: the disk.

use crate::Entry;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

/// A file only its owner can read on Unix (it holds device tokens).
pub fn write_private(path: impl AsRef<Path>, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        // An older file keeps its mode on open: set it again.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)
    }
}

pub fn create_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::remove_dir_all(path)
}

/// The entries of a folder, unsorted.
pub fn list(dir: impl AsRef<Path>) -> std::io::Result<Vec<Entry>> {
    Ok(std::fs::read_dir(dir)?
        .flatten()
        .map(|e| {
            let path = e.path();
            Entry {
                is_dir: path.is_dir(),
                path,
            }
        })
        .collect())
}

/// Last change, for reloading files while the game runs (never in a browser).
pub fn modified(path: impl AsRef<Path>) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Where the user's settings go: `$XDG_CONFIG_HOME` or `~/.config` on
/// Linux, `%APPDATA%` on Windows (the game adds its own folder).
pub fn config_base() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
    }
}

/// Where caches go: `$XDG_CACHE_HOME` or `~/.cache` on Linux,
/// `%LOCALAPPDATA%` on Windows.
pub fn cache_base() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    }
}

//! The client's files, the same calls on every platform.
//!
//! On a desktop they are the disk (`std::fs`). In a browser the game's
//! `content/` and `assets/` come from one bundle fetched before the client
//! starts ([`bundle`], mounted with `mount`), and what the client writes (the
//! user folder, the text-pack cache) is kept in the page's `localStorage`.
//! Paths are the ones the desktop client uses: `assets/fonts/x.ttf`.

pub mod bundle;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

use std::path::PathBuf;

/// One entry of a folder.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// A file read as UTF-8.
pub fn read_to_string(path: impl AsRef<std::path::Path>) -> std::io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

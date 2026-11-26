//! Browser: the mounted bundle for the game's files, `localStorage` for
//! what the client writes (`/config` and `/cache`).

use crate::Entry;
use crate::bundle::{Files, key};
use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use wasm_bindgen::JsCast;

thread_local! {
    static MOUNTED: RefCell<Files> = const { RefCell::new(Files::new()) };
}

/// Prefix of the client's own entries in `localStorage`.
const STORED: &str = "file:";

/// Makes the game's files readable (the bundle fetched at start).
pub fn mount(files: Files) {
    MOUNTED.with(|m| m.borrow_mut().extend(files));
}

/// Fetches a file of the page's site (the game bundle).
pub async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let resp = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("{url}: {e:?}"))?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|_| format!("{url}: not a response"))?;
    if !resp.ok() {
        return Err(format!("{url}: HTTP {}", resp.status()));
    }
    let buf = resp.array_buffer().map_err(|e| format!("{url}: {e:?}"))?;
    let buf = wasm_bindgen_futures::JsFuture::from(buf)
        .await
        .map_err(|e| format!("{url}: {e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Only the user folder and the cache are written.
fn writable(k: &str) -> bool {
    k.starts_with("config/") || k.starts_with("cache/")
}

pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let k = key(path.as_ref());
    if writable(&k)
        && let Some(text) = storage().and_then(|s| s.get_item(&format!("{STORED}{k}")).ok().flatten())
    {
        return from_base64(&text).ok_or_else(|| Error::new(ErrorKind::InvalidData, "stored file damaged"));
    }
    MOUNTED
        .with(|m| m.borrow().get(&k).cloned())
        .ok_or_else(|| Error::new(ErrorKind::NotFound, k))
}

pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let k = key(path.as_ref());
    if !writable(&k) {
        return Err(Error::new(ErrorKind::PermissionDenied, k));
    }
    let s = storage().ok_or_else(|| Error::other("no localStorage"))?;
    // A full storage (the quota) only loses this file.
    s.set_item(&format!("{STORED}{k}"), &to_base64(bytes.as_ref()))
        .map_err(|_| Error::other("localStorage is full"))
}

/// The page keeps no file modes: the same as `write`.
pub fn write_private(path: impl AsRef<Path>, text: &str) -> std::io::Result<()> {
    write(path, text)
}

pub fn create_dir_all(_path: impl AsRef<Path>) -> std::io::Result<()> {
    Ok(())
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
    let prefix = format!("{STORED}{}/", key(path.as_ref()));
    let Some(s) = storage() else { return Ok(()) };
    let n = s.length().unwrap_or(0);
    let keys: Vec<String> = (0..n).filter_map(|i| s.key(i).ok().flatten()).collect();
    for k in keys.iter().filter(|k| k.starts_with(&prefix)) {
        let _ = s.remove_item(k);
    }
    Ok(())
}

/// The entries of a folder of the bundle.
pub fn list(dir: impl AsRef<Path>) -> std::io::Result<Vec<Entry>> {
    let k = key(dir.as_ref());
    let prefix = if k.is_empty() {
        String::new()
    } else {
        format!("{k}/")
    };
    let mut out: Vec<Entry> = MOUNTED.with(|m| {
        m.borrow()
            .keys()
            .filter_map(|p| p.strip_prefix(&prefix))
            .map(|rest| match rest.split_once('/') {
                Some((d, _)) => Entry {
                    path: dir.as_ref().join(d),
                    is_dir: true,
                },
                None => Entry {
                    path: dir.as_ref().join(rest),
                    is_dir: false,
                },
            })
            .collect()
    });
    out.dedup();
    if out.is_empty() {
        return Err(Error::new(ErrorKind::NotFound, k));
    }
    Ok(out)
}

/// Files of the bundle never change while the page is open.
pub fn modified(_path: impl AsRef<Path>) -> Option<SystemTime> {
    None
}

pub fn config_base() -> Option<PathBuf> {
    Some(PathBuf::from("/config"))
}

pub fn cache_base() -> Option<PathBuf> {
    Some(PathBuf::from("/cache"))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn to_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= c.len() {
                let idx = (n >> (18 - 6 * i)) & 63;
                out.push(char::from(B64.get(idx as usize).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn from_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut n = 0u32;
    let mut bits = 0;
    for ch in text.bytes().filter(|c| *c != b'=') {
        let v = B64.iter().position(|b| *b == ch)? as u32;
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
        }
    }
    Some(out)
}

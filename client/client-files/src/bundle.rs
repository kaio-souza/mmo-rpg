//! A game's files in one blob, for the browser client: `MMOBNDL1`, then for
//! each file a `u32` path length, the path (UTF-8, `/`-separated), a `u32`
//! data length and the data, all little-endian.

use std::collections::BTreeMap;
use std::path::Path;

const MAGIC: &[u8; 8] = b"MMOBNDL1";

/// Path (`/`-separated) to bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

pub fn encode(files: &Files) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    for (path, data) in files {
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

pub fn decode(bytes: &[u8]) -> Result<Files, String> {
    let mut rest = bytes.strip_prefix(MAGIC.as_slice()).ok_or("not a game bundle")?;
    let mut files = Files::new();
    let take = |rest: &mut &[u8]| -> Result<Vec<u8>, String> {
        let (len, tail) = rest.split_first_chunk::<4>().ok_or("bundle cut short")?;
        let len = u32::from_le_bytes(*len) as usize;
        let data = tail.get(..len).ok_or("bundle cut short")?.to_vec();
        *rest = tail.get(len..).unwrap_or_default();
        Ok(data)
    };
    while !rest.is_empty() {
        let path = String::from_utf8(take(&mut rest)?).map_err(|_| "bundle path is not UTF-8")?;
        let data = take(&mut rest)?;
        files.insert(path, data);
    }
    Ok(files)
}

/// Every file under `dir`, stored as `<prefix>/<path below dir>`.
pub fn add_dir(files: &mut Files, dir: &Path, prefix: &str) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let key = format!("{prefix}/{name}");
        if p.is_dir() {
            add_dir(files, &p, &key)?;
        } else {
            files.insert(key, std::fs::read(&p)?);
        }
    }
    Ok(())
}

/// A path as a bundle key: `/`-separated, without `./` or a leading `/`.
pub fn key(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            std::path::Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut f = Files::new();
        f.insert("assets/a.png".into(), vec![1, 2, 3]);
        f.insert("content/text/en/ui.ron".into(), b"{}".to_vec());
        f.insert("empty".into(), Vec::new());
        assert_eq!(decode(&encode(&f)).unwrap(), f);
        assert!(decode(b"nope").is_err());
        let mut cut = encode(&f);
        cut.pop();
        assert!(decode(&cut).is_err());
    }

    #[test]
    fn keys_are_plain() {
        assert_eq!(key(Path::new("./assets/x/../y.png")), "assets/y.png");
        assert_eq!(key(Path::new("/config/game/login.ron")), "config/game/login.ron");
    }
}

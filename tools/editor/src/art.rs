//! Sprites and icons: the catalogue the page shows and the PNG upload
//! ([pipeline de assets]).
//!
//! A graphic is called by name, so the name of a sprite or icon is its file
//! stem: putting `wolf.png` under `assets/sprites/creatures/` is all it
//! takes for `sprite: "wolf"` to work. The editor writes the file and
//! nothing else.
//!
//! No rule of the game and no rule of the art lives here: the expected size
//! of an image is read from the files already in the same folder, so a
//! folder with 64×64 creatures warns about a 48×48 upload without the
//! editor knowing what a creature is.

use std::path::{Path, PathBuf};

/// Biggest PNG the page may upload (provisional art is small; a 4 MiB
/// file is a mistake, not a sprite).
pub const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;

/// A PNG under `assets/`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Image {
    /// File stem: the name content calls it by.
    pub name: String,
    /// Path under `assets/`, as the designer sees it.
    pub path: String,
    /// Folder under `sprites/` or `icons/` (empty at the root).
    pub folder: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

/// `sprite` and `icon` are the two kinds of graphic content refers to.
pub fn folder_of(kind: &str) -> Option<&'static str> {
    match kind {
        "sprite" => Some("sprites"),
        "icon" => Some("icons"),
        _ => None,
    }
}

/// A name content may use: the same shape as a content id.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A folder the upload may write into: one path step, or empty for the
/// root of `sprites/` and `icons/`.
pub fn valid_folder(folder: &str) -> bool {
    folder.is_empty() || (valid_name(folder) && folder.len() <= 32)
}

/// Every PNG of one kind, sorted by name.
pub fn list(assets: &Path, kind: &str) -> Vec<Image> {
    let Some(folder) = folder_of(kind) else {
        return Vec::new();
    };
    let root = assets.join(folder);
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Image>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "png") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let folder = rel
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or_default()
            .replace('\\', "/");
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let (width, height) = std::fs::read(&path)
            .ok()
            .and_then(|b| png_size(&b))
            .unwrap_or((0, 0));
        out.push(Image {
            name: name.to_owned(),
            path: path
                .strip_prefix(root.parent().unwrap_or(root))
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
            folder,
            width,
            height,
            bytes,
        });
    }
}

/// Width and height of a PNG, or `None` when the bytes are not a PNG.
///
/// Only the signature and the `IHDR` chunk are read, which is all the
/// page needs to show a size and to compare it with the folder.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.get(..8)? != SIGNATURE || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let h = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// What the page says about an upload that went through: where the file
/// landed, and a warning when its size is not the one the folder uses.
#[derive(Debug, serde::Serialize)]
pub struct Saved {
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    /// Replaced a file that was already there.
    pub replaced: bool,
    pub warnings: Vec<String>,
}

/// Writes `bytes` as `<assets>/<kind folder>/<folder>/<name>.png`.
///
/// Refuses anything that is not a PNG, a name content could not call, a
/// file over [`MAX_PNG_BYTES`], and an existing name unless `replace`
/// (nothing is overwritten by accident).
pub fn save(
    assets: &Path,
    kind: &str,
    folder: &str,
    name: &str,
    bytes: &[u8],
    replace: bool,
) -> Result<Saved, String> {
    let Some(kind_folder) = folder_of(kind) else {
        return Err(format!("`{kind}` is neither a sprite nor an icon"));
    };
    if !valid_name(name) {
        return Err(format!("`{name}`: a name uses only a-z, 0-9 and _"));
    }
    if !valid_folder(folder) {
        return Err(format!("`{folder}`: a folder uses only a-z, 0-9 and _"));
    }
    if bytes.len() > MAX_PNG_BYTES {
        return Err(format!(
            "{} KiB: over the {} KiB an image may have",
            bytes.len() / 1024,
            MAX_PNG_BYTES / 1024
        ));
    }
    let Some((width, height)) = png_size(bytes) else {
        return Err("not a PNG file".to_owned());
    };
    let root = assets.join(kind_folder);
    // The same name twice in the folder tree would make one of them
    // unreachable: content calls art by name, not by path.
    let existing = list(assets, kind).into_iter().find(|i| i.name == name);
    if let Some(old) = &existing {
        if !replace {
            return Err(format!("`{name}` already exists in {}", old.path));
        }
        if old.folder != folder {
            return Err(format!(
                "`{name}` already exists in {}: a name is unique in the whole folder",
                old.path
            ));
        }
    }
    let dir = if folder.is_empty() {
        root.clone()
    } else {
        root.join(folder)
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(format!("{name}.png"));
    std::fs::write(&file, bytes).map_err(|e| format!("{}: {e}", file.display()))?;

    let mut warnings = Vec::new();
    if let Some((w, h)) = folder_size(assets, kind, folder, name)
        && (w, h) != (width, height)
    {
        warnings.push(format!(
            "the other images of {kind_folder}/{folder} are {w}×{h}; this one is {width}×{height}"
        ));
    }
    Ok(Saved {
        name: name.to_owned(),
        path: file
            .strip_prefix(assets)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/"),
        width,
        height,
        replaced: existing.is_some(),
        warnings,
    })
}

/// The size every other image of the folder shares, when they agree.
fn folder_size(assets: &Path, kind: &str, folder: &str, skip: &str) -> Option<(u32, u32)> {
    let mut sizes = list(assets, kind)
        .into_iter()
        .filter(|i| i.folder == folder && i.name != skip && i.width > 0)
        .map(|i| (i.width, i.height));
    let first = sizes.next()?;
    sizes.all(|s| s == first).then_some(first)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// 1×1 PNG, the smallest a browser produces.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        b.extend_from_slice(&13u32.to_be_bytes());
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn a_png_gives_its_size_and_anything_else_is_refused() {
        assert_eq!(png_size(&png(64, 32)), Some((64, 32)));
        assert_eq!(png_size(b"GIF89a"), None);
        assert_eq!(png_size(&[]), None);
        // A PNG whose header says 0 wide is not a usable image.
        assert_eq!(png_size(&png(0, 8)), None);
    }

    #[test]
    fn names_follow_the_rule_for_content_names() {
        assert!(valid_name("old_wolf") && valid_name("w2"));
        assert!(!valid_name("") && !valid_name("Wolf") && !valid_name("old-wolf"));
        assert!(!valid_name("../etc/passwd") && !valid_name("a/b"));
        assert!(valid_folder("") && valid_folder("creatures") && !valid_folder("a/b"));
    }

    #[test]
    fn upload_writes_the_file_and_guards_the_name() {
        let dir = std::env::temp_dir().join(format!("editor_art_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let saved = save(&dir, "sprite", "creatures", "wolf", &png(64, 64), false).unwrap();
        assert_eq!(saved.path, "sprites/creatures/wolf.png");
        assert!(saved.warnings.is_empty() && !saved.replaced);
        assert_eq!(list(&dir, "sprite")[0].name, "wolf");

        // The same name twice needs `replace`, and never moves folder.
        assert!(save(&dir, "sprite", "creatures", "wolf", &png(64, 64), false).is_err());
        assert!(save(&dir, "sprite", "equipment", "wolf", &png(64, 64), true).is_err());
        assert!(
            save(&dir, "sprite", "creatures", "wolf", &png(64, 64), true)
                .unwrap()
                .replaced
        );

        // A size the folder does not use is a warning, not a refusal.
        let odd = save(&dir, "sprite", "creatures", "boar", &png(48, 48), false).unwrap();
        assert_eq!(odd.warnings.len(), 1, "{odd:?}");

        assert!(save(&dir, "sprite", "creatures", "bad", b"nope", false).is_err());
        assert!(save(&dir, "theme", "", "x", &png(8, 8), false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

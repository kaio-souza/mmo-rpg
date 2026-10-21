//! Game screen: the game's name (`ui.game.title` in the client's text
//! files, `assets/text/<locale>/ui.ron`) and the picture behind the login
//! screens (`assets/ui/login_background.png`; without it the client draws
//! its summoning circle).

use crate::art;
use crate::texts::{Entry, TextFile};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The key of the game's name.
pub const NAME_KEY: &str = "ui.game.title";
/// Where the login background lives, relative to `assets/`.
pub const BACKGROUND: &str = "ui/login_background.png";
/// Smaller than this is a blur on any screen.
const MIN_SIZE: (u32, u32) = (640, 360);

fn ui_file(assets: &Path, locale: &str) -> PathBuf {
    assets.join("text").join(locale).join("ui.ron")
}

/// The client's languages: the folders of `assets/text/`, English first.
fn locales(assets: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(assets.join("text"))
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().join("ui.ron").is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|l| (l != "en", l.clone()));
    out
}

/// The name in every language, its length limit and the background.
pub fn read(assets: &Path) -> Value {
    let mut names = BTreeMap::new();
    let mut max_chars = None;
    let order = locales(assets);
    for l in &order {
        let Ok(text) = std::fs::read_to_string(ui_file(assets, l)) else {
            continue;
        };
        let Ok(file) = TextFile::parse(&text) else {
            continue;
        };
        if let Some((_, e)) = file.entries.iter().find(|(k, _)| k == NAME_KEY) {
            names.insert(l.clone(), e.text().to_owned());
            if let Entry::Full(f) = e
                && l == "en"
            {
                max_chars = f.max_chars;
            }
        }
    }
    let bg = std::fs::read(assets.join(BACKGROUND)).ok();
    let size = bg.as_deref().and_then(art::png_size);
    json!({
        "locales": order,
        "names": names,
        "max_chars": max_chars,
        "background": size.map(|(w, h)| json!({ "width": w, "height": h, "bytes": bg.map_or(0, |b| b.len()) })),
    })
}

/// Writes the name in each language given. Only the name's line changes
/// (the rest of the file stays byte for byte); English keeps its note and
/// limit; an empty name is refused (the game would show the key).
pub fn save_names(assets: &Path, names: &BTreeMap<String, String>) -> Result<(), String> {
    let known = locales(assets);
    let limit = read(assets)["max_chars"]
        .as_u64()
        .and_then(|m| usize::try_from(m).ok());
    let mut planned = Vec::new();
    for (l, name) in names {
        if !known.contains(l) {
            return Err(format!("`{l}` is not a language of the client"));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("{l}: the name is empty"));
        }
        // English sets the limit for every language (as `validate` checks).
        if let Some(m) = limit
            && name.chars().count() > m
        {
            return Err(format!("{l}: \"{name}\" has more than {m} characters"));
        }
        let path = ui_file(assets, l);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = TextFile::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let line = match file.entries.iter().find(|(k, _)| k == NAME_KEY).map(|(_, e)| e) {
            Some(Entry::Full(f)) => {
                let mut v = format!("(text: {}", quote(name));
                if let Some(n) = &f.note {
                    v.push_str(&format!(", note: {}", quote(n)));
                }
                if let Some(m) = f.max_chars {
                    v.push_str(&format!(", max_chars: {m}"));
                }
                v.push(')');
                v
            }
            _ => quote(name),
        };
        planned.push((path, replace_line(&text, NAME_KEY, &line)?));
    }
    for (path, text) in planned {
        // The result must still be a text file the game reads.
        TextFile::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// A string as the hand-written files spell it (`'` stays as is).
fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// The file with the line of `key` set to `value`; a missing key goes
/// before the closing brace.
fn replace_line(text: &str, key: &str, value: &str) -> Result<String, String> {
    let head = format!("\"{key}\":");
    let new = format!("    {head} {value},");
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if !found && l.trim_start().starts_with(&head) {
                found = true;
                new.clone()
            } else {
                l.to_owned()
            }
        })
        .collect();
    if !found {
        let close = out
            .iter()
            .rposition(|l| l.trim() == "}")
            .ok_or("the text file has no closing brace")?;
        out.insert(close, new);
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    Ok(joined)
}

/// Puts a new login background in place (a PNG, at least 640 x 360).
pub fn save_background(assets: &Path, bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > art::MAX_PNG_BYTES {
        return Err(format!(
            "{} KiB: over the {} KiB an image may have",
            bytes.len() / 1024,
            art::MAX_PNG_BYTES / 1024
        ));
    }
    let (w, h) = art::png_size(bytes).ok_or("not a PNG file")?;
    if w < MIN_SIZE.0 || h < MIN_SIZE.1 {
        return Err(format!(
            "{w}x{h}: the background needs at least {}x{}",
            MIN_SIZE.0, MIN_SIZE.1
        ));
    }
    let path = assets.join(BACKGROUND);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "width": w, "height": h }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn only_the_name_line_changes() {
        let text = "// Texts.\n{\n    \"a\": \"it's\",\n    \"ui.game.title\": \"Old\",\n}\n";
        let out = replace_line(text, NAME_KEY, &quote("New")).unwrap();
        assert_eq!(out, text.replace("\"Old\"", "\"New\""));
        let missing = replace_line("{\n}\n", NAME_KEY, &quote("N")).unwrap();
        assert_eq!(missing, "{\n    \"ui.game.title\": \"N\",\n}\n");
    }

    #[test]
    fn the_real_name_is_read_in_every_language() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets");
        let v = read(&assets);
        assert_eq!(v["locales"][0], "en");
        assert!(v["names"]["en"].as_str().is_some_and(|n| !n.is_empty()));
        assert!(v["max_chars"].as_u64().is_some());
    }
}

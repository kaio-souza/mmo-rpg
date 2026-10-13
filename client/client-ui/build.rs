//! The interface texts built into the client, one entry per
//! language of `content/text/locales.ron`, in list order: adding
//! a language is a folder and a line in that file, then a rebuild.
//!
//! The game folder is `MMO_GAME_DIR` (absolute, or relative to the workspace
//! root); without it, the new-game template `templates/new-game`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct Entry {
    code: String,
    #[serde(default)]
    required: bool,
}

/// The folder's name, kept to `[a-z0-9_-]` (anything else becomes `_`).
fn game_id(root: &Path) -> String {
    let name = root
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let id: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if id.trim_matches('_').is_empty() {
        "game".to_owned()
    } else {
        id
    }
}

fn main() {
    let workspace = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("../..");
    println!("cargo:rerun-if-env-changed=MMO_GAME_DIR");
    let root =
        workspace.join(std::env::var("MMO_GAME_DIR").unwrap_or_else(|_| "templates/new-game".to_owned()));
    let assets = root.join("assets/text");
    let content = root.join("content/text");
    println!("cargo:rerun-if-changed={}", assets.display());
    println!("cargo:rerun-if-changed={}", content.join("locales.ron").display());
    let list: Vec<Entry> = std::fs::read_to_string(content.join("locales.ron"))
        .ok()
        .and_then(|t| ron::from_str(&t).ok())
        .unwrap_or_else(|| {
            let mut codes: Vec<String> = std::fs::read_dir(&assets)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            codes.sort_by_key(|c| (c != "en", c.clone()));
            codes
                .into_iter()
                .map(|code| Entry { code, required: true })
                .collect()
        });
    let file = |p: &Path| -> String {
        println!("cargo:rerun-if-changed={}", p.display());
        if p.is_file() {
            format!("include_str!({:?})", p.display().to_string())
        } else {
            "\"{}\"".to_owned()
        }
    };
    let mut out = String::from(
        "/// `(code, ui.ron, error.ron, required)` per language, in list order.\npub const FILES: &[(&str, &str, &str, bool)] = &[\n",
    );
    for e in &list {
        let _ = writeln!(
            out,
            "    ({:?}, {}, {}, {}),",
            e.code,
            file(&assets.join(&e.code).join("ui.ron")),
            file(&content.join(&e.code).join("error.ron")),
            e.required || e.code == "en"
        );
    }
    out.push_str("];\n");
    let _ = writeln!(
        out,
        "/// The game this client is built for: the name of its folder, used for\n/// the user and cache folders.\npub const GAME_ID: &str = {:?};",
        game_id(&root)
    );
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("ui_files.rs");
    let _ = std::fs::write(dest, out);
}

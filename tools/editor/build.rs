//! The editor's own labels (`text/<locale>/editor.ron`), one entry per
//! folder, English first: a new folder is a new language of the editor
//! page after a rebuild.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("text");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut codes: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("editor.ron").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    codes.sort_by_key(|c| (c != "en", c.clone()));
    let mut out =
        String::from("/// `(locale code, editor.ron)`, English first.\nconst FILES: &[(&str, &str)] = &[\n");
    for c in codes {
        let p = dir.join(&c).join("editor.ron");
        println!("cargo:rerun-if-changed={}", p.display());
        let _ = writeln!(out, "    ({c:?}, include_str!({:?})),", p.display().to_string());
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("editor_files.rs");
    let _ = std::fs::write(dest, out);
}

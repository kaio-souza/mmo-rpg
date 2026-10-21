//! The content folder on disk: read every file, check a changed set with
//! the loader, write one file atomically.

use defs::ContentProblem;
use i18n::{Locale, Severity, TextProblemKind};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

pub struct Store {
    root: PathBuf,
}

/// Result of checking the content with one change applied.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// Block saving.
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Text key -> locales without it (filled from `en` to test).
    pub missing: BTreeMap<String, Vec<String>>,
    /// Each error and warning cut into text keys (P12): the page
    /// shows them in the editor's language.
    pub parts: BTreeMap<String, Vec<serde_json::Value>>,
}

/// One piece as `{text}` or `{key, args}`.
fn part_json(p: &defs::ProblemPart) -> serde_json::Value {
    match p {
        defs::ProblemPart::Text(t) => serde_json::json!({ "text": t }),
        defs::ProblemPart::Coded { key, args } => {
            let args: serde_json::Map<String, serde_json::Value> = args
                .iter()
                .map(|(n, v)| ((*n).to_owned(), part_json(v)))
                .collect();
            serde_json::json!({ "key": key, "args": args })
        }
    }
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Store {
    pub fn new(root: PathBuf) -> Store {
        Store { root }
    }

    /// `path` relative to the content folder, never leaving it.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        let ok = !path.is_empty()
            && p.components().all(|c| matches!(c, Component::Normal(_)))
            && p.extension().is_some_and(|e| e == "ron");
        if ok {
            Ok(self.root.join(p))
        } else {
            Err(format!("bad path `{path}`"))
        }
    }

    /// Every `.ron` file: `(relative path, text)`, sorted.
    pub fn files(&self) -> Result<Vec<(String, String)>, String> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), String> {
            let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            for entry in rd {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_dir() {
                    walk(root, &path, out)?;
                } else if path.extension().is_some_and(|e| e == "ron") {
                    let text =
                        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                    let rel = path.strip_prefix(root).unwrap_or(&path);
                    let rel: Vec<String> = rel.iter().map(|c| c.to_string_lossy().into_owned()).collect();
                    out.push((rel.join("/"), text));
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out)?;
        out.sort();
        Ok(out)
    }

    pub fn read(&self, path: &str) -> Result<Option<String>, String> {
        let full = self.resolve(path)?;
        match std::fs::read_to_string(&full) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{path}: {e}")),
        }
    }

    /// Paths git already has, relative to the content folder. `None` when
    /// git cannot tell (no repository, no git): then nothing counts as new.
    pub fn tracked(&self) -> Option<std::collections::BTreeSet<String>> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["ls-files", "-z"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(
            out.stdout
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .collect(),
        )
    }

    /// Removes a file that git does not have yet: a published id is never
    /// removed.
    pub fn remove_new(&self, path: &str) -> Result<(), String> {
        let full = self.resolve(path)?;
        match self.tracked() {
            Some(t) if !t.contains(path) => {}
            _ => return Err(format!("{path}: already in git; a published id is never removed")),
        }
        match std::fs::remove_file(&full) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{path}: {e}")),
        }
    }

    /// Writes through a temporary file and a rename: a crash never leaves
    /// half a file for the server's reload to read.
    pub fn write(&self, path: &str, text: &str) -> Result<(), String> {
        let full = self.resolve(path)?;
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{path}: {e}"))?;
        }
        let tmp = full.with_extension("editor-tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{path}: {e}"))?;
        std::fs::rename(&tmp, &full).map_err(|e| format!("{path}: {e}"))
    }
}

/// `files` with `changes` applied (path -> new text) and `removed` gone.
pub fn with_changes(
    files: &[(String, String)],
    changes: &[(String, String)],
    removed: &[String],
) -> Vec<(String, String)> {
    let mut map: BTreeMap<String, String> = files.iter().cloned().collect();
    for p in removed {
        map.remove(p);
    }
    for (p, t) in changes {
        map.insert(p.clone(), t.clone());
    }
    map.into_iter().collect()
}

/// Loads, fills missing translations from `en` (they do not block a save,
///) and validates.
/// Sprite and icon names without a file in `assets` are warnings.
/// Sprite and icon names the content refers to, for the graphics screen:
/// an image no content names is dead weight, which is the other half of
/// the warning about a name with no image.
pub fn used_art(files: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let Ok(mut catalog) = defs::from_text(&refs) else {
        return (Vec::new(), Vec::new());
    };
    let _ = catalog.fill_missing_text();
    match catalog.validate() {
        Ok(valid) => (valid.sprites().to_vec(), valid.icons().to_vec()),
        Err(_) => (Vec::new(), Vec::new()),
    }
}

pub fn check(files: &[(String, String)], assets: &defs::AssetNames) -> Report {
    let mut report = Report::default();
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let mut catalog = match defs::from_text(&refs) {
        Ok(c) => c,
        Err(e) => {
            report.errors.push(e.to_string());
            return report;
        }
    };
    for (locale, key) in catalog.fill_missing_text() {
        report
            .missing
            .entry(key)
            .or_default()
            .push(locale.code().to_owned());
    }
    match catalog.validate() {
        Ok(valid) => {
            report
                .warnings
                .extend(valid.warnings().iter().map(|w| w.to_string()));
            report.warnings.extend(assets.missing(&valid));
        }
        Err(problems) => {
            for p in problems {
                match &p {
                    ContentProblem::Text(t) if t.kind == TextProblemKind::MissingKey => {
                        if let Some(k) = &t.key {
                            report
                                .missing
                                .entry(k.clone())
                                .or_default()
                                .push(t.locale.code().to_owned());
                        }
                    }
                    _ if p.severity() == Severity::Warning => report.warnings.push(p.to_string()),
                    _ => report.errors.push(p.to_string()),
                }
            }
        }
    }
    for locales in report.missing.values_mut() {
        locales.sort();
        locales.dedup();
    }
    for m in report.errors.iter().chain(&report.warnings) {
        let parts = defs::problem_parts(m).iter().map(part_json).collect();
        report.parts.insert(m.clone(), parts);
    }
    report
}

/// The languages of the content (`text/locales.ron`), in list
/// order; a broken list falls back to the folders under `text/`.
pub fn locale_list(files: &[(String, String)]) -> Vec<defs::LocaleDef> {
    let mut folders: Vec<Locale> = files
        .iter()
        .filter_map(|(p, _)| {
            let mut parts = p.split('/');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("text"), Some(l), Some(_)) => l.parse().ok(),
                _ => None,
            }
        })
        .collect();
    folders.sort();
    folders.dedup();
    let list = files
        .iter()
        .find(|(p, _)| p == "text/locales.ron")
        .map(|(_, t)| t.as_str());
    defs::parse_locales(list, &folders)
        .or_else(|_| defs::parse_locales(None, &folders))
        .unwrap_or_default()
}

/// Locale codes of the content, in list order.
pub fn locales(files: &[(String, String)]) -> Vec<String> {
    locale_list(files)
        .iter()
        .map(|d| d.locale.code().to_owned())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn content() -> Store {
        Store::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/content"))
    }

    fn assets() -> defs::AssetNames {
        defs::AssetNames::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets"))
    }

    #[test]
    fn the_real_content_has_no_warnings() {
        let r = check(&content().files().unwrap(), &assets());
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn paths_stay_inside_the_folder() {
        let s = Store::new(PathBuf::from("/x"));
        assert!(s.resolve("items/a.ron").is_ok());
        assert!(s.resolve("../a.ron").is_err());
        assert!(s.resolve("/etc/a.ron").is_err());
        assert!(s.resolve("items/a.txt").is_err());
    }

    #[test]
    fn the_real_content_is_clean() {
        let r = check(&content().files().unwrap(), &assets());
        assert!(r.ok(), "{:?}", r.errors);
        assert!(r.missing.is_empty(), "{:?}", r.missing);
    }

    #[test]
    fn a_new_item_without_translation_saves_but_is_marked() {
        let files = content().files().unwrap();
        let en = files
            .iter()
            .find(|(p, _)| p == "text/en/items.ron")
            .unwrap()
            .1
            .clone();
        let en = en.replacen(
            '{',
            "{\n    \"item.stone.name\": \"Stone\",\n    \"item.stone.desc\": \"A stone.\",",
            1,
        );
        let changed = with_changes(
            &files,
            &[
                ("items/stone.ron".into(), "(kind: Material)".into()),
                ("text/en/items.ron".into(), en),
            ],
            &[],
        );
        let r = check(&changed, &assets());
        assert!(r.ok(), "{:?}", r.errors);
        assert_eq!(r.missing["item.stone.name"].len(), 9);
    }

    #[test]
    fn a_broken_reference_blocks_the_save() {
        let files = content().files().unwrap();
        let changed = with_changes(
            &files,
            &[(
                "techniques/quick_stab.ron".into(),
                "(proficiency: \"nope\", unlock_at: 1, cooldown_ticks: 1, damage_permille: 1000)".into(),
            )],
            &[],
        );
        assert!(!check(&changed, &assets()).ok());
    }

    /// Every content file goes through RON -> JSON -> RON without changing
    /// its meaning: saving in the editor never corrupts a file.
    #[test]
    fn every_content_file_round_trips() {
        for (path, text) in content().files().unwrap() {
            let Some((kind, _)) = defs::editor::kind_of(&path) else {
                continue;
            };
            let json = defs::editor::to_json(kind.name, &text).unwrap_or_else(|e| panic!("{path}: {e}"));
            let ron = defs::editor::to_ron(kind.name, json.clone()).unwrap();
            assert_eq!(defs::editor::to_json(kind.name, &ron).unwrap(), json, "{path}");
        }
    }

    /// A new content type is only done with a form.
    #[test]
    fn every_content_file_has_a_kind() {
        for (path, _) in content().files().unwrap() {
            let raw = path.starts_with("text/");
            assert!(
                raw || defs::editor::kind_of(&path).is_some(),
                "{path} has no form in the editor"
            );
        }
    }
}

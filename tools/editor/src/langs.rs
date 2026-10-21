//! The languages screen: what `text/locales.ron` lists, how much
//! of each language is translated (content texts and the client's
//! interface texts in `assets/text/`) and the plural and number rules
//! `i18n` has for it. The list itself is saved like any other file.

use crate::texts::{self, TextFile};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct LangInfo {
    pub code: String,
    pub required: bool,
    /// `content/text/<code>/` has files.
    pub folder: bool,
    /// Content texts: `(translated, total)`.
    pub content: (usize, usize),
    /// Client interface texts (`assets/text/<code>/ui.ron`).
    pub ui: (usize, usize),
    /// `None`: `i18n` has no rules for it and uses the English ones.
    pub rules: Option<Rules>,
}

#[derive(Debug, Serialize)]
pub struct Rules {
    pub plural: String,
    pub decimal: char,
    pub group: char,
}

fn ui_keys(assets: &Path, code: &str) -> Vec<String> {
    std::fs::read_to_string(assets.join("text").join(code).join("ui.ron"))
        .ok()
        .and_then(|t| TextFile::parse(&t).ok())
        .map(|f| f.entries.into_iter().map(|(k, _)| k).collect())
        .unwrap_or_default()
}

pub fn overview(files: &[(String, String)], assets: &Path) -> Vec<LangInfo> {
    let table = texts::table(files);
    let en_ui = ui_keys(assets, "en");
    crate::store::locale_list(files)
        .into_iter()
        .map(|d| {
            let code = d.locale.code().to_owned();
            let have = table.iter().filter(|r| r.texts.contains_key(&code)).count();
            let ui = ui_keys(assets, &code);
            let ui_have = en_ui.iter().filter(|k| ui.contains(k)).count();
            let prefix = format!("text/{code}/");
            LangInfo {
                folder: files.iter().any(|(p, _)| p.starts_with(&prefix)),
                content: (have, table.len()),
                ui: (ui_have, en_ui.len()),
                rules: i18n::known_rules(d.locale).map(|r| Rules {
                    plural: format!("{:?}", r.plural),
                    decimal: r.decimal,
                    group: r.group,
                }),
                required: d.required,
                code,
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_real_languages_are_whole() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara");
        let store = crate::store::Store::new(root.join("content"));
        let langs = overview(&store.files().unwrap(), &root.join("assets"));
        assert_eq!(langs.first().map(|l| l.code.as_str()), Some("en"));
        for l in &langs {
            assert!(l.rules.is_some(), "{}", l.code);
            if l.required {
                assert_eq!(l.content.0, l.content.1, "{}", l.code);
                assert_eq!(l.ui.0, l.ui.1, "{}", l.code);
            }
        }
    }
}

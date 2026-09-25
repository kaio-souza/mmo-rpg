//! The languages of the game: `content/text/locales.ron`.
//!
//! ```ron
//! [
//!     (code: "en", required: true),
//!     (code: "pt-BR", required: true),
//!     (code: "fr"),
//! ]
//! ```
//!
//! The order is the order of the language list. `required`: a text missing
//! in that language stops `validate`; otherwise it is a warning and the
//! English text is shown. English is the source and always required.
//! Without the file, every folder under `text/` is a required language.

use i18n::Locale;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleDef {
    pub locale: Locale,
    pub required: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct LocaleFile {
    pub code: String,
    #[serde(default)]
    pub required: bool,
}

/// The list, or the problems with it. `folders`: locales with a folder.
pub fn parse_locales(text: Option<&str>, folders: &[Locale]) -> Result<Vec<LocaleDef>, Vec<String>> {
    let Some(text) = text else {
        let mut all: Vec<LocaleDef> = folders
            .iter()
            .map(|l| LocaleDef {
                locale: *l,
                required: true,
            })
            .collect();
        all.sort_by_key(|d| d.locale != Locale::SOURCE);
        if !all.iter().any(|d| d.locale == Locale::SOURCE) {
            all.insert(
                0,
                LocaleDef {
                    locale: Locale::SOURCE,
                    required: true,
                },
            );
        }
        return Ok(all);
    };
    let files: Vec<LocaleFile> = ron::from_str(text).map_err(|e| vec![format!("{e}")])?;
    let mut out: Vec<LocaleDef> = Vec::new();
    let mut problems = Vec::new();
    for f in files {
        let Ok(locale) = f.code.parse::<Locale>() else {
            problems.push(format!("`{}` is not a locale code (like en or pt-BR)", f.code));
            continue;
        };
        if out.iter().any(|d| d.locale == locale) {
            problems.push(format!("`{locale}` is listed twice"));
            continue;
        }
        out.push(LocaleDef {
            locale,
            required: f.required || locale == Locale::SOURCE,
        });
    }
    if !out.iter().any(|d| d.locale == Locale::SOURCE) {
        problems.push(format!(
            "`{}` (the source language) must be listed",
            Locale::SOURCE
        ));
    }
    for l in folders {
        if !out.iter().any(|d| d.locale == *l) {
            problems.push(format!("text/{l}/ is not listed: add it or remove the folder"));
        }
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(c: &str) -> Locale {
        c.parse().unwrap()
    }

    #[test]
    fn the_list_its_flags_and_its_problems() {
        let list = parse_locales(
            Some(r#"[(code: "en"), (code: "pt-BR", required: true), (code: "fr")]"#),
            &[l("en"), l("pt-BR")],
        )
        .unwrap();
        assert_eq!(
            list.iter()
                .map(|d| (d.locale.code().to_owned(), d.required))
                .collect::<Vec<_>>(),
            [("en".into(), true), ("pt-BR".into(), true), ("fr".into(), false)]
        );
        assert!(parse_locales(Some(r#"[(code: "pt-BR")]"#), &[]).is_err());
        assert!(parse_locales(Some(r#"[(code: "en"), (code: "en")]"#), &[]).is_err());
        assert!(parse_locales(Some(r#"[(code: "en")]"#), &[l("de")]).is_err());
        assert!(parse_locales(Some(r#"[(code: "e n")]"#), &[]).is_err());
        let none = parse_locales(None, &[l("de"), l("en")]).unwrap();
        assert_eq!(none.first().map(|d| d.locale), Some(Locale::EN));
    }
}

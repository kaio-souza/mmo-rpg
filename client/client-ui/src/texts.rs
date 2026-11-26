//! Interface texts: `assets/text/<locale>/ui.ron` and the `error.*` keys of
//! `content/text/<locale>/error.ron`. The client reads them from the game
//! folder once, at start ([`set_game`]), so one client binary serves any
//! game. The screens before the login need them, and the `ErrorCode` list
//! is compiled in anyway. Content texts keep coming in the server text pack.

use base::{Arg, Locale, TextRef};
use i18n::{Bundle, KeyTable};
use serde::Deserialize;
use smallvec::SmallVec;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Entry {
    code: String,
    #[serde(default)]
    required: bool,
}

/// The interface texts of one language, as read from the game folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFile {
    pub code: String,
    /// `assets/text/<code>/ui.ron`, or `{}` when the file is missing.
    pub ui: String,
    /// `content/text/<code>/error.ron`, or `{}` when the file is missing.
    pub error: String,
    /// A required language is offered on the login screen.
    pub required: bool,
}

struct Game {
    id: String,
    files: Vec<TextFile>,
}

static GAME: OnceLock<Game> = OnceLock::new();

/// Reads the game's interface texts. `content` and `assets` are the game's
/// folders (the server's `--content` and the client's `--assets`). Fails
/// when English, the source language, has no `ui.ron`; a second call fails
/// too (the game is read once).
pub fn set_game(content: &Path, assets: &Path) -> Result<(), String> {
    let files = read_files(content, assets);
    let english_ui = files.iter().find(|f| f.code == "en").map(|f| f.ui.as_str());
    if english_ui.is_none_or(|t| t == "{}") {
        return Err(format!(
            "no English interface texts: expected {}",
            assets.join("text").join("en").join("ui.ron").display()
        ));
    }
    let id = slug(&english_title(&files).unwrap_or_default());
    let game = Game { id, files };
    GAME.set(game)
        .map_err(|_| "the game's texts are already set".to_owned())
}

/// The game's English title (`ui.game.title`), lower case and kept to
/// `[a-z0-9_-]` (anything else becomes `_`): the name of its user and
/// cache folders and of its audio stream. It does not depend on where the
/// game's folder is, so a package folder (`dist/`, a release archive) keeps
/// the player's settings across versions.
pub fn game_id() -> &'static str {
    GAME.get().map_or("game", |g| g.id.as_str())
}

/// Every language of the game, in list order.
pub fn text_files() -> &'static [TextFile] {
    GAME.get().map_or(&[], |g| g.files.as_slice())
}

/// `ui.game.title` in English, as the player sees it.
fn english_title(files: &[TextFile]) -> Option<String> {
    let en = files.iter().find(|f| f.code == "en")?;
    let sources = [("ui.ron", en.ui.as_str()), ("error.ron", en.error.as_str())];
    let keys = KeyTable::from_source(&sources).ok()?;
    let bundle = Bundle::from_text(Locale::EN, &keys, &sources).ok()?;
    let id = keys.id("ui.game.title")?;
    Some(i18n::format(
        &bundle,
        &TextRef {
            id,
            args: SmallVec::new(),
        },
    ))
}

fn slug(name: &str) -> String {
    let id: String = name
        .to_lowercase()
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

/// The languages: `content/text/locales.ron` in list order, or, without it,
/// every folder of `assets/text` (English first).
fn read_files(content: &Path, assets: &Path) -> Vec<TextFile> {
    let ui_dir = assets.join("text");
    let error_dir = content.join("text");
    let list: Vec<Entry> = client_files::read_to_string(error_dir.join("locales.ron"))
        .ok()
        .and_then(|t| ron::from_str(&t).ok())
        .unwrap_or_else(|| {
            let mut codes: Vec<String> = client_files::list(&ui_dir)
                .into_iter()
                .flatten()
                .filter(|e| e.is_dir)
                .filter_map(|e| e.path.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            codes.sort_by_key(|c| (c != "en", c.clone()));
            codes
                .into_iter()
                .map(|code| Entry { code, required: true })
                .collect()
        });
    list.into_iter()
        .map(|e| TextFile {
            ui: read_or_empty(&ui_dir.join(&e.code).join("ui.ron")),
            error: read_or_empty(&error_dir.join(&e.code).join("error.ron")),
            required: e.required || e.code == "en",
            code: e.code,
        })
        .collect()
}

fn read_or_empty(path: &Path) -> String {
    client_files::read_to_string(path).unwrap_or_else(|_| "{}".to_owned())
}

/// The required languages, each named in itself (`ui.locale.name` of its
/// own file), in list order: the language list of the login screen. An
/// optional language is left out: it needs no name and is never offered
pub fn language_names() -> &'static [(Locale, String)] {
    static NAMES: OnceLock<Vec<(Locale, String)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        text_files()
            .iter()
            .filter(|f| f.required)
            .filter_map(|f| f.code.parse::<Locale>().ok())
            .map(|l| (l, UiText::new(l).t("ui.locale.name")))
            .collect()
    })
}

pub struct UiText {
    keys: KeyTable,
    bundle: Option<Bundle>,
    /// What a language without some text shows.
    english: Option<Bundle>,
}

fn sources(code: &str) -> Option<[(&'static str, &'static str); 2]> {
    text_files()
        .iter()
        .find(|f| f.code == code)
        .map(|f| [("ui.ron", f.ui.as_str()), ("error.ron", f.error.as_str())])
}

impl UiText {
    pub fn new(locale: Locale) -> UiText {
        let keys = sources("en")
            .and_then(|f| KeyTable::from_source(&f).ok())
            .unwrap_or_else(KeyTable::empty);
        let bundle = sources(locale.code()).and_then(|f| Bundle::from_text(locale, &keys, &f).ok());
        let english = sources("en").and_then(|f| Bundle::from_text(Locale::EN, &keys, &f).ok());
        UiText {
            keys,
            bundle,
            english,
        }
    }

    /// The text of `key`; the key itself when missing (shows the bug).
    pub fn t(&self, key: &str) -> String {
        self.t_args(key, &[])
    }

    pub fn t_args(&self, key: &str, args: &[Arg]) -> String {
        let Some(id) = self.keys.id(key) else {
            return key.to_owned();
        };
        let b = self
            .bundle
            .as_ref()
            .filter(|b| b.get(id).is_some())
            .or(self.english.as_ref());
        match (Some(id), b) {
            (Some(id), Some(b)) => i18n::format(
                b,
                &TextRef {
                    id,
                    args: args.iter().cloned().collect::<SmallVec<_>>(),
                },
            ),
            _ => key.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_folder_gives_its_languages_and_texts() {
        let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/new-game");
        let files = read_files(&game.join("content"), &game.join("assets"));
        let en = files.iter().find(|f| f.code == "en").expect("english");
        assert!(en.required);
        assert_ne!(en.ui, "{}", "the template has english interface texts");
        assert!(files.iter().any(|f| f.code == "pt-BR"));
    }

    #[test]
    fn a_missing_file_is_an_empty_map_not_an_error() {
        assert_eq!(read_or_empty(Path::new("/nonexistent/ui.ron")), "{}");
    }

    #[test]
    fn the_game_id_is_the_english_title() {
        let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/new-game");
        let files = read_files(&game.join("content"), &game.join("assets"));
        assert_eq!(slug(&english_title(&files).unwrap_or_default()), "my_game");
        assert_eq!(slug("天空"), "game");
    }
}

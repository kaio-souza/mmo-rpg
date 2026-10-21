//! Theme screen: the colors, fonts, sizes and timings of the client's
//! interface (`assets/themes/<id>.ron`).
//! The shape and the rules are the client's own (`client_ui::ThemeFile`,
//! `client_ui::load_theme`), so the editor cannot save a theme the game
//! refuses. The running client reloads the file when it changes.

use client_ui::{
    COLORS, FALLBACK_THEME, FontSet, GAME_THEME_FILE, GameTheme, Panel, Sizes, SystemWindow, ThemeFile,
    WindowAnim,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The theme the game uses (`assets/theme.ron`); the player does not
/// choose it.
pub fn game_theme(assets: &Path) -> String {
    std::fs::read_to_string(assets.join(GAME_THEME_FILE))
        .ok()
        .and_then(|t| client_ui::read_game_theme(&t).ok())
        .unwrap_or_else(|| FALLBACK_THEME.into())
}

/// Makes `id` the theme of the game. Only a theme the client loads.
pub fn use_in_game(assets: &Path, id: &str) -> Result<(), String> {
    check_id(id)?;
    let p = path(assets, id);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    client_ui::load_theme(&text).map_err(|e| format!("{id}: {}", e.0))?;
    let file = assets.join(GAME_THEME_FILE);
    let old = std::fs::read_to_string(&file).unwrap_or_default();
    let body = ron::to_string(&GameTheme { theme: id.to_owned() }).map_err(|e| e.to_string())?;
    let new = format!("{}GameTheme{body}\n", head(&old));
    client_ui::read_game_theme(&new).map_err(|e| e.0)?;
    std::fs::write(&file, new).map_err(|e| format!("{}: {e}", file.display()))
}

fn dir(assets: &Path) -> PathBuf {
    assets.join("themes")
}

fn path(assets: &Path, id: &str) -> PathBuf {
    dir(assets).join(format!("{id}.ron"))
}

/// A theme id is its file name: lowercase letters, digits and `_`.
fn check_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`{id}`: a theme name uses only a-z, 0-9 and _ (at most 40)"
        ))
    }
}

/// The themes in `assets/themes/`, sorted.
fn ids(assets: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir(assets))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension()? != "ron" {
                        return None;
                    }
                    Some(p.file_stem()?.to_string_lossy().into_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The font files in `assets/fonts/`, as a theme names them.
fn fonts(assets: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(assets.join("fonts"))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
                    ["ttf", "otf", "ttc"]
                        .contains(&ext.as_str())
                        .then(|| format!("fonts/{name}"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// A number as the hand-written files spell it: `8`, `0.6`, never
/// `0.6000000238418579` (the `f32` fields come back through `f64`).
fn clean(v: Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => {
            let short = format!("{}", n.as_f64().unwrap_or_default() as f32);
            serde_json::from_str(&short).unwrap_or(Value::Number(n))
        }
        Value::Array(a) => Value::Array(a.into_iter().map(clean).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, clean(v))).collect()),
        v => v,
    }
}

fn to_json<T: serde::Serialize>(t: &T) -> Value {
    clean(serde_json::to_value(t).unwrap_or(Value::Null))
}

/// The screen's fixed data: the themes, the fonts, the colors a theme has
/// (with what an optional one falls back to) and the default numbers.
pub fn overview(assets: &Path) -> Value {
    let colors: Vec<Value> = COLORS
        .iter()
        .map(|c| json!({ "name": c.name, "fallback": c.fallback }))
        .collect();
    json!({
        "themes": ids(assets),
        "game_theme": game_theme(assets),
        "fonts": fonts(assets),
        "colors": colors,
        "defaults": {
            "panel": to_json(&Panel::default()),
            "sizes": to_json(&Sizes::default()),
            "system_window": to_json(&SystemWindow::default()),
            "windows": to_json(&WindowAnim::default()),
        },
    })
}

/// One theme, with the fields the file leaves out filled by their default.
pub fn read(assets: &Path, id: &str) -> Result<Value, String> {
    check_id(id)?;
    let p = path(assets, id);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let file: ThemeFile = ron::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    // A broken file still opens, so the screen can fix it.
    let problem = client_ui::load_theme(&text).err().map(|e| e.0);
    Ok(json!({ "theme": to_json(&file), "problem": problem }))
}

/// The comment lines on top of the file (kept when the editor rewrites it).
fn head(text: &str) -> String {
    let mut out = String::new();
    for l in text.lines() {
        if l.trim_start().starts_with("//") {
            out.push_str(l);
            out.push('\n');
        } else {
            break;
        }
    }
    out
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// `(a: 1, b: "x")`, as RON writes a struct without its name.
fn fields(v: &Value) -> String {
    let Value::Object(o) = v else {
        return "()".into();
    };
    let parts: Vec<String> = o
        .iter()
        .map(|(k, v)| {
            let v = match v {
                Value::String(s) => quote(s),
                Value::Object(_) => fields(v),
                other => other.to_string(),
            };
            format!("{k}: {v}")
        })
        .collect();
    format!("({})", parts.join(", "))
}

/// The theme as a file: one line per color and per language, in the order
/// the client documents them.
fn write(head: &str, t: &ThemeFile) -> String {
    let mut s = head.to_owned();
    s.push_str("Theme(\n");
    s.push_str(&format!("    id: {},\n", quote(&t.id)));
    s.push_str(&format!("    fonts: {},\n", fields(&to_json(&t.fonts))));
    if !t.fonts_by_locale.is_empty() {
        s.push_str("    fonts_by_locale: {\n");
        for (l, f) in &t.fonts_by_locale {
            s.push_str(&format!("        {}: {},\n", quote(l), fields(&to_json(f))));
        }
        s.push_str("    },\n");
    }
    s.push_str("    colors: {\n");
    for c in COLORS {
        if let Some(v) = t.colors.get(c.name) {
            s.push_str(&format!("        {}: {},\n", quote(c.name), quote(v)));
        }
    }
    s.push_str("    },\n");
    for (name, v) in [
        ("panel", to_json(&t.panel)),
        ("sizes", to_json(&t.sizes)),
        ("system_window", to_json(&t.system_window)),
        ("windows", to_json(&t.windows)),
    ] {
        s.push_str(&format!("    {name}: {},\n", fields(&v)));
    }
    s.push_str(")\n");
    s
}

/// Font files the theme names that are not in `assets/` (the client draws
/// with its fallback; the CJK font is downloaded apart, `fetch-cjk.sh`).
fn missing_fonts(assets: &Path, t: &ThemeFile) -> Vec<String> {
    let sets = std::iter::once(&t.fonts).chain(t.fonts_by_locale.values());
    let mut out: Vec<String> = sets
        .flat_map(|f: &FontSet| [&f.title, &f.text, &f.numbers])
        .map(|p| p.split('#').next().unwrap_or(p).to_owned())
        .filter(|p| !assets.join(p).is_file())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Saves a theme sent by the screen. The file is rewritten in the editor's
/// layout (comments inside it are lost; the ones on top stay) and must load
/// in the client before it is written.
pub fn save(assets: &Path, id: &str, theme: Value) -> Result<Value, String> {
    check_id(id)?;
    let mut t: ThemeFile = serde_json::from_value(theme).map_err(|e| e.to_string())?;
    t.id = id.to_owned();
    // An empty optional color means "use the fallback".
    t.colors.retain(|_, v| !v.trim().is_empty());
    let p = path(assets, id);
    let old = std::fs::read_to_string(&p).unwrap_or_default();
    let text = write(&head(&old), &t);
    client_ui::load_theme(&text).map_err(|e| e.0)?;
    std::fs::create_dir_all(dir(assets)).map_err(|e| e.to_string())?;
    std::fs::write(&p, &text).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(json!({ "missing_fonts": missing_fonts(assets, &t) }))
}

/// A new theme copied from another one.
pub fn create(assets: &Path, from: &str, id: &str) -> Result<(), String> {
    check_id(from)?;
    check_id(id)?;
    let p = path(assets, id);
    if p.exists() {
        return Err(format!("the theme `{id}` already exists"));
    }
    let src = path(assets, from);
    let text = std::fs::read_to_string(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut t: ThemeFile = ron::from_str(&text).map_err(|e| format!("{}: {e}", src.display()))?;
    t.id = id.to_owned();
    let head = format!("// Theme made in the editor from `{from}`. Run the client with `--theme {id}`.\n");
    let text = write(&head, &t);
    client_ui::load_theme(&text).map_err(|e| e.0)?;
    std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn assets() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara/assets")
    }

    #[test]
    fn every_theme_in_assets_rewrites_to_the_same_theme() {
        let a = assets();
        assert!(ids(&a).contains(&game_theme(&a)));
        for id in ids(&a) {
            let text = std::fs::read_to_string(path(&a, &id)).unwrap();
            let before = client_ui::load_theme(&text).unwrap();
            let file: ThemeFile = ron::from_str(&text).unwrap();
            let again = write(&head(&text), &file);
            assert_eq!(client_ui::load_theme(&again).unwrap(), before, "{id}");
            assert!(again.starts_with("//"), "{id}: the comment on top stays");
            // Through the screen's JSON and back, nothing changes either.
            let json = read(&a, &id).unwrap()["theme"].clone();
            let back: ThemeFile = serde_json::from_value(json).unwrap();
            assert_eq!(write("", &back), write("", &file), "{id}");
        }
    }

    #[test]
    fn numbers_are_written_short() {
        let v = to_json(&Panel::default());
        assert_eq!(
            fields(&v),
            "(diagonal_slash: 8, border_thickness: 1, border_glow: 0.6, background_blur: false)"
        );
    }

    #[test]
    fn save_refuses_what_the_client_refuses() {
        let dir = std::env::temp_dir().join(format!("editor-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::copy(path(&assets(), "isekai"), path(&dir, "isekai")).unwrap();
        create(&dir, "isekai", "night").unwrap();
        assert!(create(&dir, "isekai", "night").is_err());
        assert!(create(&dir, "isekai", "Bad Name").is_err());
        let mut t = read(&dir, "night").unwrap()["theme"].clone();
        t["colors"]["text"] = json!("blue");
        assert!(save(&dir, "night", t.clone()).unwrap_err().contains("text"));
        t["colors"]["text"] = json!("#ffffff");
        t["colors"]["cooldown"] = json!("");
        t["sizes"]["text"] = json!(18);
        let out = save(&dir, "night", t).unwrap();
        // No font folder here: every font is reported missing, not refused.
        assert!(!out["missing_fonts"].as_array().unwrap().is_empty());
        let saved = client_ui::load_theme(&std::fs::read_to_string(path(&dir, "night")).unwrap()).unwrap();
        assert_eq!(saved.colors.text, [1.0; 4]);
        assert!((saved.sizes.text - 18.0).abs() < f32::EPSILON);
        // The game switches to it; a theme that does not load is refused.
        assert_eq!(game_theme(&dir), FALLBACK_THEME);
        use_in_game(&dir, "night").unwrap();
        assert_eq!(game_theme(&dir), "night");
        std::fs::write(path(&dir, "broken"), "Theme(id: \"broken\")").unwrap();
        assert!(use_in_game(&dir, "broken").is_err());
        assert!(use_in_game(&dir, "nothing").is_err());
        assert_eq!(game_theme(&dir), "night");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

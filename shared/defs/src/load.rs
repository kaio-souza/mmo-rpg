use crate::Catalog;
use crate::catalog::RawZone;
use i18n::Locale;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ContentErrors {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    /// A folder under `text/` whose name is not a locale code.
    UnknownLocale {
        path: String,
    },
    /// A `.ron` file in a place no content type uses.
    BadPath {
        path: String,
    },
}

impl fmt::Display for ContentErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::UnknownLocale { path } => {
                write!(f, "{path}: the folder is not a locale code (like en or pt-BR)")
            }
            Self::BadPath { path } => write!(
                f,
                "{path}: unexpected file (expected text/<locale>/<file>.ron, text/locales.ron, zones/<zone>/zone.ron|map.ron|npcs.ron, npcs/<id>.ron, npcs/dialogues/<id>.ron or moderation/reasons.ron, appearance/appearance.ron, items/<id>.ron, quests/<id>.ron, monsters|classes|proficiencies|techniques|titles|special_attributes|loot|shops|recipes|nodes|emotes|rankings|dungeons|goals/<id>.ron, counters.ron, progression/formulas.ron, combat/formulas.ron or economy/formulas.ron; zones may have monsters.ron and nodes.ron)"
            ),
        }
    }
}

impl std::error::Error for ContentErrors {}

/// Reads every `.ron` file under `folder`. The only function in `shared/`
/// allowed to touch the file system (rule 2).
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
pub fn load(folder: &Path) -> Result<Catalog, ContentErrors> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), ContentErrors> {
        let io = |error| ContentErrors::Io {
            path: dir.to_path_buf(),
            error,
        };
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(io)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()
            .map_err(io)?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, out)?;
            // A leftover `.rhai` is still read so it fails as an unexpected
            // file instead of being ignored (scripts are gone).
            } else if path.extension().is_some_and(|e| e == "ron" || e == "rhai") {
                let text = std::fs::read_to_string(&path).map_err(|error| ContentErrors::Io {
                    path: path.clone(),
                    error,
                })?;
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let rel: Vec<String> = rel.iter().map(|c| c.to_string_lossy().into_owned()).collect();
                out.push((rel.join("/"), text));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(folder, folder, &mut files)?;
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    from_text(&refs)
}

/// Same as [`load`] from text in memory: `(path relative to content/, text)`.
pub fn from_text(files: &[(&str, &str)]) -> Result<Catalog, ContentErrors> {
    let mut sorted: Vec<(&str, &str)> = files.to_vec();
    sorted.sort();
    let mut cat = Catalog::default();
    for (path, text) in sorted {
        let bad = || ContentErrors::BadPath {
            path: path.to_owned(),
        };
        let parts: Vec<&str> = path.split('/').collect();
        match parts.as_slice() {
            ["text", locale, name] if name.ends_with(".ron") => {
                let locale: Locale = locale.parse().map_err(|_| ContentErrors::UnknownLocale {
                    path: path.to_owned(),
                })?;
                let entry = ((*name).to_owned(), text.to_owned());
                match cat.texts.iter_mut().find(|(l, _)| *l == locale) {
                    Some((_, v)) => v.push(entry),
                    None => cat.texts.push((locale, vec![entry])),
                }
            }
            ["appearance", "appearance.ron"] => cat.appearance = Some(text.to_owned()),
            ["text", "locales.ron"] => cat.locales = Some(text.to_owned()),
            ["moderation", "reasons.ron"] => {
                cat.moderation = Some(text.to_owned());
            }
            ["items", name] if name.ends_with(".ron") => {
                let id = name.trim_end_matches(".ron").to_owned();
                if cat.items.insert(id, text.to_owned()).is_some() {
                    return Err(bad());
                }
            }
            [
                dir @ ("monsters" | "classes" | "proficiencies" | "techniques" | "titles"
                | "special_attributes" | "loot" | "shops" | "recipes" | "nodes" | "emotes"
                | "rankings" | "dungeons" | "goals"),
                name,
            ] if name.ends_with(".ron") => {
                let id = name.trim_end_matches(".ron").to_owned();
                let map = match *dir {
                    "monsters" => &mut cat.monsters,
                    "classes" => &mut cat.classes,
                    "proficiencies" => &mut cat.proficiencies,
                    "techniques" => &mut cat.techniques,
                    "titles" => &mut cat.titles,
                    "loot" => &mut cat.loot_tables,
                    "shops" => &mut cat.shops,
                    "recipes" => &mut cat.recipes,
                    "nodes" => &mut cat.nodes,
                    "emotes" => &mut cat.emotes,
                    "rankings" => &mut cat.rankings,
                    "dungeons" => &mut cat.dungeons,
                    "goals" => &mut cat.goals,
                    _ => &mut cat.special_attributes,
                };
                if map.insert(id, text.to_owned()).is_some() {
                    return Err(bad());
                }
            }
            ["counters.ron"] => cat.counters = Some(text.to_owned()),
            ["progression", "formulas.ron"] => cat.progression_formulas = Some(text.to_owned()),
            ["combat", "formulas.ron"] => cat.combat_formulas = Some(text.to_owned()),
            ["economy", "formulas.ron"] => cat.economy_formulas = Some(text.to_owned()),
            ["quests", name] if name.ends_with(".ron") => {
                let id = name.trim_end_matches(".ron").to_owned();
                if cat.quests.insert(id, text.to_owned()).is_some() {
                    return Err(bad());
                }
            }
            ["npcs", "dialogues", name] if name.ends_with(".ron") => {
                let id = name.trim_end_matches(".ron").to_owned();
                if cat.dialogues.insert(id.clone(), text.to_owned()).is_some() {
                    return Err(bad());
                }
            }
            ["npcs", name] if name.ends_with(".ron") => {
                let id = name.trim_end_matches(".ron").to_owned();
                if cat.npcs.insert(id, text.to_owned()).is_some() {
                    return Err(bad());
                }
            }
            [
                "zones",
                zone,
                file @ ("zone.ron" | "map.ron" | "npcs.ron" | "monsters.ron" | "nodes.ron"),
            ] => {
                let raw = cat
                    .zones
                    .entry((*zone).to_owned())
                    .or_insert_with(RawZone::default);
                match *file {
                    "zone.ron" => raw.zone = Some(text.to_owned()),
                    "map.ron" => raw.map = Some(text.to_owned()),
                    "monsters.ron" => raw.monsters = Some(text.to_owned()),
                    "nodes.ron" => raw.nodes = Some(text.to_owned()),
                    _ => raw.npcs = Some(text.to_owned()),
                }
            }
            _ => return Err(bad()),
        }
        cat.files.push((path.to_owned(), text.to_owned()));
    }
    cat.texts.sort_by_key(|(l, _)| *l);
    Ok(cat)
}

/// File stems of every PNG under `dir` (`AssetNames`): the only other
/// place `defs` reads the disk, next to [`load`].
#[allow(clippy::disallowed_methods)]
pub(crate) fn png_names(dir: &Path) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for path in rd.flatten().map(|e| e.path()) {
        if path.is_dir() {
            out.extend(png_names(&path));
        } else if path.extension().is_some_and(|e| e == "png")
            && let Some(stem) = path.file_stem()
        {
            out.insert(stem.to_string_lossy().into_owned());
        }
    }
    out
}

/// `assets/sounds/sounds.ron` (empty when there is none) and every `.ogg`
/// under `assets/sounds/` and `assets/music/`, as paths under `assets/`
/// The third place `defs` reads the disk.
#[allow(clippy::disallowed_methods)]
pub fn read_sounds(
    assets: &Path,
) -> Result<(crate::SoundConfig, std::collections::BTreeSet<String>), String> {
    let config = match std::fs::read_to_string(assets.join("sounds").join("sounds.ron")) {
        Ok(text) => crate::SoundConfig::parse(&text)?,
        Err(_) => crate::SoundConfig::default(),
    };
    let mut files = std::collections::BTreeSet::new();
    for top in ["sounds", "music"] {
        ogg_paths(&assets.join(top), top, &mut files);
    }
    Ok((config, files))
}

#[allow(clippy::disallowed_methods)]
fn ogg_paths(dir: &Path, prefix: &str, out: &mut std::collections::BTreeSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            ogg_paths(&path, &format!("{prefix}/{name}"), out);
        } else if path.extension().is_some_and(|e| e == "ogg") {
            out.insert(format!("{prefix}/{name}"));
        }
    }
}

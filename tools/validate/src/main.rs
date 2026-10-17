//! `validate`: checks content and architecture. Exit code 1 on any error.
//!
//! ```text
//! validate content/        texts in the ten locales and definitions
//! validate content/ --assets assets/   also warns on sprite or icon names without a file
//! and checks assets/sounds/sounds.ron
//! validate content/ --allow-missing-text   a key missing in a locale is a warning, not an error
//! validate arch            crate dependencies against arch.ron
//! ```
//!
//!

mod arch;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage:\n  validate <content-folder> [--assets <assets-folder>] [--allow-missing-text]\n  validate arch [--root <workspace-folder>]";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Development only, same meaning as the server's flag: a text still to
    // be translated shows in English instead of failing the check
    let allow_missing = args.iter().any(|a| a == "--allow-missing-text");
    args.retain(|a| a != "--allow-missing-text");
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["arch"] => arch::run(Path::new(".")),
        ["arch", "--root", root] => arch::run(Path::new(root)),
        [folder] if !folder.starts_with('-') => content(&PathBuf::from(folder), None, allow_missing),
        [folder, "--assets", assets] if !folder.starts_with('-') => {
            content(&PathBuf::from(folder), Some(Path::new(assets)), allow_missing)
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn content(folder: &Path, assets: Option<&Path>, allow_missing: bool) -> Result<(), String> {
    let mut catalog = defs::load(folder).map_err(|e| format!("error: {e}"))?;
    let mut filled = Vec::new();
    if allow_missing {
        filled = catalog.fill_missing_text();
    }
    match catalog.validate() {
        Ok(c) => {
            for w in c.warnings() {
                println!("{w}");
            }
            for (locale, key) in &filled {
                println!("warning: [{locale}] `{key}`: missing translation, shown in English");
            }
            for w in assets
                .map(|a| defs::AssetNames::read(a).missing(&c))
                .unwrap_or_default()
            {
                println!("{w}");
            }
            if let Some(a) = assets {
                sounds(a, &c)?;
            }
            println!(
                "content ok: {} text keys in {} locales, {} NPCs, {} items, {} quests, hash {:016x}",
                c.keys().len(),
                c.locales().len(),
                c.npcs().count(),
                c.items().len(),
                c.quests().len(),
                c.content_hash()
            );
            if !filled.is_empty() {
                println!(
                    "{} text(s) still to translate (--allow-missing-text)",
                    filled.len()
                );
            }
            Ok(())
        }
        Err(problems) => {
            for p in &problems {
                println!("{p}");
            }
            Err(format!("content has {} problem(s)", problems.len()))
        }
    }
}

/// `assets/sounds/sounds.ron`: errors fail, missing files warn;
/// techniques and zones must exist in the content.
fn sounds(assets: &Path, catalog: &defs::ValidatedCatalog) -> Result<(), String> {
    let (config, files) = defs::read_sounds(assets).map_err(|e| format!("error: {e}"))?;
    let mut errs = config.check();
    errs.extend(config.check_refs(
        catalog.techniques().map(|(_, t)| t.id.as_str()),
        catalog.zones().map(|(_, z)| z.name.as_str()),
    ));
    if !errs.is_empty() {
        for e in &errs {
            println!("error: {e}");
        }
        return Err(format!("sounds.ron has {} problem(s)", errs.len()));
    }
    for w in config.missing_files(&files) {
        println!("{w}");
    }
    Ok(())
}

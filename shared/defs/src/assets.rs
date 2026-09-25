//! Names of the images in the assets folder:
//! every PNG under `sprites/` is a sprite and every PNG under `icons/` an
//! icon, named by file stem. Content calls art by these names; a name with
//! no file is drawn with the flat fallback, so it is a warning, not an error.

use crate::ValidatedCatalog;
use std::collections::BTreeSet;
use std::path::Path;

/// Sprite and icon names found on disk.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AssetNames {
    pub sprites: BTreeSet<String>,
    pub icons: BTreeSet<String>,
}

impl AssetNames {
    /// Reads `<dir>/sprites/**.png` and `<dir>/icons/**.png`; a missing
    /// folder is empty.
    pub fn read(dir: &Path) -> AssetNames {
        AssetNames {
            sprites: crate::load::png_names(&dir.join("sprites")),
            icons: crate::load::png_names(&dir.join("icons")),
        }
    }

    /// One warning per sprite or icon name the content uses without a file.
    pub fn missing(&self, catalog: &ValidatedCatalog) -> Vec<String> {
        let sprites = catalog.sprites().iter().filter(|n| !self.sprites.contains(*n));
        let icons = catalog.icons().iter().filter(|n| !self.icons.contains(*n));
        sprites
            .map(|n| format!("warning: sprite `{n}` has no file in assets/sprites/"))
            .chain(icons.map(|n| format!("warning: icon `{n}` has no file in assets/icons/")))
            .chain(self.erased(catalog))
            .collect()
    }

    /// P04: content left with the generic icon while `assets/icons/<id>.png`
    /// exists: the name was probably erased by mistake.
    fn erased(&self, catalog: &ValidatedCatalog) -> Vec<String> {
        catalog
            .without_icon()
            .into_iter()
            .filter(|(_, id)| self.icons.contains(*id))
            .map(|(folder, id)| {
                format!("warning: {folder}/{id}.ron: icon is empty but assets/icons/{id}.png exists")
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_real_content_has_all_its_art() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/riftara");
        let names = AssetNames::read(&root.join("assets"));
        assert!(names.sprites.contains("wolf") && names.icons.contains("generic_item"));
        // Translations still to be written are not this test's business
        // (`validate content/` checks those).
        let mut raw = crate::load(&root.join("content")).unwrap();
        let _ = raw.fill_missing_text();
        let catalog = raw.validate().unwrap();
        assert_eq!(names.missing(&catalog), Vec::<String>::new());
    }
}

//! TTF fonts and the glyph atlas. A font path may name a face of a
//! collection: `fonts/NotoSansCJK-Regular.ttc#2`. Each role (title, text,
//! numbers) has a fallback chain: the locale font, the base font of the
//! role, the base text font; a character takes the first font that has it.
//!
//! Glyphs are rasterized once per (font, char, size) into an `R8` atlas
//! that the binary hands to `client-render` when it changes.

use crate::theme::{FontSet, UiTheme};
use std::collections::BTreeMap;

/// Side of the square atlas, in pixels.
pub const ATLAS_SIZE: u32 = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Title,
    Text,
    Numbers,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Glyph {
    /// Position in the atlas, pixels.
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) xmin: f32,
    pub(crate) ymin: f32,
    pub(crate) advance: f32,
}

pub struct Fonts {
    faces: Vec<fontdue::Font>,
    /// Face indexes per role, best first.
    chains: BTreeMap<Role, Vec<usize>>,
    cache: BTreeMap<(usize, char, u32), Glyph>,
    atlas: Vec<u8>,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
    dirty: bool,
}

fn split_path(path: &str) -> (&str, u32) {
    match path.rsplit_once('#') {
        Some((p, i)) => (p, i.parse().unwrap_or(0)),
        None => (path, 0),
    }
}

impl Fonts {
    /// `read` gives the bytes of a file under `assets/`. Fonts that fail to
    /// load are skipped and reported; with none at all, text is not drawn.
    pub fn load(
        theme: &UiTheme,
        locale: &str,
        read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    ) -> (Fonts, Vec<String>) {
        let mut problems = Vec::new();
        let mut faces = Vec::new();
        let mut by_path: BTreeMap<String, Option<usize>> = BTreeMap::new();
        let mut face = |path: &str, problems: &mut Vec<String>| -> Option<usize> {
            if let Some(i) = by_path.get(path) {
                return *i;
            }
            let (file, index) = split_path(path);
            let loaded = read(file).and_then(|bytes| {
                let settings = fontdue::FontSettings {
                    collection_index: index,
                    ..fontdue::FontSettings::default()
                };
                match fontdue::Font::from_bytes(bytes, settings) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        problems.push(format!("font `{path}`: {e}"));
                        None
                    }
                }
            });
            if loaded.is_none() && !problems.iter().any(|p| p.contains(path)) {
                problems.push(format!("font `{path}` not found"));
            }
            let i = loaded.map(|f| {
                faces.push(f);
                faces.len() - 1
            });
            by_path.insert(path.to_owned(), i);
            i
        };
        let local: &FontSet = theme.fonts_for(locale);
        let base = &theme.fonts;
        let own = [&local.text, &local.title, &base.text, &base.title];
        let other_script = theme
            .locale_text_fonts()
            .find(|p| own.iter().all(|q| split_path(q).0 != split_path(p).0))
            .map(str::to_owned);
        let mut chains = BTreeMap::new();
        for (role, pick) in [
            (
                Role::Title,
                (|s: &FontSet| s.title.clone()) as fn(&FontSet) -> String,
            ),
            (Role::Text, |s: &FontSet| s.text.clone()),
            (Role::Numbers, |s: &FontSet| s.numbers.clone()),
        ] {
            let mut chain = Vec::new();
            for path in [pick(local), pick(base), local.text.clone(), base.text.clone()] {
                if let Some(i) = face(&path, &mut problems)
                    && !chain.contains(&i)
                {
                    chain.push(i);
                }
            }
            // Last, one face of another script (CJK) so a language name
            // such as 日本語 reads in any locale. Missing is not a problem:
            // that font is downloaded apart.
            if let Some(path) = &other_script
                && let Some(i) = face(path, &mut Vec::new())
                && !chain.contains(&i)
            {
                chain.push(i);
            }
            chains.insert(role, chain);
        }
        let fonts = Fonts {
            faces,
            chains,
            cache: BTreeMap::new(),
            atlas: vec![0; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            shelf_x: 1,
            shelf_y: 1,
            shelf_h: 0,
            dirty: true,
        };
        (fonts, problems)
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    fn face_for(&self, role: Role, c: char) -> Option<usize> {
        let chain = self.chains.get(&role)?;
        chain
            .iter()
            .copied()
            .find(|&i| self.faces[i].lookup_glyph_index(c) != 0)
            .or_else(|| chain.first().copied())
    }

    /// Height of a line at `px`.
    pub fn line_height(&self, role: Role, px: f32) -> f32 {
        self.chains
            .get(&role)
            .and_then(|c| c.first())
            .and_then(|&i| self.faces[i].horizontal_line_metrics(px))
            .map_or(px * 1.25, |m| m.new_line_size)
    }

    /// Distance from the top of a line to its baseline.
    pub fn ascent(&self, role: Role, px: f32) -> f32 {
        self.chains
            .get(&role)
            .and_then(|c| c.first())
            .and_then(|&i| self.faces[i].horizontal_line_metrics(px))
            .map_or(px, |m| m.ascent)
    }

    pub(crate) fn glyph(&mut self, role: Role, c: char, px: f32) -> Option<Glyph> {
        let face = self.face_for(role, c)?;
        let size = (px * 4.0).round() as u32; // quarter pixels
        if let Some(g) = self.cache.get(&(face, c, size)) {
            return Some(*g);
        }
        let (m, bitmap) = self.faces[face].rasterize(c, size as f32 / 4.0);
        let (w, h) = (m.width as u32, m.height as u32);
        let place = match self.place(w, h) {
            Some(p) => p,
            None => {
                // Full: start over (glyphs come back as they are drawn).
                self.cache.clear();
                self.atlas.iter_mut().for_each(|b| *b = 0);
                (self.shelf_x, self.shelf_y, self.shelf_h) = (1, 1, 0);
                self.place(w, h)?
            }
        };
        for row in 0..h {
            let src = &bitmap[(row * w) as usize..((row + 1) * w) as usize];
            let at = ((place.1 + row) * ATLAS_SIZE + place.0) as usize;
            self.atlas[at..at + w as usize].copy_from_slice(src);
        }
        self.dirty |= w > 0 && h > 0;
        let g = Glyph {
            x: place.0,
            y: place.1,
            w,
            h,
            xmin: m.xmin as f32,
            ymin: m.ymin as f32,
            advance: m.advance_width,
        };
        self.cache.insert((face, c, size), g);
        Some(g)
    }

    fn place(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w + 2 > ATLAS_SIZE || h + 2 > ATLAS_SIZE {
            return None;
        }
        if self.shelf_x + w + 1 > ATLAS_SIZE {
            self.shelf_y += self.shelf_h + 1;
            self.shelf_x = 1;
            self.shelf_h = 0;
        }
        if self.shelf_y + h + 1 > ATLAS_SIZE {
            return None;
        }
        let p = (self.shelf_x, self.shelf_y);
        self.shelf_x += w + 1;
        self.shelf_h = self.shelf_h.max(h);
        Some(p)
    }

    /// Width of `text` on one line.
    pub fn measure(&mut self, role: Role, px: f32, text: &str) -> f32 {
        text.chars()
            .filter_map(|c| self.glyph(role, c, px))
            .map(|g| g.advance)
            .sum()
    }

    /// Makes the next [`Fonts::take_atlas`] return the atlas again (the GPU
    /// copy was lost).
    pub fn invalidate_atlas(&mut self) {
        self.dirty = true;
    }

    /// The atlas once after it changed: `(size, pixels)`.
    pub fn take_atlas(&mut self) -> Option<(u32, &[u8])> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        Some((ATLAS_SIZE, &self.atlas))
    }

    /// Breaks `text` into lines no wider than `width` (at spaces; a word
    /// wider than the line, or text without spaces as in CJK, breaks
    /// anywhere).
    pub fn wrap(&mut self, role: Role, px: f32, text: &str, width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            let mut line_w = 0.0;
            let mut last_space: Option<usize> = None;
            for c in paragraph.chars() {
                let adv = self.glyph(role, c, px).map_or(0.0, |g| g.advance);
                if line_w + adv > width && !line.is_empty() {
                    match last_space {
                        Some(at) if c != ' ' => {
                            let rest = line.split_off(at);
                            lines.push(line);
                            line = rest.trim_start().to_owned();
                            line_w = self.measure(role, px, &line);
                        }
                        _ => {
                            lines.push(std::mem::take(&mut line));
                            line_w = 0.0;
                        }
                    }
                    last_space = None;
                    if c == ' ' {
                        continue;
                    }
                }
                if c == ' ' {
                    last_space = Some(line.len());
                }
                line.push(c);
                line_w += adv;
            }
            lines.push(line);
        }
        lines
    }
}

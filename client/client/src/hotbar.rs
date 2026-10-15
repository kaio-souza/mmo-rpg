//! The player's own hotbar layout: which technique or consumable sits in
//! each of the ten slots. It lives on this computer only
//! (`hotbar-<character>.ron` in the user folder), like the key map; the
//! server never sees it. New techniques and new consumables take the first
//! empty slot once; after that only the player moves them.

use client_state::WorldState;
use std::path::PathBuf;
use tracing::warn;

pub const SLOTS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Bind {
    Technique(u32),
    /// An item kind: the bar uses whichever backpack slot holds it.
    Item(u32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Layout {
    pub slots: [Option<Bind>; SLOTS],
    /// Everything that was ever placed automatically: removed by the player,
    /// it does not come back on its own.
    pub seen: Vec<Bind>,
}

/// The layout of the character in play, saved when it changes.
#[derive(Debug, Default)]
pub struct Hotbar {
    pub layout: Layout,
    owner: Option<String>,
    dirty: bool,
}

fn path(owner: &str) -> Option<PathBuf> {
    crate::interface::config_dir().map(|d| d.join(format!("hotbar-{owner}.ron")))
}

impl Hotbar {
    pub fn get(&self, i: usize) -> Option<Bind> {
        self.layout.slots.get(i).copied().flatten()
    }

    /// Loads the layout of `owner` the first time it is seen in play.
    pub fn select(&mut self, owner: &str) {
        if self.owner.as_deref() == Some(owner) {
            return;
        }
        self.save();
        self.owner = Some(owner.to_owned());
        self.layout = path(owner)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| {
                ron::from_str(&t)
                    .map_err(|e| warn!(owner, "hotbar file: {e}"))
                    .ok()
            })
            .unwrap_or_default();
    }

    /// New techniques and consumables go to the first empty slot, once.
    pub fn sync(&mut self, state: &WorldState) {
        if self.owner.is_none() {
            return;
        }
        // A consumable that ran out leaves the bar, and is forgotten so that
        // the next one found takes a slot again. Only once the backpack is
        // known, or a fresh login would empty the bar.
        if let Some(inv) = state.inventory() {
            for slot in &mut self.layout.slots {
                let Some(Bind::Item(id)) = *slot else { continue };
                let left: u64 = inv
                    .slots
                    .iter()
                    .flatten()
                    .filter(|h| h.item.get() == id)
                    .map(|h| u64::from(h.qty))
                    .sum();
                if left == 0 {
                    self.layout.seen.retain(|b| *b != Bind::Item(id));
                    *slot = None;
                    self.dirty = true;
                }
            }
        }
        let mut fresh: Vec<Bind> = Vec::new();
        if let Some(s) = state.stats() {
            fresh.extend(s.techniques.iter().map(|(t, _)| Bind::Technique(t.get())));
        }
        if let Some(inv) = state.inventory() {
            for held in inv.slots.iter().flatten() {
                let usable = state.item(held.item).is_some_and(|d| d.use_effect.is_some());
                let b = Bind::Item(held.item.get());
                if usable && !fresh.contains(&b) {
                    fresh.push(b);
                }
            }
        }
        for b in fresh {
            if self.layout.seen.contains(&b) {
                continue;
            }
            self.layout.seen.push(b);
            if self.layout.slots.contains(&Some(b)) {
                continue;
            }
            if let Some(free) = self.layout.slots.iter_mut().find(|s| s.is_none()) {
                *free = Some(b);
            }
            self.dirty = true;
        }
        self.save();
    }

    /// Puts `b` in `slot`. If it was in another slot, what was in `slot`
    /// goes there (a swap); otherwise what was in `slot` leaves the bar.
    pub fn place(&mut self, slot: usize, b: Bind) {
        if slot >= SLOTS {
            return;
        }
        let old = self.layout.slots[slot];
        if let Some(from) = self.layout.slots.iter().position(|s| *s == Some(b)) {
            self.layout.slots[from] = old;
        }
        self.layout.slots[slot] = Some(b);
        self.dirty = true;
        self.save();
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a < SLOTS && b < SLOTS {
            self.layout.slots.swap(a, b);
            self.dirty = true;
            self.save();
        }
    }

    pub fn clear(&mut self, slot: usize) {
        if let Some(s) = self.layout.slots.get_mut(slot) {
            *s = None;
            self.dirty = true;
            self.save();
        }
    }

    /// The first empty slot, if any.
    pub fn free(&self) -> Option<usize> {
        self.layout.slots.iter().position(Option::is_none)
    }

    pub fn contains(&self, b: Bind) -> bool {
        self.layout.slots.contains(&Some(b))
    }

    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let Some(p) = self.owner.as_deref().and_then(path) else {
            return;
        };
        let text = match ron::ser::to_string_pretty(&self.layout, ron::ser::PrettyConfig::default()) {
            Ok(t) => t,
            Err(e) => return warn!("hotbar file: {e}"),
        };
        let written = p
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&p, text));
        if let Err(e) = written {
            warn!(path = %p.display(), "cannot save the hotbar: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> Hotbar {
        // No owner: nothing is written to disk.
        Hotbar::default()
    }

    #[test]
    fn place_swaps_when_already_in_the_bar() {
        let mut h = bar();
        h.layout.slots[0] = Some(Bind::Technique(1));
        h.layout.slots[1] = Some(Bind::Item(7));
        h.place(1, Bind::Technique(1));
        assert_eq!(h.get(0), Some(Bind::Item(7)));
        assert_eq!(h.get(1), Some(Bind::Technique(1)));
    }

    #[test]
    fn place_from_outside_replaces() {
        let mut h = bar();
        h.layout.slots[3] = Some(Bind::Item(7));
        h.place(3, Bind::Technique(2));
        assert_eq!(h.get(3), Some(Bind::Technique(2)));
        assert!(!h.contains(Bind::Item(7)));
        h.swap(3, 9);
        assert_eq!(h.get(9), Some(Bind::Technique(2)));
        h.clear(9);
        assert_eq!(h.free(), Some(0));
        assert!(h.layout.slots.iter().all(Option::is_none));
    }
}

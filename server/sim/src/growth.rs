//! A player's progression inside the zone (phase 4): the `progression`
//! sheet with content ids, the declared counters and technique
//! cooldowns. Text ids at the edges, like `PlayerData`.

use crate::types::SavedProgress;
use base::{ClassId, TechniqueId, TitleId};
use defs::{Attributes, ValidatedCatalog};
use progression::{Proficiency, ProgressionSheet, Technique};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Growth {
    pub sheet: ProgressionSheet,
    /// `(family, key, value)`, sorted; only declared families.
    pub counters: Vec<(String, String, u64)>,
    /// Ready tick per technique, sorted.
    pub cooldowns: Vec<(TechniqueId, u64)>,
    /// Saved entries the running content does not have.
    pub kept: SavedProgress,
    /// Unique titles and classes another character of the server holds
    /// (`Command::Taken`), sorted: out of reach.
    #[serde(default)]
    pub taken_titles: Vec<TitleId>,
    #[serde(default)]
    pub taken_classes: Vec<ClassId>,
}

impl Growth {
    /// Attributes in use: the sheet, the equipment and every owned title
    /// Titles and classes look at the sheet alone.
    pub fn attributes(&self, gear: &Attributes, cat: &ValidatedCatalog) -> Attributes {
        self.sheet
            .attributes
            .plus(gear)
            .plus(&progression::title_bonuses(&self.sheet, cat).attributes)
    }

    pub fn import(s: &SavedProgress, cat: &ValidatedCatalog) -> Growth {
        let mut sheet = ProgressionSheet::new(cat);
        let mut kept = SavedProgress::default();
        if s.level > 0 {
            sheet.level = s.level;
            sheet.xp = s.xp;
            sheet.max_level = s.max_level.max(s.level);
            sheet.attributes = s.attributes;
            sheet.free_points = s.free_points;
            sheet.karma = s.karma.clamp(-defs::KARMA_MAX, defs::KARMA_MAX);
            match s.class.as_deref().map(|c| (c, cat.class_by_name(c))) {
                Some((_, Some((id, _)))) => sheet.class = Some(id),
                Some((name, None)) => kept.class = Some(name.to_owned()),
                None => {}
            }
        }
        for (name, points, xp) in &s.proficiencies {
            match cat.proficiency_by_name(name) {
                Some((id, _)) => sheet.proficiencies.push(Proficiency {
                    id,
                    points: *points,
                    xp: *xp,
                    last_gain: 0,
                }),
                None => kept.proficiencies.push((name.clone(), *points, *xp)),
            }
        }
        sheet.proficiencies.sort_by_key(|p| p.id);
        for (name, level, uses) in &s.techniques {
            match cat.technique_by_name(name) {
                Some((id, _)) => sheet.techniques.push(Technique {
                    id,
                    level: *level,
                    uses: *uses,
                }),
                None => kept.techniques.push((name.clone(), *level, *uses)),
            }
        }
        sheet.techniques.sort_by_key(|t| t.id);
        for name in &s.titles {
            match cat.title_by_name(name) {
                Some((id, _)) => sheet.titles.push(id),
                None => kept.titles.push(name.clone()),
            }
        }
        sheet.titles.sort();
        // Counters of families no longer declared are kept, not counted.
        let mut counters = Vec::new();
        for c in &s.counters {
            if cat.counter_families().contains(&c.0) {
                counters.push(c.clone());
            } else {
                kept.counters.push(c.clone());
            }
        }
        counters.sort();
        Growth {
            sheet,
            counters,
            cooldowns: Vec::new(),
            kept,
            taken_titles: Vec::new(),
            taken_classes: Vec::new(),
        }
    }

    pub fn export(&self, cat: &ValidatedCatalog) -> SavedProgress {
        let s = &self.sheet;
        let mut out = SavedProgress {
            level: s.level,
            xp: s.xp,
            max_level: s.max_level,
            attributes: s.attributes,
            free_points: s.free_points,
            class: s
                .class
                .and_then(|c| cat.class(c))
                .map(|c| c.id.clone())
                .or_else(|| self.kept.class.clone()),
            proficiencies: s
                .proficiencies
                .iter()
                .filter_map(|p| Some((cat.proficiency(p.id)?.id.clone(), p.points, p.xp)))
                .collect(),
            techniques: s
                .techniques
                .iter()
                .filter_map(|t| Some((cat.technique(t.id)?.id.clone(), t.level, t.uses)))
                .collect(),
            titles: s
                .titles
                .iter()
                .filter_map(|t| Some(cat.title(*t)?.id.clone()))
                .collect(),
            counters: self.counters.clone(),
            karma: s.karma,
        };
        out.proficiencies.extend(self.kept.proficiencies.iter().cloned());
        out.techniques.extend(self.kept.techniques.iter().cloned());
        out.titles.extend(self.kept.titles.iter().cloned());
        out.counters.extend(self.kept.counters.iter().cloned());
        out.proficiencies.sort();
        out.techniques.sort();
        out.titles.sort();
        out.counters.sort();
        out
    }

    /// Adds to a counter when its family is declared.
    pub fn count(&mut self, cat: &ValidatedCatalog, family: &str, key: &str, n: u64) {
        if !cat.counter_families().iter().any(|f| f == family) {
            return;
        }
        let probe = (family.to_owned(), key.to_owned());
        match self
            .counters
            .binary_search_by(|(f, k, _)| (f.as_str(), k.as_str()).cmp(&(probe.0.as_str(), probe.1.as_str())))
        {
            Ok(i) => {
                if let Some(c) = self.counters.get_mut(i) {
                    c.2 = c.2.saturating_add(n);
                }
            }
            Err(i) => self.counters.insert(i, (probe.0, probe.1, n)),
        }
    }

    /// Titles earned now. Called wherever something a trigger reads can
    /// change: a kill (level, counters), spent points (attributes) and a
    /// proficiency going up.
    pub fn check_titles(&mut self, cat: &ValidatedCatalog) -> Vec<progression::Change> {
        let counters = &self.counters;
        progression::check_titles_except(
            &mut self.sheet,
            |f, k| counter_of(counters, f, k),
            cat,
            &self.taken_titles,
        )
    }

    /// Sets what other characters hold from `title:<id>` and `class:<id>`
    /// keys (unknown ids are ignored).
    pub fn set_taken(&mut self, keys: &[String], cat: &ValidatedCatalog) {
        self.taken_titles = keys
            .iter()
            .filter_map(|k| cat.title_by_name(k.strip_prefix("title:")?).map(|(id, _)| id))
            .collect();
        self.taken_titles.sort();
        self.taken_classes = keys
            .iter()
            .filter_map(|k| cat.class_by_name(k.strip_prefix("class:")?).map(|(id, _)| id))
            .collect();
        self.taken_classes.sort();
    }

    /// Takes away a unique title or class another character holds;
    /// `true` when the sheet had it.
    pub fn revoke(&mut self, key: &str, cat: &ValidatedCatalog) -> bool {
        if let Some((id, _)) = key.strip_prefix("title:").and_then(|t| cat.title_by_name(t)) {
            let had = self.sheet.titles.contains(&id);
            self.sheet.titles.retain(|t| *t != id);
            return had;
        }
        if let Some((id, _)) = key.strip_prefix("class:").and_then(|c| cat.class_by_name(c))
            && self.sheet.class == Some(id)
        {
            // Back to the class it specialized, or to none.
            self.sheet.class = cat.class(id).and_then(|c| c.parent);
            return true;
        }
        false
    }

    pub fn ready_at(&self, t: TechniqueId) -> u64 {
        self.cooldowns
            .iter()
            .find(|(id, _)| *id == t)
            .map_or(0, |(_, r)| *r)
    }

    pub fn set_cooldown(&mut self, t: TechniqueId, ready: u64) {
        match self.cooldowns.binary_search_by_key(&t, |(id, _)| *id) {
            Ok(i) => {
                if let Some(c) = self.cooldowns.get_mut(i) {
                    c.1 = ready;
                }
            }
            Err(i) => self.cooldowns.insert(i, (t, ready)),
        }
    }
}

/// A counter; `key` empty sums the family.
fn counter_of(counters: &[(String, String, u64)], family: &str, key: &str) -> u64 {
    counters
        .iter()
        .filter(|(f, k, _)| f == family && (key.is_empty() || k == key))
        .map(|(_, _, v)| *v)
        .sum()
}

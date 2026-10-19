//! Phase 4 reports: the economy snapshot
//! and the bot suspects. Pure functions over rows read from the database,
//! so they are tested without one.

use base::{CharacterId, CharacterName};
use persistence::{CoinFlowRow, SavedSheet, TelemetryRow};
use std::collections::BTreeMap;

/// What every character holds right now.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Economy {
    pub characters: usize,
    /// Copper held by all characters.
    pub coins: u64,
    /// Item text id: (units in stacks, items with identity).
    pub items: BTreeMap<String, (u64, u64)>,
    /// Level: characters at it.
    pub levels: BTreeMap<u8, usize>,
    /// The richest characters, most copper first (at most 10).
    pub richest: Vec<(CharacterName, u64)>,
}

pub fn economy(sheets: &[(CharacterId, CharacterName, SavedSheet)]) -> Economy {
    let mut e = Economy {
        characters: sheets.len(),
        ..Economy::default()
    };
    for (_, name, s) in sheets {
        e.coins = e.coins.saturating_add(s.coins);
        for stack in s.backpack.iter().flatten() {
            e.items.entry(stack.item.clone()).or_default().0 += u64::from(stack.qty);
        }
        for i in &s.items {
            e.items.entry(i.item.clone()).or_default().1 += 1;
        }
        *e.levels.entry(s.progress.level.max(1)).or_default() += 1;
        e.richest.push((name.clone(), s.coins));
    }
    e.richest
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));
    e.richest.truncate(10);
    e
}

/// Coins created and destroyed over a period, by source and sink (phase
/// 7). The goal of the
/// design: destroyed per day >= 90% of created.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoinFlow {
    pub created: u64,
    pub destroyed: u64,
    /// Source or sink name: copper.
    pub sources: BTreeMap<String, u64>,
    pub sinks: BTreeMap<String, u64>,
}

impl CoinFlow {
    /// Destroyed as a share of created, in thousandths (`None`: nothing
    /// was created, so there is no ratio to show).
    pub fn sink_permille(&self) -> Option<u64> {
        (self.created > 0).then(|| self.destroyed.saturating_mul(1000) / self.created)
    }
}

pub fn coin_flow(rows: &[CoinFlowRow]) -> CoinFlow {
    let mut f = CoinFlow::default();
    for r in rows {
        let (total, by_name) = if r.created {
            (&mut f.created, &mut f.sources)
        } else {
            (&mut f.destroyed, &mut f.sinks)
        };
        *total = total.saturating_add(r.amount);
        let e = by_name.entry(r.flow.clone()).or_default();
        *e = e.saturating_add(r.amount);
    }
    f
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod coin_tests {
    use super::*;

    fn row(flow: &str, created: bool, amount: u64) -> CoinFlowRow {
        CoinFlowRow {
            at: 10,
            flow: flow.into(),
            created,
            amount,
        }
    }

    #[test]
    fn sources_and_sinks_are_told_apart() {
        let f = coin_flow(&[
            row("monster_loot", true, 100),
            row("quest_reward", true, 300),
            row("shop_purchase", false, 200),
            row("repair", false, 160),
        ]);
        assert_eq!((f.created, f.destroyed), (400, 360));
        assert_eq!(f.sources.get("quest_reward"), Some(&300));
        assert_eq!(f.sinks.get("repair"), Some(&160));
        // 360 of 400 destroyed: 90%, the goal of the design.
        assert_eq!(f.sink_permille(), Some(900));
        assert_eq!(coin_flow(&[]).sink_permille(), None);
    }
}

/// Limits above which a character is listed for human review. Never an
/// automatic sanction.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub kills_per_hour: u64,
    /// Longest play without a pause, in seconds.
    pub session_secs: u64,
}

/// A pause: two samples further apart than this (saves are every 30 s).
pub const PAUSE_SECS: u64 = 10 * 60;

#[derive(Debug, PartialEq, Eq)]
pub struct Suspect {
    pub character: CharacterId,
    pub kills_per_hour: u64,
    pub longest_session_secs: u64,
    pub levels_gained: u8,
}

/// Rows must come ordered by character, then time (as the query returns).
pub fn suspects(rows: &[TelemetryRow], limits: Limits) -> Vec<Suspect> {
    let mut out = Vec::new();
    for chunk in rows.chunk_by(|a, b| a.character == b.character) {
        let (first, last) = (&chunk[0], &chunk[chunk.len() - 1]);
        let mut played = 0u64;
        let mut session = 0u64;
        let mut longest = 0u64;
        for w in chunk.windows(2) {
            let gap = w[1].at.saturating_sub(w[0].at);
            if gap > PAUSE_SECS {
                session = 0;
            } else {
                session += gap;
                played += gap;
            }
            longest = longest.max(session);
        }
        let kills = last.kills.saturating_sub(first.kills);
        // At least ten minutes of play before a rate means anything.
        let kills_per_hour = if played >= PAUSE_SECS {
            kills * 3600 / played
        } else {
            0
        };
        if kills_per_hour > limits.kills_per_hour || longest > limits.session_secs {
            out.push(Suspect {
                character: first.character,
                kills_per_hour,
                longest_session_secs: longest,
                levels_gained: last.level.saturating_sub(first.level),
            });
        }
    }
    out.sort_by(|a, b| {
        b.kills_per_hour
            .cmp(&a.kills_per_hour)
            .then(b.longest_session_secs.cmp(&a.longest_session_secs))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence::{SavedItem, SavedStack};

    fn row(id: u32, at: u64, kills: u64) -> TelemetryRow {
        TelemetryRow {
            character: CharacterId::new(id),
            at,
            level: 1,
            kills,
        }
    }

    #[test]
    fn economy_counts_coins_stacks_and_identity_items() {
        let a = SavedSheet {
            coins: 50,
            backpack: vec![
                Some(SavedStack {
                    item: "wolf_pelt".into(),
                    qty: 3,
                }),
                None,
            ],
            items: vec![SavedItem {
                id: 9,
                item: "rusty_sword".into(),
                equipped: Some(0),
                slot: 0,
                hidden: vec![],
                appraised: false,
                durability: (80, 80),
                crafter: None,
                rarity: None,
                mastery: 0,
                power: None,
            }],
            ..SavedSheet::default()
        };
        let b = SavedSheet {
            coins: 70,
            backpack: vec![Some(SavedStack {
                item: "wolf_pelt".into(),
                qty: 2,
            })],
            ..SavedSheet::default()
        };
        let n = |s: &str| CharacterName::try_from(s).unwrap();
        let e = economy(&[
            (CharacterId::new(1), n("Alfa"), a),
            (CharacterId::new(2), n("Bravo"), b),
        ]);
        assert_eq!(e.characters, 2);
        assert_eq!(e.coins, 120);
        assert_eq!(e.items["wolf_pelt"], (5, 0));
        assert_eq!(e.items["rusty_sword"], (0, 1));
        assert_eq!(e.levels[&1], 2);
        assert_eq!(e.richest[0], (n("Bravo"), 70));
    }

    #[test]
    fn fast_killers_and_endless_sessions_are_listed() {
        let limits = Limits {
            kills_per_hour: 300,
            session_secs: 8 * 3600,
        };
        let mut rows = Vec::new();
        // 1: 30 minutes, 400 kills: 800/h.
        for i in 0..=60 {
            rows.push(row(1, i * 30, i * 400 / 60));
        }
        // 2: calm player, a pause in the middle.
        for i in 0..=60 {
            rows.push(row(2, i * 30, i / 6));
        }
        rows.push(row(2, 5000, 20));
        // 3: nine hours without a pause.
        for i in 0..=(9 * 120) {
            rows.push(row(3, i * 30, 0));
        }
        let s = suspects(&rows, limits);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].character, CharacterId::new(1));
        assert_eq!(s[0].kills_per_hour, 800);
        assert_eq!(s[1].character, CharacterId::new(3));
        assert_eq!(s[1].longest_session_secs, 9 * 3600);
    }
}

//! Unique titles and classes: only one character of the server holds each
//! (`unique_holders`). Zones cannot see each other, so the gateway decides:
//!
//! - A zone says a player just got one (`FromZone::Claims`); the gateway
//!   claims it in the database. When another character already holds it,
//!   the zone takes it back (`Command::Revoke`).
//! - Every player in the game knows what others hold (`Command::Taken`),
//!   so it is out of their reach; sent on entering and on every change.
//! - A character that enters with one claims it again: it may have passed
//!   to an heir while it was away.
//! - Once an hour, holders who have not played for the content's
//!   `heir_after_days` lose the row; the next character to earn it holds it.

use super::{Done, Gateway};
use crate::login;
use base::{CharacterId, PlayerId, ZoneId};
use sim::Command;
use std::collections::BTreeMap;
use tracing::{info, warn};

/// How often holders who stopped playing are checked.
pub(crate) const HEIR_CHECK_SECS: u64 = 3600;

/// `title:<id>` and `class:<id>` of every unique title and class of the
/// content, with its days before an heir may take it (0: never).
pub(crate) fn unique_keys(cat: &defs::ValidatedCatalog) -> BTreeMap<String, u16> {
    let titles = cat
        .titles()
        .filter_map(|(_, t)| Some((format!("title:{}", t.id), t.unique?.heir_after_days)));
    let classes = cat
        .classes()
        .filter_map(|(_, c)| Some((format!("class:{}", c.id), c.unique?.heir_after_days)));
    // A unique item never passes to an heir: it exists until it breaks
    // or is sold.
    let items = cat
        .items()
        .iter()
        .filter(|d| d.unique)
        .map(|d| (format!("item:{}", d.id), 0));
    titles.chain(classes).chain(items).collect()
}

/// The unique keys a saved sheet has.
pub(crate) fn keys_of(sheet: &sim::CharacterSheet, unique: &BTreeMap<String, u16>) -> Vec<String> {
    let p = &sheet.progress;
    p.titles
        .iter()
        .map(|t| format!("title:{t}"))
        .chain(p.class.iter().map(|c| format!("class:{c}")))
        .filter(|k| unique.contains_key(k))
        .collect()
}

/// Holders whose heir time has come: not online, unseen for longer than
/// the days of their key.
pub(crate) fn expired(
    holders: &[persistence::UniqueHolder],
    unique: &BTreeMap<String, u16>,
    online: &[CharacterId],
    now: u64,
) -> Vec<(String, CharacterId)> {
    holders
        .iter()
        .filter(|h| !online.contains(&h.character))
        .filter(|h| match unique.get(&h.key) {
            Some(0) => false,
            Some(days) => now.saturating_sub(h.last_seen) > u64::from(*days) * 86_400,
            // No longer unique in the content: free it.
            None => true,
        })
        .map(|h| (h.key.clone(), h.character))
        .collect()
}

impl Gateway {
    /// Reads the holders once, before anyone enters.
    pub(super) async fn load_holders(&mut self) {
        match self.repo.unique_holders().await {
            Ok(rows) => self.holders = rows.into_iter().map(|h| (h.key, h.character)).collect(),
            Err(e) => warn!(error = %e, "cannot read the unique holders"),
        }
    }

    /// What others hold, for one character.
    fn taken_for(&self, character: CharacterId) -> Vec<String> {
        self.holders
            .iter()
            .filter(|(_, c)| **c != character)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// The unique items that exist, to one zone (or every zone).
    pub(super) fn send_taken_items(&self, zone: Option<ZoneId>) {
        let keys: Vec<String> = self
            .holders
            .keys()
            .filter(|k| k.starts_with("item:"))
            .cloned()
            .collect();
        let cmd = || crate::zone_link::ToZone::Command(Command::TakenItems { keys: keys.clone() });
        match zone {
            Some(z) => self.to_zone(z, cmd()),
            None => {
                for z in self.zones.keys() {
                    self.to_zone(*z, cmd());
                }
            }
        }
    }

    /// Unique items that left the game: they may exist again.
    pub(super) fn on_released(&mut self, keys: Vec<String>) {
        for key in keys {
            if self.holders.remove(&key).is_none() {
                continue;
            }
            let repo = self.repo.clone();
            tokio::spawn(async move {
                if let Err(e) = repo.release_key(&key).await {
                    warn!(error = %e, key, "unique item not released");
                }
            });
        }
        self.send_taken_items(None);
    }

    /// `Command::Taken` to every player in the game.
    pub(super) fn send_taken_to_all(&self) {
        for (player, who) in &self.in_game {
            self.to_player(
                *player,
                Command::Taken {
                    player: *player,
                    keys: self.taken_for(who.character),
                },
            );
        }
    }

    /// Before `Command::Enter` and `Command::Arrive`: what others hold.
    pub(super) fn send_taken(&self, player: PlayerId, character: CharacterId) {
        self.to_player(
            player,
            Command::Taken {
                player,
                keys: self.taken_for(character),
            },
        );
    }

    /// After `Command::Enter`: a claim for every unique key the sheet has
    /// (it may have passed to an heir while the character was away).
    pub(super) fn claim_all(&self, player: PlayerId, character: CharacterId, keys: Vec<String>) {
        for key in keys {
            self.claim(player, character, key);
        }
    }

    /// Unique titles and classes a zone's players just got.
    pub(super) fn on_claims(&self, claims: Vec<(PlayerId, String)>) {
        for (player, key) in claims {
            if let Some(who) = self.in_game.get(&player) {
                self.claim(player, who.character, key);
            }
        }
    }

    fn claim(&self, player: PlayerId, character: CharacterId, key: String) {
        if self.holders.get(&key) == Some(&character) {
            return;
        }
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            match repo.claim_unique(&key, character, login::now_unix()).await {
                Ok(holder) => {
                    let _ = done.send(Done::Claimed {
                        player,
                        character,
                        key,
                        holder,
                    });
                }
                Err(e) => warn!(error = %e, key, "unique claim not written"),
            }
        });
    }

    pub(super) fn on_claimed(
        &mut self,
        player: PlayerId,
        character: CharacterId,
        key: String,
        holder: CharacterId,
    ) {
        if holder != character {
            // Someone else was first: the player loses it.
            self.to_player(
                player,
                Command::Revoke {
                    player,
                    key: key.clone(),
                },
            );
        }
        if self.holders.insert(key.clone(), holder) != Some(holder) {
            info!(key, holder = holder.get(), "unique holder");
            self.send_taken_to_all();
            if key.starts_with("item:") {
                self.send_taken_items(None);
            }
        }
    }

    /// Once an hour: holders gone too long make room for an heir.
    pub(super) fn heir_check_if_due(&mut self, now: u64) {
        if now < self.next_heir_check {
            return;
        }
        self.next_heir_check = now + HEIR_CHECK_SECS;
        let (repo, done, unique) = (
            self.repo.clone(),
            self.done_tx.clone(),
            self.cfg.content.unique.clone(),
        );
        let online: Vec<CharacterId> = self.in_game.values().map(|w| w.character).collect();
        tokio::spawn(async move {
            let holders = match repo.unique_holders().await {
                Ok(h) => h,
                Err(e) => return warn!(error = %e, "cannot read the unique holders"),
            };
            for (key, character) in expired(&holders, &unique, &online, now) {
                match repo.release_unique(&key, character).await {
                    Ok(true) => info!(key, holder = character.get(), "unique released for an heir"),
                    Ok(false) => {}
                    Err(e) => warn!(error = %e, key, "unique not released"),
                }
            }
            if let Ok(rows) = repo.unique_holders().await {
                let _ = done.send(Done::Holders(
                    rows.into_iter().map(|h| (h.key, h.character)).collect(),
                ));
            }
        });
    }

    pub(super) fn on_holders(&mut self, holders: BTreeMap<String, CharacterId>) {
        if holders != self.holders {
            self.holders = holders;
            self.send_taken_to_all();
            self.send_taken_items(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base::CharacterName;
    use persistence::UniqueHolder;

    #[test]
    fn heirs_only_after_the_days_and_never_online() {
        let h = |key: &str, id: u32, seen: u64| UniqueHolder {
            key: key.into(),
            character: CharacterId::new(id),
            name: CharacterName::try_from("Ana").unwrap(),
            last_seen: seen,
        };
        let unique: BTreeMap<String, u16> = [("title:king".to_owned(), 1), ("class:smith".to_owned(), 0)]
            .into_iter()
            .collect();
        let day = 86_400;
        let holders = vec![
            h("title:king", 1, 0),
            h("class:smith", 2, 0),
            h("title:gone", 3, 10 * day),
        ];
        // Day 2: the king was away more than a day; the smith never goes;
        // a key no longer unique is freed.
        let out = expired(&holders, &unique, &[], 2 * day);
        assert_eq!(
            out,
            vec![
                ("title:king".into(), CharacterId::new(1)),
                ("title:gone".into(), CharacterId::new(3))
            ]
        );
        // Online holders keep it.
        let out = expired(&holders, &unique, &[CharacterId::new(1)], 2 * day);
        assert_eq!(out, vec![("title:gone".into(), CharacterId::new(3))]);
    }
}

//! Dungeons (`content/dungeons/`): lockouts and clear records. The zone
//! checks level and group size at the door and says when a boss died
//! (`FromZone::Clears`); the gateway, which sees every character, keeps
//! the lockouts (`lockouts`), the clear times (`dungeon_clears`, for
//! `RankBy::FastestClear`) and the server first of each dungeon.

use super::{Done, Gateway};
use crate::login;
use base::{Arg, CharacterId, PlayerId, TextRef, ZoneId};
use sim::{Arrival, ClearOut};
use tracing::warn;

impl Gateway {
    /// Reads the running lockouts once, before anyone enters.
    pub(super) async fn load_lockouts(&mut self) {
        match self.repo.lockouts(login::now_unix()).await {
            Ok(rows) => {
                self.lockouts = rows.into_iter().map(|(c, d, until)| ((c, d), until)).collect();
            }
            Err(e) => warn!(error = %e, "cannot read the dungeon lockouts"),
        }
    }

    /// A character locked out of the dungeon in `template`: hours left.
    fn locked_hours(&self, character: CharacterId, template: ZoneId) -> Option<(base::TextId, u64)> {
        let d = self.cfg.content.dungeons.iter().find(|d| d.zone == template)?;
        let until = *self.lockouts.get(&(character, d.id.clone()))?;
        let now = login::now_unix();
        (until > now).then(|| (d.name, (until - now).div_ceil(3600)))
    }

    /// An arrival into a dungeon the character is locked out of goes back
    /// where it came from, with the reason; any other goes on to `to`.
    pub(super) fn turn_back_if_locked(
        &mut self,
        player: PlayerId,
        from: ZoneId,
        to: ZoneId,
        mut arrival: Box<Arrival>,
    ) -> (ZoneId, Box<Arrival>) {
        let locked = self
            .in_game
            .get(&player)
            .and_then(|w| self.locked_hours(w.character, to));
        let Some((name, hours)) = locked else {
            return (to, arrival);
        };
        if let Some(&c) = self.players.get(&player) {
            let args = vec![
                Arg::Ref(Box::new(TextRef::new(name))),
                Arg::Int(i64::try_from(hours).unwrap_or(i64::MAX)),
            ];
            self.system(c, "system.dungeon_locked", args);
        }
        arrival.pos = arrival.left_from.unwrap_or(arrival.pos);
        (from, arrival)
    }

    /// Runs cleared in a zone: lockouts, the record and the server first.
    pub(super) fn on_clears(&mut self, clears: Vec<ClearOut>) {
        let now = login::now_unix();
        for clear in clears {
            let Some(def) = self
                .cfg
                .content
                .dungeons
                .iter()
                .find(|d| d.id == clear.dungeon)
                .cloned()
            else {
                continue;
            };
            let who: Vec<(CharacterId, String)> = clear
                .players
                .iter()
                .filter_map(|p| self.in_game.get(p))
                .map(|w| (w.character, w.name.to_string()))
                .collect();
            let Some(&(first, _)) = who.first() else { continue };
            let party = who.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ");
            let until = def.lockout.until(now);
            let locks: Vec<CharacterId> = if until > now {
                who.iter().map(|(c, _)| *c).collect()
            } else {
                Vec::new()
            };
            for c in &locks {
                self.lockouts.insert((*c, def.id.clone()), until);
            }
            let (repo, done) = (self.repo.clone(), self.done_tx.clone());
            tokio::spawn(async move {
                if let Err(e) = repo.add_clear(&def.id, &party, clear.secs, now).await {
                    warn!(error = %e, dungeon = def.id, "clear not recorded");
                }
                for c in locks {
                    if let Err(e) = repo.set_lockout(c, &def.id, until).await {
                        warn!(error = %e, dungeon = def.id, "lockout not written");
                    }
                }
                match repo
                    .record_first(&format!("dungeon:{}", def.id), first, now)
                    .await
                {
                    Ok(true) => {
                        let _ = done.send(Done::DungeonFirst {
                            party,
                            name: def.name,
                        });
                    }
                    Ok(false) => {}
                    Err(e) => warn!(error = %e, "dungeon first not written"),
                }
            });
        }
    }

    pub(super) fn on_dungeon_first(&self, party: String, name: base::TextId) {
        self.tell_everyone(
            "system.dungeon_first",
            vec![Arg::Str(party), Arg::Ref(Box::new(TextRef::new(name)))],
        );
    }
}

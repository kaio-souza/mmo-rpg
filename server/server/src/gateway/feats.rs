//! Feats the whole server hears of: titles announced to everyone, server
//! firsts (kept in `server_firsts`), public rankings (`/ranking`) and the
//! list of firsts (`/firsts`). Zones only say which title a player earned
//! (`FromZone::Feats`); the gateway is the one place that sees every zone,
//! so it decides who was first.

use super::{Done, Gateway, Phase};
use crate::login;
use base::{Arg, CharacterName, PlayerId, TextRef};
use defs::{Announce, RankBy, RankingDef};
use persistence::SavedSheet;
use protocol::{RankingId, ServerMsg};
use server_net::ConnectionId;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::warn;

/// Rankings are built again when older than this, on the next `/ranking`.
pub(crate) const RANKINGS_FRESH_SECS: u64 = 300;

/// Lines of every ranking, by ranking id, best first.
/// A line names a character, or a party for the fastest clears.
pub(crate) type RankingLists = BTreeMap<String, Vec<(String, u64)>>;

/// The rankings built last and when.
#[derive(Default)]
pub(crate) struct RankingCache {
    pub(crate) built_at: u64,
    pub(crate) lists: Arc<RankingLists>,
    /// A build is running; requests wait for it.
    pub(crate) waiting: Vec<(ConnectionId, Option<String>)>,
}

/// The value of `sheet` in a ranking (0: left out).
pub(crate) fn score(by: &RankBy, sheet: &SavedSheet) -> u64 {
    let p = &sheet.progress;
    match by {
        // Level first, XP breaks ties within a level.
        RankBy::Level => u64::from(p.level.max(1)) << 48 | p.xp.min((1 << 48) - 1),
        RankBy::Counter { family, key } => p
            .counters
            .iter()
            .filter(|(f, k, _)| f == family && (key.is_empty() || k == key))
            .map(|(_, _, n)| *n)
            .fold(0u64, u64::saturating_add),
        RankBy::Proficiency(id) => p
            .proficiencies
            .iter()
            .find(|(i, _, _)| i == id)
            .map_or(0, |(_, points, _)| u64::from(*points)),
        RankBy::Titles => p.titles.len() as u64,
        // From the clear records, not the sheets.
        RankBy::FastestClear(_) => 0,
    }
}

/// The value shown to players (the level alone, for `Level`).
pub(crate) fn shown(by: &RankBy, value: u64) -> u64 {
    match by {
        RankBy::Level => value >> 48,
        _ => value,
    }
}

/// Every ranking from every sheet: best first, ties by name, zeros out.
/// `clears`: the fastest clears read for each `FastestClear` dungeon.
pub(crate) fn build(
    rankings: &[RankingDef],
    sheets: &[(CharacterName, SavedSheet)],
    clears: &BTreeMap<String, Vec<(String, u64)>>,
) -> RankingLists {
    rankings
        .iter()
        .map(|r| {
            if let RankBy::FastestClear(d) = &r.by {
                let mut lines = clears.get(d).cloned().unwrap_or_default();
                lines.truncate(usize::from(r.size));
                return (r.id.clone(), lines);
            }
            let mut lines: Vec<(String, u64)> = sheets
                .iter()
                .map(|(name, sheet)| (name.to_string(), score(&r.by, sheet)))
                .filter(|(_, v)| *v > 0)
                .collect();
            lines.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            lines.truncate(usize::from(r.size));
            (r.id.clone(), lines)
        })
        .collect()
}

impl Gateway {
    /// A System line to every player in the game, shown in the window.
    pub(super) fn tell_everyone(&self, key: &str, args: Vec<Arg>) {
        let Some(id) = self.keys.id(key) else {
            warn!(key, "system text key missing");
            return;
        };
        let mut text = TextRef::new(id);
        text.args.extend(args);
        let msg = ServerMsg::System { text, window: true };
        for (c, conn) in &self.conns {
            if matches!(conn.phase, Phase::InZone(_)) {
                self.net.send_msg(*c, &msg);
            }
        }
    }

    /// Title name of `id` as a text argument.
    fn title_arg(&self, id: &str) -> Option<Arg> {
        let key = defs::title_name_key(id);
        Some(Arg::Ref(Box::new(TextRef::new(self.keys.id(&key)?))))
    }

    /// Titles a zone's players earned that the server may hear of.
    pub(super) fn on_feats(&mut self, feats: Vec<(PlayerId, String, Announce)>) {
        for (player, title, announce) in feats {
            let Some(who) = self.in_game.get(&player) else {
                continue;
            };
            let Some(arg) = self.title_arg(&title) else {
                continue;
            };
            let name = Arg::Str(who.name.to_string());
            match announce {
                Announce::Quiet => {}
                Announce::Everyone => self.tell_everyone("system.world_title", vec![name, arg]),
                Announce::First => {
                    let (repo, done, character) = (self.repo.clone(), self.done_tx.clone(), who.character);
                    let who = who.name.clone();
                    tokio::spawn(async move {
                        let key = format!("title:{title}");
                        match repo.record_first(&key, character, login::now_unix()).await {
                            Ok(true) => {
                                let _ = done.send(Done::FirstFeat { name: who, title });
                            }
                            Ok(false) => {}
                            Err(e) => warn!(error = %e, key, "server first not written"),
                        }
                    });
                }
            }
        }
    }

    pub(super) fn on_first_feat(&mut self, name: &CharacterName, title: &str) {
        if let Some(arg) = self.title_arg(title) {
            self.tell_everyone("system.server_first", vec![Arg::Str(name.to_string()), arg]);
        }
    }

    /// `/ranking [id]`.
    pub(super) fn on_ranking(&mut self, c: ConnectionId, id: Option<RankingId>) {
        let rankings = &self.cfg.content.rankings;
        let Some(id) = id else {
            if rankings.is_empty() {
                return self.system(c, "system.ranking_none", Vec::new());
            }
            for r in rankings.iter() {
                let name = Arg::Ref(Box::new(TextRef::new(r.name)));
                self.system(c, "system.ranking_entry", vec![Arg::Str(r.id.clone()), name]);
            }
            return;
        };
        let id = id.as_str().to_owned();
        if !rankings.iter().any(|r| r.id == id) {
            return self.system(c, "system.ranking_unknown", vec![Arg::Str(id)]);
        }
        let now = login::now_unix();
        if now.saturating_sub(self.rankings.built_at) < RANKINGS_FRESH_SECS && self.rankings.built_at > 0 {
            return self.show_ranking(c, &id);
        }
        self.rankings.waiting.push((c, Some(id)));
        if self.rankings.waiting.len() > 1 {
            return;
        }
        let (repo, done, defs) = (self.repo.clone(), self.done_tx.clone(), rankings.clone());
        tokio::spawn(async move {
            let mut clears = BTreeMap::new();
            for r in defs.iter() {
                if let RankBy::FastestClear(d) = &r.by {
                    match repo.fastest_clears(d, u32::from(r.size)).await {
                        Ok(lines) => {
                            clears.insert(d.clone(), lines);
                        }
                        Err(e) => warn!(error = %e, dungeon = d, "fastest clears not read"),
                    }
                }
            }
            let lists = match repo.all_sheets().await {
                Ok(rows) => {
                    let sheets: Vec<(CharacterName, SavedSheet)> =
                        rows.into_iter().map(|(_, name, sheet)| (name, sheet)).collect();
                    build(&defs, &sheets, &clears)
                }
                Err(e) => {
                    warn!(error = %e, "rankings not built");
                    RankingLists::new()
                }
            };
            let _ = done.send(Done::Ranked { lists, at: now });
        });
    }

    pub(super) fn on_ranked(&mut self, lists: RankingLists, at: u64) {
        if !lists.is_empty() {
            self.rankings.lists = Arc::new(lists);
            self.rankings.built_at = at;
        }
        for (c, id) in std::mem::take(&mut self.rankings.waiting) {
            if let Some(id) = id {
                self.show_ranking(c, &id);
            }
        }
    }

    fn show_ranking(&self, c: ConnectionId, id: &str) {
        let Some(def) = self.cfg.content.rankings.iter().find(|r| r.id == id) else {
            return;
        };
        let name = Arg::Ref(Box::new(TextRef::new(def.name)));
        let lines = self.rankings.lists.get(id).map(Vec::as_slice).unwrap_or_default();
        if lines.is_empty() {
            return self.system(c, "system.ranking_empty", vec![name]);
        }
        self.system(c, "system.ranking_head", vec![name]);
        for (i, (who, value)) in lines.iter().enumerate() {
            let value = i64::try_from(shown(&def.by, *value)).unwrap_or(i64::MAX);
            let place = i64::try_from(i + 1).unwrap_or(i64::MAX);
            self.system(
                c,
                "system.ranking_line",
                vec![Arg::Int(place), Arg::Str(who.clone()), Arg::Int(value)],
            );
        }
    }

    /// `/firsts`.
    pub(super) fn on_firsts(&mut self, c: ConnectionId) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            match repo.server_firsts().await {
                Ok(rows) => {
                    let _ = done.send(Done::Firsts { c, rows });
                }
                Err(e) => warn!(error = %e, "server firsts not read"),
            }
        });
    }

    pub(super) fn on_firsts_read(&self, c: ConnectionId, rows: &[(String, CharacterName, u64)]) {
        let mut any = false;
        for (key, name, _) in rows {
            let dungeon = |id: &str| {
                let d = self.cfg.content.dungeons.iter().find(|d| d.id == id)?;
                Some(Arg::Ref(Box::new(TextRef::new(d.name))))
            };
            let arg = match key.split_once(':') {
                Some(("title", t)) => self.title_arg(t),
                Some(("dungeon", d)) => dungeon(d),
                _ => None,
            };
            let Some(arg) = arg else {
                continue;
            };
            any = true;
            self.system(c, "system.first_line", vec![arg, Arg::Str(name.to_string())]);
        }
        if !any {
            self.system(c, "system.first_none", Vec::new());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(level: u8, kills: u64, titles: usize) -> SavedSheet {
        let mut s = SavedSheet::default();
        s.progress.level = level;
        s.progress.counters = vec![
            ("kills".into(), "wolf".into(), kills),
            ("kills".into(), "bat".into(), 1),
        ];
        s.progress.titles = (0..titles).map(|i| format!("t{i}")).collect();
        s
    }

    fn def(id: &str, by: RankBy, size: u8) -> RankingDef {
        RankingDef {
            id: id.into(),
            name: base::TextId::new(0),
            by,
            size,
        }
    }

    #[test]
    fn ranks_best_first_and_cuts() {
        let n = |s: &str| CharacterName::try_from(s).unwrap();
        let sheets = vec![
            (n("Ana"), sheet(3, 5, 0)),
            (n("Bea"), sheet(7, 2, 2)),
            (n("Cid"), sheet(7, 9, 1)),
        ];
        let lists = build(
            &[
                def("level", RankBy::Level, 2),
                def(
                    "wolves",
                    RankBy::Counter {
                        family: "kills".into(),
                        key: "wolf".into(),
                    },
                    10,
                ),
                def(
                    "all",
                    RankBy::Counter {
                        family: "kills".into(),
                        key: String::new(),
                    },
                    10,
                ),
                def("titles", RankBy::Titles, 10),
                def("crypt", RankBy::FastestClear("crypt".into()), 1),
            ],
            &sheets,
            &[(
                "crypt".to_owned(),
                vec![("Ana, Bea".to_owned(), 90), ("Cid".to_owned(), 120)],
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(lists["crypt"], vec![("Ana, Bea".to_owned(), 90)]);
        let names = |id: &str| lists[id].iter().map(|(n, v)| (n.clone(), *v)).collect::<Vec<_>>();
        assert_eq!(names("level").len(), 2);
        assert_eq!(shown(&RankBy::Level, lists["level"][0].1), 7);
        assert_eq!(
            names("wolves"),
            vec![("Cid".into(), 9), ("Ana".into(), 5), ("Bea".into(), 2)]
        );
        assert_eq!(names("all")[0], ("Cid".into(), 10));
        // Ana has no title: left out.
        assert_eq!(names("titles"), vec![("Bea".into(), 2), ("Cid".into(), 1)]);
    }
}

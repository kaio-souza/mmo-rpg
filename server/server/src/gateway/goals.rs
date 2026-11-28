//! Server goals (`goals/<id>.ron`): one shared quest for the whole server.
//! Zones only count what their players add (`FromZone::Goals`); the
//! gateway, the one place that sees every zone, sums it, tells everyone at
//! each quarter and when a goal is reached, and then tells every zone
//! (`Command::GoalsDone`) so dialogues open and counting stops.
//! Progress is written to `server_goals` within a second of a change.

use super::Gateway;
use crate::login;
use base::{Arg, TextRef};
use server_net::ConnectionId;
use sim::Command;
use std::collections::{BTreeMap, BTreeSet};
use tracing::warn;

/// How far the server got.
#[derive(Default)]
pub(crate) struct Goals {
    /// By goal id: progress and the unix second it was reached.
    pub(crate) state: BTreeMap<String, (u64, Option<u64>)>,
    /// Goals changed since the last write.
    pub(crate) dirty: BTreeSet<String>,
}

impl Goals {
    /// Adds `n` to a goal of `target`; the quarters it crossed (25, 50 or
    /// 75) and whether it was reached now.
    pub(crate) fn add(&mut self, id: &str, n: u64, target: u64, now: u64) -> (Option<u64>, bool) {
        let entry = self.state.entry(id.to_owned()).or_insert((0, None));
        if entry.1.is_some() || n == 0 {
            return (None, false);
        }
        let before = entry.0;
        entry.0 = before.saturating_add(n).min(target);
        self.dirty.insert(id.to_owned());
        if entry.0 >= target {
            entry.1 = Some(now);
            return (None, true);
        }
        let quarter = |v: u64| u64::try_from(u128::from(v) * 4 / u128::from(target.max(1))).unwrap_or(0);
        let crossed = (quarter(entry.0) > quarter(before)).then(|| quarter(entry.0) * 25);
        (crossed, false)
    }

    pub(crate) fn done(&self) -> Vec<String> {
        self.state
            .iter()
            .filter(|(_, (_, done))| done.is_some())
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Gateway {
    /// Reads the goals once, before anyone enters.
    pub(super) async fn load_goals(&mut self) {
        match self.repo.server_goals().await {
            Ok(rows) => {
                self.goals.state = rows.into_iter().map(|(id, p, done)| (id, (p, done))).collect();
            }
            Err(e) => warn!(error = %e, "cannot read the server goals"),
        }
    }

    /// Writes the goals that changed.
    pub(super) fn save_goals(&mut self) {
        if self.goals.dirty.is_empty() {
            return;
        }
        let rows: Vec<(String, u64, Option<u64>)> = std::mem::take(&mut self.goals.dirty)
            .into_iter()
            .filter_map(|id| {
                let (p, done) = *self.goals.state.get(&id)?;
                Some((id, p, done))
            })
            .collect();
        let repo = self.repo.clone();
        tokio::spawn(async move {
            if let Err(e) = repo.save_server_goals(rows).await {
                warn!(error = %e, "server goals not written");
            }
        });
    }

    /// The goals reached, to one zone (or every zone).
    pub(super) fn send_goals_done(&self, zone: Option<base::ZoneId>) {
        let ids = self.goals.done();
        let cmd = || crate::zone_link::ToZone::Command(Command::GoalsDone { ids: ids.clone() });
        match zone {
            Some(z) => self.to_zone(z, cmd()),
            None => {
                for z in self.zones.keys() {
                    self.to_zone(*z, cmd());
                }
            }
        }
    }

    fn goal_arg(name: base::TextId) -> Arg {
        Arg::Ref(Box::new(TextRef::new(name)))
    }

    /// What a zone's players added this tick.
    pub(super) fn on_goal_counts(&mut self, counts: Vec<(String, u64)>) {
        let now = login::now_unix();
        let mut reached = false;
        for (id, n) in counts {
            let Some(def) = self.cfg.content.goals.iter().find(|g| g.id == id) else {
                continue;
            };
            let (name, target) = (def.name, def.target);
            let (quarter, done) = self.goals.add(&id, n, target, now);
            if let Some(q) = quarter {
                let percent = i64::try_from(q).unwrap_or(0);
                self.tell_everyone(
                    "system.goal_progress",
                    vec![Self::goal_arg(name), Arg::Int(percent)],
                );
            }
            if done {
                reached = true;
                self.tell_everyone("system.goal_done", vec![Self::goal_arg(name)]);
            }
        }
        if reached {
            self.send_goals_done(None);
        }
    }

    /// `/goals`: every goal of the game and how far the server got.
    pub(super) fn on_goals_asked(&self, c: ConnectionId) {
        let goals = &self.cfg.content.goals;
        if goals.is_empty() {
            return self.system(c, "system.goal_none", Vec::new());
        }
        for g in goals {
            let (progress, done) = self.goals.state.get(&g.id).copied().unwrap_or((0, None));
            let int = |v: u64| Arg::Int(i64::try_from(v).unwrap_or(i64::MAX));
            if done.is_some() {
                self.system(c, "system.goal_line_done", vec![Self::goal_arg(g.name)]);
            } else {
                self.system(
                    c,
                    "system.goal_line",
                    vec![Self::goal_arg(g.name), int(progress), int(g.target)],
                );
            }
            self.system(c, "system.goal_desc", vec![Self::goal_arg(g.description)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarters_and_the_end_are_told_once() {
        let mut g = Goals::default();
        assert_eq!(g.add("cull", 20, 100, 1), (None, false));
        assert_eq!(g.add("cull", 10, 100, 2), (Some(25), false));
        assert_eq!(
            g.add("cull", 40, 100, 3),
            (Some(75), false),
            "two quarters: the last told"
        );
        assert_eq!(g.add("cull", 500, 100, 4), (None, true));
        assert_eq!(g.state.get("cull"), Some(&(100, Some(4))), "capped at the target");
        assert_eq!(g.add("cull", 5, 100, 5), (None, false), "reached for good");
        assert_eq!(g.done(), vec!["cull".to_owned()]);
    }
}

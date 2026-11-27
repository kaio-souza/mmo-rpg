//! Instances: a zone marked `instance` in content is never run as one
//! shared zone. A party that walks into it (a portal or a `Teleport` aimed
//! at one of its tiles) gets its own copy, made here on demand:
//!
//! - The copy gets a runtime zone id (`INSTANCE_IDS` and up) that only the
//!   gateway knows; its zone runs the template's map with its own seed.
//! - Arrivals wait until the copy's link is up.
//! - An empty copy closes after `close_after_secs`; with a time limit,
//!   everyone inside is sent to `exit` when it runs out.
//! - A character saved inside is saved at `exit`: copies never outlive a
//!   restart.

use super::{Gateway, ZoneConn};
use base::{PlayerId, TilePos, ZoneId};
use defs::InstanceRule;
use sim::{Arrival, Command};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// Runtime zone ids of copies start here, far above any content zone.
pub(crate) const INSTANCE_IDS: u32 = 1 << 24;

/// Makes a copy of `template` run as `runtime` with `seed`, linked to the
/// gateway (it comes up as `ZoneEvent::Up`). `None` in a gateway without
/// local zones: instances are refused and arrivals go to the exit.
pub type InstanceSpawner = Arc<dyn Fn(ZoneId, ZoneId, u64) -> Result<(), String> + Send + Sync>;

/// One copy in use.
pub(crate) struct Live {
    pub(crate) template: ZoneId,
    /// The party that opened it: its leader then.
    pub(crate) party: PlayerId,
    pub(crate) opened: Instant,
    pub(crate) empty_since: Option<Instant>,
    pub(crate) evicted: bool,
}

/// Every content zone that is an instance, with its exit's zone.
pub(crate) fn instance_rules(cat: &defs::ValidatedCatalog) -> BTreeMap<ZoneId, (InstanceRule, ZoneId)> {
    let zones: Vec<&defs::ZoneDef> = cat.zones().map(|(_, z)| z).collect();
    cat.zones()
        .filter_map(|(id, z)| {
            let rule = z.instance?;
            let exit = defs::zone_at(zones.iter().copied(), rule.exit)?;
            let (exit_id, _) = cat.zone_by_name(&exit.name)?;
            Some((id, (rule, exit_id)))
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct Instances {
    pub(crate) live: BTreeMap<ZoneId, Live>,
    /// `(template, party leader)` → runtime id.
    pub(crate) by_party: BTreeMap<(ZoneId, PlayerId), ZoneId>,
    /// Arrivals for copies whose link is not up yet.
    pub(crate) waiting: BTreeMap<ZoneId, Vec<(PlayerId, Box<Arrival>)>>,
    pub(crate) next: u32,
}

impl Gateway {
    /// The rule of a content zone, when it is an instance.
    fn rule_of(&self, template: ZoneId) -> Option<(InstanceRule, ZoneId)> {
        self.cfg.content.instances.get(&template).copied()
    }

    /// Where a character in `zone` at `pos` is saved: the exit of a copy.
    pub(super) fn place(&self, zone: ZoneId, pos: TilePos) -> (String, TilePos) {
        match self
            .instances
            .live
            .get(&zone)
            .and_then(|l| self.rule_of(l.template))
        {
            Some((rule, exit_zone)) => (self.zone_name(exit_zone), rule.exit),
            None => (self.zone_name(zone), pos),
        }
    }

    /// The template of a runtime copy, for names in logs.
    pub(super) fn template_of(&self, zone: ZoneId) -> Option<ZoneId> {
        self.instances.live.get(&zone).map(|l| l.template)
    }

    /// A hand-off to `to`. A content instance becomes the party's copy
    /// (made now if needed); `None` when the arrival was handled here
    /// (waiting for the copy, or sent to the exit).
    pub(super) fn route_arrival(
        &mut self,
        player: PlayerId,
        to: ZoneId,
        mut arrival: Box<Arrival>,
    ) -> Option<(ZoneId, Box<Arrival>)> {
        let Some((rule, exit_zone)) = self.rule_of(to) else {
            return Some((to, arrival));
        };
        let party = arrival.party.unwrap_or(player);
        let runtime = match self.instances.by_party.get(&(to, party)) {
            Some(r) => *r,
            None => match self.open_instance(to, party) {
                Some(r) => r,
                None => {
                    arrival.pos = rule.exit;
                    return Some((exit_zone, arrival));
                }
            },
        };
        if self.zones.contains_key(&runtime) {
            return Some((runtime, arrival));
        }
        self.instances
            .waiting
            .entry(runtime)
            .or_default()
            .push((player, arrival));
        if let Some(w) = self.in_game.get_mut(&player) {
            w.zone = runtime;
        }
        None
    }

    fn open_instance(&mut self, template: ZoneId, party: PlayerId) -> Option<ZoneId> {
        let spawn = self.cfg.spawn_instance.clone()?;
        let n = self.instances.next;
        self.instances.next = n.wrapping_add(1);
        let runtime = ZoneId::new(INSTANCE_IDS.saturating_add(n));
        let seed = u64::from(n)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(u64::from(template.get()));
        if let Err(e) = spawn(template, runtime, seed) {
            warn!(error = %e, zone = %self.zone_name(template), "instance not made: players go to the exit");
            return None;
        }
        info!(zone = %self.zone_name(template), copy = n, "instance opened");
        self.instances.live.insert(
            runtime,
            super::instance::Live {
                template,
                party,
                opened: Instant::now(),
                empty_since: None,
                evicted: false,
            },
        );
        self.instances.by_party.insert((template, party), runtime);
        Some(runtime)
    }

    /// A copy's link is up: the players waiting for it go in.
    pub(super) fn on_instance_up(&mut self, zone: ZoneId) {
        for (player, arrival) in self.instances.waiting.remove(&zone).unwrap_or_default() {
            if let Some(character) = self.in_game.get(&player).map(|w| w.character) {
                self.send_taken(player, character);
            }
            self.to_zone(
                zone,
                crate::zone_link::ToZone::Command(Command::Arrive { player, arrival }),
            );
        }
    }

    /// Once a second: time limits and empty copies.
    pub(super) fn instances_second(&mut self) {
        let now = Instant::now();
        let mut close = Vec::new();
        let mut evict = Vec::new();
        for (zone, live) in &mut self.instances.live {
            let inside = self.in_game.values().any(|w| w.zone == *zone);
            let Some((rule, _)) = self.cfg.content.instances.get(&live.template).copied() else {
                close.push(*zone);
                continue;
            };
            if inside {
                live.empty_since = None;
                let limit = u64::from(rule.time_limit_secs);
                if limit > 0 && !live.evicted && now.duration_since(live.opened).as_secs() >= limit {
                    live.evicted = true;
                    evict.push((*zone, rule.exit));
                }
            } else {
                let since = *live.empty_since.get_or_insert(now);
                // Not closed while arrivals still wait for it.
                if now.duration_since(since).as_secs() >= u64::from(rule.close_after_secs)
                    && !self.instances.waiting.contains_key(zone)
                {
                    close.push(*zone);
                }
            }
        }
        for (zone, to) in evict {
            info!(zone = %self.zone_name(zone), "instance time is up: everyone to the exit");
            self.to_zone(zone, crate::zone_link::ToZone::Command(Command::Evict { to }));
        }
        for zone in close {
            self.close_instance(zone);
        }
    }

    fn close_instance(&mut self, zone: ZoneId) {
        let Some(live) = self.instances.live.remove(&zone) else {
            return;
        };
        self.instances.by_party.remove(&(live.template, live.party));
        // Dropping the link stops the copy's zone on its next tick.
        let _: Option<ZoneConn> = self.zones.remove(&zone);
        self.pending_reload.remove(&zone);
        info!(zone = %self.zone_name(live.template), "instance closed");
    }
}

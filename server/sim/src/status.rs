//! Statuses (poison, stun, slow) left by blows, area blows and the phases
//! of a monster's fight (a boss). Content sets them on techniques
//! (`area`, `status`) and monsters (`phases`); every rule here is fixed
//! point and runs in the zone's tick order.

use crate::Zone;
use crate::fight::{Fighter, MonsterComp, strike};
use crate::types::TickOutput;
use crate::zone::{Body, PlayerComp};
use base::{Arg, EntityId, TextRef, TilePos};
use defs::{MonsterPhase, RegionKind, Status, StatusKind};
use serde::{Deserialize, Serialize};

/// Ticks between two poison bites (1 s).
const POISON_EVERY_TICKS: u64 = base::TICKS_PER_SECOND;

/// A status on a fighter until `until`; `by` left it (a poison kill is
/// theirs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActiveStatus {
    pub kind: StatusKind,
    pub until: u64,
    pub power: u32,
    pub by: EntityId,
}

impl Fighter {
    /// The status of this kind still running at `now`.
    pub(crate) fn status(&self, kind: StatusKind, now: u64) -> Option<&ActiveStatus> {
        self.statuses.iter().find(|s| s.kind == kind && now < s.until)
    }

    pub(crate) fn stunned(&self, now: u64) -> bool {
        self.status(StatusKind::Stun, now).is_some()
    }

    /// `ticks` between blows, longer while slowed.
    pub(crate) fn paced(&self, ticks: u64, now: u64) -> u64 {
        match self.status(StatusKind::Slow, now) {
            Some(s) => ticks.saturating_add(combat::permille(ticks, s.power)),
            None => ticks,
        }
    }
}

/// System line key told to a player who gets a status.
fn told(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Poison => "system.status_poison",
        StatusKind::Stun => "system.status_stun",
        StatusKind::Slow => "system.status_slow",
    }
}

/// Leaves `status` on `tgt` (the same kind is replaced, never stacked).
pub(crate) fn apply(z: &mut Zone, by: EntityId, tgt: EntityId, status: &Status, out: &mut TickOutput) {
    let now = z.tick.0;
    let Some(&h) = z.by_id.get(&tgt) else { return };
    let Ok(f) = z.world.query_one_mut::<&mut Fighter>(h) else {
        return;
    };
    if !f.alive() {
        return;
    }
    f.statuses.retain(|s| s.kind != status.kind && now < s.until);
    f.statuses.push(ActiveStatus {
        kind: status.kind,
        until: now.saturating_add(u64::from(status.duration_ticks)),
        power: status.power,
        by,
    });
    f.statuses.sort_by_key(|s| s.kind);
    if status.kind == StatusKind::Stun {
        f.next_attack = f
            .next_attack
            .max(now.saturating_add(u64::from(status.duration_ticks)));
        if let Ok(w) = z.world.query_one_mut::<&mut crate::zone::Walk>(h) {
            w.steps.clear();
        }
    }
    if let Some(player) = z.world.get::<&PlayerComp>(h).ok().map(|p| p.player) {
        let secs = i64::from(status.duration_ticks.div_ceil(20));
        z.note(
            out,
            player,
            told(status.kind),
            smallvec::smallvec![Arg::Int(secs)],
        );
    }
}

/// Every fighter's statuses: expired ones go, poison bites every second.
pub(crate) fn run(z: &mut Zone, out: &mut TickOutput) {
    let now = z.tick.0;
    let bite = now.is_multiple_of(POISON_EVERY_TICKS);
    let mut bitten: Vec<(EntityId, EntityId, u32)> = Vec::new();
    for (&id, &h) in &z.by_id {
        let Ok(mut f) = z.world.get::<&mut Fighter>(h) else {
            continue;
        };
        if f.statuses.is_empty() {
            continue;
        }
        f.statuses.retain(|s| now < s.until);
        if bite
            && f.alive()
            && let Some(p) = f.status(StatusKind::Poison, now)
        {
            bitten.push((p.by, id, p.power));
        }
    }
    for (by, tgt, power) in bitten {
        poison(z, by, tgt, power, out);
    }
}

fn poison(z: &mut Zone, by: EntityId, tgt: EntityId, power: u32, out: &mut TickOutput) {
    let now = z.tick.0;
    let Some(&h) = z.by_id.get(&tgt) else { return };
    let killed = match z.world.query_one_mut::<(&mut Fighter, &mut Body)>(h) {
        Ok((f, body)) => {
            f.hp = f.hp.saturating_sub(power);
            body.status_tick = now;
            f.hp == 0
        }
        Err(_) => return,
    };
    out.combat.push(crate::types::CombatOut {
        attacker: by,
        target: tgt,
        damage: Some(power),
        crit: false,
        killed,
        technique: None,
    });
    if killed {
        crate::fight::on_kill(z, by, tgt, out);
    }
}

/// Foes of `att` within `area` tiles of `center`, but `skip`, sorted by
/// id: monsters for a player, players outside safe ground for a monster.
pub(crate) fn foes_around(
    z: &Zone,
    att: EntityId,
    center: TilePos,
    area: u16,
    skip: EntityId,
) -> Vec<EntityId> {
    let Some(&ah) = z.by_id.get(&att) else {
        return Vec::new();
    };
    let player = z.world.get::<&PlayerComp>(ah).is_ok();
    let mut near = Vec::new();
    z.grid.neighbors(center, area, &mut near);
    let mut out: Vec<EntityId> = near
        .into_iter()
        .filter(|(e, p)| *e != skip && *e != att && center.distance(*p) <= u32::from(area))
        .filter(|(e, p)| {
            let Some(&h) = z.by_id.get(e) else { return false };
            let alive = z.world.get::<&Fighter>(h).is_ok_and(|f| f.alive());
            let is_player = z.world.get::<&PlayerComp>(h).is_ok();
            let is_monster = z.world.get::<&MonsterComp>(h).is_ok();
            alive
                && if player {
                    is_monster && z.can_hit(att, *e, u16::MAX).is_ok()
                } else {
                    is_player && z.region(*p) != Some(RegionKind::Safe)
                }
        })
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// One blow with its area and status: the target, then every foe around
/// it. Returns whether the blow on the target landed.
/// What a blow carries besides who strikes whom.
#[derive(Clone, Copy)]
pub(crate) struct Hit<'a> {
    /// Damage multiplier in thousandths.
    pub mult: u32,
    pub technique: Option<base::TechniqueId>,
    pub area: u16,
    pub status: Option<&'a Status>,
}

pub(crate) fn blow(z: &mut Zone, att: EntityId, tgt: EntityId, hit: &Hit<'_>, out: &mut TickOutput) -> bool {
    let Hit {
        mult,
        technique,
        area,
        status,
    } = *hit;
    let center = z.body_of(tgt).map(|(_, p)| p);
    let mut others = Vec::new();
    if area > 0
        && let Some(c) = center
    {
        others = foes_around(z, att, c, area, tgt);
    }
    let landed = strike(z, att, tgt, mult, technique, out);
    if landed && let Some(s) = status {
        apply(z, att, tgt, s, out);
    }
    for o in others {
        if strike(z, att, o, mult, technique, out)
            && let Some(s) = status
        {
            apply(z, att, o, s, out);
        }
    }
    landed
}

/// The deepest phase `hp` of `max_hp` has reached (1-based; 0: none).
pub(crate) fn phase_of(phases: &[MonsterPhase], hp: u32, max_hp: u32) -> u8 {
    let left = (u64::from(hp).saturating_mul(1000))
        .checked_div(u64::from(max_hp.max(1)))
        .unwrap_or(0);
    let n = phases
        .iter()
        .take_while(|p| left <= u64::from(p.below_hp_permille))
        .count();
    u8::try_from(n).unwrap_or(u8::MAX)
}

/// Moves a monster into a deeper phase when its HP crossed one, telling
/// the players who fight it. Returns the phase in force.
pub(crate) fn update_phase<'a>(
    z: &mut Zone,
    h: hecs::Entity,
    phases: &'a [MonsterPhase],
    name: base::TextId,
    out: &mut TickOutput,
) -> Option<&'a MonsterPhase> {
    let (hp, max) = z.world.get::<&Fighter>(h).ok().map(|f| (f.hp, f.max_hp))?;
    let now = phase_of(phases, hp, max);
    let (was, fighters) = {
        let m = z.world.get::<&MonsterComp>(h).ok()?;
        let target = z.world.get::<&Fighter>(h).ok().and_then(|f| f.target);
        (m.phase, m.threat.keys().chain(target).collect::<Vec<_>>())
    };
    if now > was {
        if let Ok(mut m) = z.world.get::<&mut MonsterComp>(h) {
            m.phase = now;
        }
        let mut players: Vec<base::PlayerId> = fighters
            .iter()
            .filter_map(|e| z.by_id.get(e))
            .filter_map(|h| z.world.get::<&PlayerComp>(*h).ok().map(|p| p.player))
            .collect();
        players.sort();
        players.dedup();
        for p in players {
            let arg = Arg::Ref(Box::new(TextRef::new(name)));
            z.tell(out, p, "system.boss_phase", smallvec::smallvec![arg]);
        }
    }
    let phase = now.max(was);
    phases.get(usize::from(phase).checked_sub(1)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phase(below: u16) -> MonsterPhase {
        MonsterPhase {
            below_hp_permille: below,
            damage_permille: 1000,
            attack_ticks: None,
            area: 0,
            status: None,
        }
    }

    #[test]
    fn phases_follow_the_hp_left() {
        let phases = [phase(700), phase(300)];
        assert_eq!(phase_of(&phases, 100, 100), 0);
        assert_eq!(phase_of(&phases, 70, 100), 1);
        assert_eq!(phase_of(&phases, 31, 100), 1);
        assert_eq!(phase_of(&phases, 30, 100), 2);
        assert_eq!(phase_of(&phases, 0, 100), 2);
        assert_eq!(phase_of(&[], 1, 100), 0);
    }
}

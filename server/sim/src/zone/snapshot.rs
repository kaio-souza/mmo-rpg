//! The zone's state as data: the snapshot a replay starts from, the zone
//! rebuilt out of one, and the hash that says two runs agree.

use super::*;
use base::StateHash;

impl Zone {
    /// A zone in the state of a replay snapshot (for the `replay` tool).
    pub fn from_snapshot(
        content: Arc<Content>,
        zone: ZoneId,
        seed: u64,
        state: &ZoneState,
    ) -> Result<Zone, ZoneError> {
        let mut z = Zone::new(content, zone, seed)?;
        // The snapshot has its own NPCs.
        for id in z.by_id.keys().copied().collect::<Vec<_>>() {
            z.remove_entity(id);
        }
        z.tick = state.tick;
        z.next_id = state.next_id;
        z.rng = Rng64::new(state.rng);
        z.respawns = state.respawns.clone();
        z.item_ids = state.item_ids.map(|(a, b)| IdBlock::new(a, b));
        z.social = state.social.clone();
        z.ghosts = state.ghosts.clone();
        z.escrows = postcard::from_bytes(&state.escrows).unwrap_or_default();
        z.now_unix = state.now_unix;
        for e in &state.entities {
            let mut builder = hecs::EntityBuilder::new();
            builder.add(e.body.clone()).add(e.walk.clone());
            if let Some(p) = &e.player {
                builder.add(p.clone());
            }
            if let Some(n) = &e.npc {
                builder.add(n.clone());
            }
            if let Some(d) = &e.dialogue {
                builder.add(d.clone());
            }
            if let Some(f) = &e.fighter {
                builder.add(f.clone());
            }
            if let Some(m) = &e.monster {
                builder.add(m.clone());
            }
            if let Some(s) = &e.shop {
                builder.add(s.clone());
            }
            if let Some(o) = e.shop_open {
                builder.add(o);
            }
            if let Some(n) = &e.node {
                builder.add(n.clone());
            }
            if let Some(c) = &e.companion {
                builder.add(c.clone());
            }
            if let Some(g) = e
                .gear
                .as_deref()
                .and_then(|b| postcard::from_bytes::<Gear>(b).ok())
            {
                builder.add(g);
            }
            z.insert_entity(e.body.id, e.body.pos, builder.build());
        }
        let snap = z.snapshot();
        z.replay.start(snap);
        Ok(z)
    }

    /// Complete state, entities sorted by id. Also the base of replay.
    pub fn snapshot(&self) -> ZoneState {
        let entities = self
            .by_id
            .values()
            .filter_map(|&h| {
                let body = Body::clone(&*self.world.get::<&Body>(h).ok()?);
                let walk = Walk::clone(&*self.world.get::<&Walk>(h).ok()?);
                let player = self
                    .world
                    .get::<&PlayerComp>(h)
                    .ok()
                    .map(|p| PlayerComp::clone(&p));
                let npc = self.world.get::<&NpcComp>(h).ok().map(|n| NpcComp::clone(&n));
                let dialogue = self
                    .world
                    .get::<&DialogueComp>(h)
                    .ok()
                    .map(|d| DialogueComp::clone(&d));
                let fighter = self.world.get::<&Fighter>(h).ok().map(|f| Fighter::clone(&f));
                let monster = self
                    .world
                    .get::<&MonsterComp>(h)
                    .ok()
                    .map(|m| MonsterComp::clone(&m));
                let gear = self
                    .world
                    .get::<&Gear>(h)
                    .ok()
                    .and_then(|g| postcard::to_stdvec(&*g).ok());
                let shop = self.world.get::<&ShopComp>(h).ok().map(|s| ShopComp::clone(&s));
                let shop_open = self.world.get::<&ShopOpen>(h).ok().map(|o| *o);
                let node = self.world.get::<&NodeComp>(h).ok().map(|n| NodeComp::clone(&n));
                let companion = self
                    .world
                    .get::<&crate::companion::CompanionComp>(h)
                    .ok()
                    .map(|c| crate::companion::CompanionComp::clone(&c));
                Some(crate::replay::EntitySnapshot {
                    body,
                    walk,
                    player,
                    npc,
                    dialogue,
                    fighter,
                    monster,
                    gear,
                    shop,
                    shop_open,
                    node,
                    companion,
                })
            })
            .collect();
        ZoneState {
            tick: self.tick,
            next_id: self.next_id,
            entities,
            rng: self.rng.state(),
            respawns: self.respawns.clone(),
            item_ids: self.item_ids.as_ref().map(IdBlock::bounds),
            social: self.social.clone(),
            ghosts: self.ghosts.clone(),
            escrows: postcard::to_stdvec(&self.escrows).unwrap_or_default(),
            now_unix: self.now_unix,
        }
    }

    /// Stable hash of the whole state (compared by replay and by the CI).
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHash::new();
        // One buffer for every `postcard` encoding: the hash runs every
        // tick, and a fresh vector per component was most of its cost.
        let mut buf = Vec::new();
        h.write_u64(self.tick.0);
        h.write_u64(u64::from(self.next_id));
        h.write_u64(self.rng.state());
        for (t, s) in &self.respawns {
            h.write_u64(*t);
            h.write_u64(u64::from(*s));
        }
        if let Some((a, b)) = self.item_ids.as_ref().map(IdBlock::bounds) {
            h.write_u64(a);
            h.write_u64(b);
        }
        hash_encoded(&mut h, &mut buf, &self.social);
        hash_encoded(&mut h, &mut buf, &self.ghosts);
        hash_encoded(&mut h, &mut buf, &self.escrows);
        for &handle in self.by_id.values() {
            if let Ok(b) = self.world.get::<&Body>(handle) {
                h.write_u64(u64::from(b.id.get()));
                h.write_i32(b.pos.x);
                h.write_i32(b.pos.y);
                h.write(&[b.facing as u8]);
                h.write_u64(b.moved_tick);
            }
            if let Ok(w) = self.world.get::<&Walk>(handle) {
                h.write_u64(w.ready);
                h.write_u64(w.steps.len() as u64);
                for s in &w.steps {
                    h.write_i32(s.x);
                    h.write_i32(s.y);
                }
            }
            if let Ok(p) = self.world.get::<&PlayerComp>(handle) {
                h.write_u64(u64::from(p.player.get()));
                h.write_str(p.sheet.name.as_str());
                hash_encoded(&mut h, &mut buf, &p.data);
                h.write_u64(p.known.len() as u64);
                for k in &p.known {
                    h.write_u64(u64::from(k.get()));
                }
            }
            if let Ok(n) = self.world.get::<&NpcComp>(handle) {
                h.write_u64(u64::from(n.npc.get()));
                h.write_u64(u64::from(n.name.get()));
                h.write_u64(match n.dialogue {
                    Some(d) => u64::from(d.get()) + 1,
                    None => 0,
                });
            }
            h.write_u64(self.world.get::<&Body>(handle).map_or(0, |b| b.status_tick));
            if let Ok(s) = self.world.get::<&ShopComp>(handle) {
                hash_encoded(&mut h, &mut buf, &*s);
            }
            if let Ok(o) = self.world.get::<&ShopOpen>(handle) {
                hash_encoded(&mut h, &mut buf, &*o);
            }
            if let Ok(n) = self.world.get::<&NodeComp>(handle) {
                hash_encoded(&mut h, &mut buf, &*n);
            }
            if let Ok(f) = self.world.get::<&Fighter>(handle) {
                hash_encoded(&mut h, &mut buf, &*f);
            }
            if let Ok(m) = self.world.get::<&MonsterComp>(handle) {
                hash_encoded(&mut h, &mut buf, &*m);
            }
            if let Ok(c) = self.world.get::<&crate::companion::CompanionComp>(handle) {
                hash_encoded(&mut h, &mut buf, &*c);
            }
            if let Ok(g) = self.world.get::<&Gear>(handle) {
                hash_encoded(&mut h, &mut buf, &*g);
            }
            if let Ok(p) = self.world.get::<&PlayerComp>(handle)
                && hash_encoded(&mut h, &mut buf, &p.growth)
            {
                h.write_u64(p.locked_until);
            }
            if let Ok(d) = self.world.get::<&DialogueComp>(handle) {
                h.write_u64(u64::from(d.npc.get()));
                h.write_u64(u64::from(d.dialogue.get()));
                h.write_u64(u64::from(d.node));
                h.write_u64(d.shown.len() as u64);
            }
        }
        h.finish()
    }
}

/// Feeds `value`'s `postcard` bytes to `h` (the bytes `to_stdvec` gives),
/// encoded into `buf`, which is kept for the next one. False when it does
/// not encode (then nothing is written, as before).
fn hash_encoded<T: serde::Serialize + ?Sized>(h: &mut StateHash, buf: &mut Vec<u8>, value: &T) -> bool {
    buf.clear();
    match postcard::to_extend(value, std::mem::take(buf)) {
        Ok(bytes) => {
            h.write(&bytes);
            *buf = bytes;
            true
        }
        Err(_) => false,
    }
}

//! Content swapped under a running zone: players keep
//! everything, NPCs and monsters are placed again from the new files, and
//! a repainted map of the same size applies at once.

use super::*;

impl Zone {
    /// Swaps in new content: players keep everything (converted
    /// by text id), open dialogues close, NPCs spawn again from the new
    /// `npcs.ron`. The map stays; a new map needs a restart. Refused when
    /// the new content lacks an item, quest or NPC the running one has.
    pub fn reload(&mut self, content: Arc<Content>, out: &mut TickOutput) -> Result<(), ZoneError> {
        let (old, new) = (&self.content.catalog, &content.catalog);
        let missing = old
            .items()
            .iter()
            .map(|d| ("item", &d.id, new.item_by_name(&d.id).is_some()))
            .chain(
                old.quests()
                    .iter()
                    .map(|d| ("quest", &d.id, new.quest_by_name(&d.id).is_some())),
            )
            .chain(
                old.npcs()
                    .map(|(_, d)| ("npc", &d.id, new.npc_by_name(&d.id).is_some())),
            )
            .find(|(_, _, found)| !found);
        if let Some((kind, id, _)) = missing {
            return Err(ZoneError::Removed(format!("{kind} `{id}`")));
        }
        let def = new
            .zone(self.zone_id)
            .ok_or(ZoneError::UnknownZone(self.zone_id))?
            .clone();
        // Players: out with the old ids, in with the new ones.
        let handles: Vec<(PlayerId, hecs::Entity)> = self.by_player.iter().map(|(p, h)| (*p, *h)).collect();
        for (player, handle) in handles {
            let Some(p) = self.export_handle(handle) else {
                continue;
            };
            let mut data = PlayerData::import(&p, new);
            let mut g = Gear::import(&p.items, new);
            place_bag(&mut data, &mut g, new);
            let growth = Growth::import(&p.progress, new);
            if let Ok((comp, gear)) = self.world.query_one_mut::<(&mut PlayerComp, &mut Gear)>(handle) {
                comp.data = data;
                comp.growth = growth;
                comp.dirty_bag = true;
                comp.dirty_quests = true;
                comp.dirty_stats = true;
                comp.dirty_shops = true;
                *gear = g;
            }
            if let Ok(open) = self.world.remove_one::<DialogueComp>(handle) {
                out.dialogue.push(DialogueOut::End {
                    player,
                    npc: open.npc,
                });
            }
            // The shop ids of the new content are other numbers: close it
            // and let the player open it again (phase 7).
            if self.world.remove_one::<ShopOpen>(handle).is_ok() {
                out.shops.push((player, None));
            }
        }
        // NPCs, monsters and nodes: the old ones leave every view, the new
        // ones enter. Content ids are positions in the new lists, so no
        // entity may keep an id from the old content.
        let npcs: Vec<EntityId> = self
            .by_id
            .iter()
            .filter(|(_, h)| {
                self.world.get::<&NpcComp>(**h).is_ok()
                    || self.world.get::<&MonsterComp>(**h).is_ok()
                    || self.world.get::<&NodeComp>(**h).is_ok()
            })
            .map(|(id, _)| *id)
            .collect();
        for id in npcs {
            self.remove_entity(id);
        }
        // Same size and place: the painted tiles apply now (phase 6c); a
        // map that grew or moved needs a restart (the grid has its size).
        if let Ok(map) = crate::seam::world_map(new, self.zone_id)
            && map != self.map
            && (map.origin(), map.width(), map.height())
                == (self.map.origin(), self.map.width(), self.map.height())
        {
            self.map = map;
            // The interest sight cache was taken on the old map.
            self.scratch.observers.clear();
            out.map_changed = true;
        }
        self.content = content;
        self.regions = def.regions.clone();
        self.monster_spawns = def.monsters.clone();
        self.respawns.clear();
        self.spawn_npcs(&def);
        self.spawn_nodes(&def);
        self.spawn_all_monsters();
        let handles: Vec<hecs::Entity> = self.by_player.values().copied().collect();
        for h in handles {
            fight::refresh_player(self, h);
        }
        self.replay = ReplayLog::new(def.name.clone(), self.content.catalog.content_hash(), self.seed);
        let state = self.snapshot();
        self.replay.start(state);
        out.reloaded = true;
        Ok(())
    }
}

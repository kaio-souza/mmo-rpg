//! Dialogue with NPCs: open, pick an option (condition checked again,
//! effects all or nothing), close when the player walks away.
//!

use crate::Content;
use crate::player::{Fail, PlayerData};
use crate::types::{CoinFlow, DialogueOut, Refusal, TickOutput};
use crate::zone::{Body, DialogueComp, NpcComp, PlayerComp, TALK_RADIUS, Zone};
use base::{EntityId, NpcId, PlayerId};
use defs::{Condition, DialogueNode, Effect, ValidatedCatalog};
use std::sync::Arc;

/// What a condition reads: the player's data and their known techniques.
#[derive(Clone, Copy)]
struct Who<'a> {
    data: &'a PlayerData,
    sheet: &'a progression::ProgressionSheet,
    /// The zone's unix second (cooldown of repeatable quests).
    now: u64,
}

fn holds(cat: &ValidatedCatalog, who: Who<'_>, npc: NpcId, c: &Condition) -> bool {
    let data = who.data;
    match c {
        Condition::Item { item, qty } => data.count(*item) >= u64::from(*qty),
        Condition::Coins(n) => data.coins >= *n,
        Condition::Quest { quest, state } => data.is(cat, *quest, *state, who.now),
        Condition::QuestReady(q) => data.ready(cat, *q),
        Condition::Affinity(min) => data.affinity(npc) >= *min,
        Condition::All(v) => v.iter().all(|c| holds(cat, who, npc, c)),
        Condition::Any(v) => v.iter().any(|c| holds(cat, who, npc, c)),
        Condition::Not(c) => !holds(cat, who, npc, c),
        Condition::RecipeKnown(r) => data.knows_recipe(*r),
        Condition::TechniqueKnown(t) => who.sheet.technique(*t).is_some(),
    }
}

/// Positions (in the node) of the options the player may see.
fn visible(cat: &ValidatedCatalog, who: Who<'_>, npc: NpcId, node: &DialogueNode) -> Vec<u16> {
    node.options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.when.as_ref().is_none_or(|c| holds(cat, who, npc, c)))
        .filter_map(|(i, _)| u16::try_from(i).ok())
        .collect()
}

fn fail(f: Fail) -> Refusal {
    match f {
        Fail::Full => Refusal::InventoryFull,
        Fail::NotPossible => Refusal::RequirementsNotMet,
    }
}

impl Zone {
    fn npc_of(&self, entity: EntityId) -> Option<NpcId> {
        let h = *self.by_id.get(&entity)?;
        self.world.get::<&NpcComp>(h).ok().map(|n| n.npc)
    }

    /// Opens the start node of the target NPC's dialogue (replacing any
    /// open one) and counts the talk for quests.
    pub(crate) fn talk(&mut self, player: PlayerId, target: EntityId) -> Result<DialogueOut, Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::NoDialogue)?;
        let pos = self
            .world
            .get::<&Body>(handle)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NoDialogue)?;
        let target_handle = *self.by_id.get(&target).ok_or(Refusal::NoDialogue)?;
        let target_pos = self
            .world
            .get::<&Body>(target_handle)
            .map(|b| b.pos)
            .map_err(|_| Refusal::NoDialogue)?;
        let (npc, dialogue) = self
            .world
            .get::<&NpcComp>(target_handle)
            .ok()
            .and_then(|n| Some((n.npc, n.dialogue?)))
            .ok_or(Refusal::NoDialogue)?;
        if pos.distance(target_pos) > TALK_RADIUS {
            return Err(Refusal::TooFar);
        }
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.dialogue(dialogue).ok_or(Refusal::NoDialogue)?;
        let node = def.nodes.get(usize::from(def.start)).ok_or(Refusal::NoDialogue)?;
        let p = self
            .world
            .query_one_mut::<&mut PlayerComp>(handle)
            .map_err(|_| Refusal::NoDialogue)?;
        if p.data.talked_to(cat, npc) {
            p.dirty_quests = true;
        }
        let who = Who {
            data: &p.data,
            sheet: &p.growth.sheet,
            now: self.now_unix,
        };
        let shown = visible(cat, who, npc, node);
        let out = DialogueOut::Show {
            player,
            npc: target,
            text: node.text,
            options: shown
                .iter()
                .filter_map(|&i| node.options.get(usize::from(i)))
                .map(|o| o.text)
                .collect(),
        };
        let open = DialogueComp {
            npc: target,
            dialogue,
            node: def.start,
            shown,
        };
        self.world
            .insert_one(handle, open)
            .map_err(|_| Refusal::NoDialogue)?;
        Ok(out)
    }

    /// Follows an option of the open dialogue: effects, then the next node
    /// or the end.
    pub(crate) fn choose(&mut self, player: PlayerId, option: u16) -> Result<DialogueOut, Refusal> {
        let handle = *self.by_player.get(&player).ok_or(Refusal::NoDialogue)?;
        let (target, dialogue, node, index) = self
            .world
            .get::<&DialogueComp>(handle)
            .map_err(|_| Refusal::NoDialogue)
            .and_then(|o| {
                let index = *o.shown.get(usize::from(option)).ok_or(Refusal::NoDialogue)?;
                Ok((o.npc, o.dialogue, o.node, index))
            })?;
        let npc = self.npc_of(target).ok_or(Refusal::NoDialogue)?;
        let content = self.content.clone();
        let cat = &content.catalog;
        let def = cat.dialogue(dialogue).ok_or(Refusal::NoDialogue)?;
        let chosen = def
            .nodes
            .get(usize::from(node))
            .and_then(|n| n.options.get(usize::from(index)))
            .ok_or(Refusal::NoDialogue)?;
        let (current, sheet) = self
            .world
            .get::<&PlayerComp>(handle)
            .map(|p| (p.data.clone(), p.growth.sheet.clone()))
            .map_err(|_| Refusal::NoDialogue)?;
        // Checked again: the backpack or a quest may have changed since shown.
        let who = Who {
            data: &current,
            sheet: &sheet,
            now: self.now_unix,
        };
        if chosen.when.as_ref().is_some_and(|c| !holds(cat, who, npc, c)) {
            return Err(Refusal::RequirementsNotMet);
        }
        let mut data = current.clone();
        // Techniques live in the sheet: taught once the rest succeeded.
        let mut taught = Vec::new();
        let mut teleport = None;
        for effect in &chosen.effects {
            match effect {
                Effect::LearnTechnique(t) => taught.push(*t),
                Effect::Teleport(p) => teleport = Some(*p),
                Effect::CompleteQuest(q) => {
                    data = self.apply_effect(&content, npc, data, effect)?;
                    taught.extend(cat.quest(*q).into_iter().flat_map(|d| d.techniques()));
                }
                _ => data = self.apply_effect(&content, npc, data, effect)?,
            }
        }
        let next = chosen
            .next
            .and_then(|n| def.nodes.get(usize::from(n)).map(|node| (n, node)));
        let p = self
            .world
            .query_one_mut::<&mut PlayerComp>(handle)
            .map_err(|_| Refusal::NoDialogue)?;
        if data != current {
            p.dirty_bag |= data.coins != current.coins || data.slots != current.slots;
            // Item counts reach the log through `dirty_bag`.
            p.dirty_quests |= data.quests != current.quests;
            // Known recipes travel in the sheet: a lesson shows in the
            // crafting window at once, not at the next sheet change.
            p.dirty_stats |= data.recipes != current.recipes;
            p.data = data;
        }
        for t in taught {
            if progression::learn_technique(&mut p.growth.sheet, t).is_some() {
                p.dirty_stats = true;
            }
        }
        // A teleport ends the talk: the player is somewhere else now.
        if let Some(to) = teleport {
            self.teleports.push((player, to));
            let _ = self.world.remove_one::<DialogueComp>(handle);
            return Ok(DialogueOut::End { player, npc: target });
        }
        let Some((next, next_node)) = next else {
            let _ = self.world.remove_one::<DialogueComp>(handle);
            return Ok(DialogueOut::End { player, npc: target });
        };
        let who = Who {
            data: &p.data,
            sheet: &p.growth.sheet,
            now: self.now_unix,
        };
        let shown = visible(cat, who, npc, next_node);
        let out = DialogueOut::Show {
            player,
            npc: target,
            text: next_node.text,
            options: shown
                .iter()
                .filter_map(|&i| next_node.options.get(usize::from(i)))
                .map(|o| o.text)
                .collect(),
        };
        if let Ok(open) = self.world.query_one_mut::<&mut DialogueComp>(handle) {
            open.node = next;
            open.shown = shown;
        }
        Ok(out)
    }

    fn apply_effect(
        &mut self,
        content: &Arc<Content>,
        npc: NpcId,
        mut data: PlayerData,
        effect: &Effect,
    ) -> Result<PlayerData, Refusal> {
        let cat = &content.catalog;
        match effect {
            Effect::GiveItem { item, qty } => data.give(cat, *item, *qty).map_err(fail)?,
            Effect::TakeItem { item, qty } => data.take(*item, *qty).map_err(fail)?,
            Effect::GiveCoins(n) => data.gain_coins(CoinFlow::Gift, *n),
            Effect::TakeCoins(n) => data
                .spend_coins(CoinFlow::Fee, *n)
                .map_err(|_| Refusal::RequirementsNotMet)?,
            Effect::LearnRecipe(r) => {
                data.learn(*r);
            }
            // Applied by `choose` on the sheet.
            Effect::LearnTechnique(_) | Effect::Teleport(_) => {}
            Effect::StartQuest(q) => data.start(cat, *q, self.now_unix).map_err(fail)?,
            Effect::CompleteQuest(q) => data.complete(cat, *q, self.now_unix).map_err(fail)?,
            Effect::Affinity(d) => data.change_affinity(npc, *d),
            Effect::TopUpItem { item, qty } => {
                let missing = u64::from(*qty).saturating_sub(data.count(*item));
                if missing > 0 {
                    let missing = u32::try_from(missing).unwrap_or(*qty);
                    data.give(cat, *item, missing).map_err(fail)?;
                }
            }
        }
        Ok(data)
    }

    /// Closes the dialogues of players who walked out of the talk radius,
    /// in player order (deterministic).
    pub(crate) fn close_far_dialogues(&mut self, out: &mut TickOutput) {
        let mut far = Vec::new();
        for (&player, &handle) in &self.by_player {
            let Ok(open) = self.world.get::<&DialogueComp>(handle) else {
                continue;
            };
            let npc = open.npc;
            let near = match (self.world.get::<&Body>(handle), self.by_id.get(&npc)) {
                (Ok(me), Some(&h)) => self
                    .world
                    .get::<&Body>(h)
                    .is_ok_and(|b| me.pos.distance(b.pos) <= TALK_RADIUS),
                _ => false,
            };
            if !near {
                far.push((player, handle, npc));
            }
        }
        for (player, handle, npc) in far {
            let _ = self.world.remove_one::<DialogueComp>(handle);
            out.dialogue.push(DialogueOut::End { player, npc });
        }
    }
}

//! What a player carries and has done: coins, backpack, quests and
//! affinity (phase 3). Typed by content ids inside the zone;
//! [`CharacterSheet`] (text ids) at the edges, so a content reload or an id
//! the content no longer has never loses anything.

use crate::Content;
use crate::types::{CharacterSheet, CoinFlow, ObjectiveView, QuestMarker, QuestView, SavedQuest, SavedStack};
use base::{DialogueId, ItemId, NpcId, QuestId, RecipeId};
use defs::{AFFINITY_MAX, Effect, Objective, QuestStatus, Reward, ValidatedCatalog};
use serde::{Deserialize, Serialize};

/// Slots of the backpack until backpacks are items (phase 4).
pub const BACKPACK_SLOTS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Slot {
    Empty,
    Stack {
        item: ItemId,
        qty: u32,
    },
    /// An item the running content does not have: kept as it was saved,
    /// takes its slot and cannot be used (: nothing is lost).
    Unknown(SavedStack),
    /// An item with identity sits here; the item itself lives in `Gear`.
    Held(ItemId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QuestEntry {
    pub quest: QuestId,
    pub completed: bool,
    /// Bit `i`: `Talk` objective `i` done (at most `defs::MAX_OBJECTIVES`).
    pub talked: u8,
    /// Unix second of the last completion (0: never).
    pub completed_at: u64,
    /// Count of each objective that counts (kill, craft, reach), by index.
    #[serde(default)]
    pub progress: Vec<u32>,
}

/// Something a player did that quests may count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Deed {
    Killed(base::MonsterId),
    Crafted(ItemId, u32),
    /// Standing on this tile.
    At(base::TilePos),
}

/// Everything effects may change. Cloned to apply an option's effects all
/// or nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlayerData {
    pub coins: u64,
    pub slots: Vec<Slot>,
    /// Recipes known, sorted (phase 8).
    pub recipes: Vec<RecipeId>,
    /// Coins created and destroyed since the last tick flush, with where
    /// they came from (phase 7). Never saved and never hashed: the tick
    /// drains it into `TickOutput::coins`. An effect that rolls back takes
    /// its entries with it, because this list is part of the copy.
    #[serde(skip)]
    pub coin_log: Vec<(CoinFlow, u64)>,
    /// Saved recipe ids the running content does not have.
    pub kept_recipes: Vec<String>,
    /// Sorted by quest.
    pub quests: Vec<QuestEntry>,
    /// Sorted by NPC, never zero.
    pub affinity: Vec<(NpcId, i32)>,
    /// Saved entries whose ids the running content does not have.
    pub kept_quests: Vec<SavedQuest>,
    pub kept_affinity: Vec<(String, i32)>,
    /// `(shop id, coins paid, unix second the purse opened)`.
    pub shop_sales: Vec<(String, u64, u64)>,
}

/// Why a change cannot be done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fail {
    Full,
    NotPossible,
}

impl PlayerData {
    pub fn import(sheet: &CharacterSheet, cat: &ValidatedCatalog) -> PlayerData {
        let mut slots: Vec<Slot> = sheet
            .backpack
            .iter()
            .take(BACKPACK_SLOTS)
            .map(|s| match s {
                None => Slot::Empty,
                Some(s) => match cat.item_by_name(&s.item) {
                    Some((item, _)) if s.qty > 0 => Slot::Stack { item, qty: s.qty },
                    Some(_) => Slot::Empty,
                    None => Slot::Unknown(s.clone()),
                },
            })
            .collect();
        slots.resize(BACKPACK_SLOTS, Slot::Empty);
        let mut quests = Vec::new();
        let mut kept_quests = Vec::new();
        for q in &sheet.quests {
            match cat.quest_by_name(&q.quest) {
                Some((quest, _)) => quests.push(QuestEntry {
                    quest,
                    completed: q.completed,
                    talked: q.talked,
                    completed_at: q.completed_at,
                    progress: q.progress.clone(),
                }),
                None => kept_quests.push(q.clone()),
            }
        }
        quests.sort_by_key(|q| q.quest);
        quests.dedup_by_key(|q| q.quest);
        let mut affinity = Vec::new();
        let mut kept_affinity = Vec::new();
        for (npc, v) in &sheet.affinity {
            match cat.npc_by_name(npc) {
                Some((id, _)) if *v != 0 => affinity.push((id, (*v).clamp(-AFFINITY_MAX, AFFINITY_MAX))),
                Some(_) => {}
                None => kept_affinity.push((npc.clone(), *v)),
            }
        }
        affinity.sort_by_key(|(n, _)| *n);
        affinity.dedup_by_key(|(n, _)| *n);
        let mut recipes = Vec::new();
        let mut kept_recipes = Vec::new();
        for r in &sheet.recipes {
            match cat.recipe_by_name(r) {
                Some((id, _)) => recipes.push(id),
                None => kept_recipes.push(r.clone()),
            }
        }
        recipes.sort();
        recipes.dedup();
        PlayerData {
            coins: sheet.coins,
            slots,
            recipes,
            coin_log: Vec::new(),
            kept_recipes,
            quests,
            affinity,
            kept_quests,
            kept_affinity,
            shop_sales: sheet.shop_sales.clone(),
        }
    }

    /// Coins in, with where they came from (phase 7).
    pub fn gain_coins(&mut self, flow: CoinFlow, n: u64) {
        if n == 0 {
            return;
        }
        self.coins = self.coins.saturating_add(n);
        self.coin_log.push((flow, n));
    }

    /// Coins out; `Err` when the player cannot pay (nothing changes).
    pub fn spend_coins(&mut self, flow: CoinFlow, n: u64) -> Result<(), Fail> {
        if n == 0 {
            return Ok(());
        }
        self.coins = self.coins.checked_sub(n).ok_or(Fail::NotPossible)?;
        self.coin_log.push((flow, n));
        Ok(())
    }

    pub fn knows(&self, recipe: RecipeId) -> bool {
        self.recipes.binary_search(&recipe).is_ok()
    }

    pub fn knows_recipe(&self, recipe: RecipeId) -> bool {
        self.recipes.binary_search(&recipe).is_ok()
    }

    /// Learns a recipe; already known is not an error.
    pub fn learn(&mut self, recipe: RecipeId) -> bool {
        match self.recipes.binary_search(&recipe) {
            Ok(_) => false,
            Err(i) => {
                self.recipes.insert(i, recipe);
                true
            }
        }
    }

    /// Writes coins, backpack, quests and affinity back as text ids.
    pub fn export(&self, cat: &ValidatedCatalog, sheet: &mut CharacterSheet) {
        sheet.coins = self.coins;
        sheet.shop_sales = self.shop_sales.clone();
        sheet.recipes = self
            .recipes
            .iter()
            .filter_map(|r| Some(cat.recipe(*r)?.id.clone()))
            .chain(self.kept_recipes.iter().cloned())
            .collect();
        sheet.backpack = self
            .slots
            .iter()
            .map(|s| match s {
                Slot::Empty => None,
                Slot::Stack { item, qty } => cat.item(*item).map(|d| SavedStack {
                    item: d.id.clone(),
                    qty: *qty,
                }),
                Slot::Unknown(s) => Some(s.clone()),
                Slot::Held(_) => None,
            })
            .collect();
        sheet.quests = self
            .quests
            .iter()
            .filter_map(|q| {
                Some(SavedQuest {
                    quest: cat.quest(q.quest)?.id.clone(),
                    completed: q.completed,
                    talked: q.talked,
                    completed_at: q.completed_at,
                    progress: q.progress.clone(),
                })
            })
            .chain(self.kept_quests.iter().cloned())
            .collect();
        sheet.affinity = self
            .affinity
            .iter()
            .filter_map(|(n, v)| Some((cat.npc(*n)?.id.clone(), *v)))
            .chain(self.kept_affinity.iter().cloned())
            .collect();
    }

    pub fn count(&self, item: ItemId) -> u64 {
        self.slots
            .iter()
            .map(|s| match s {
                Slot::Stack { item: i, qty } if *i == item => u64::from(*qty),
                _ => 0,
            })
            .sum()
    }

    /// Fills stacks of the same item first, then empty slots.
    pub fn give(&mut self, cat: &ValidatedCatalog, item: ItemId, qty: u32) -> Result<(), Fail> {
        let max = cat.item(item).map_or(1, |d| u32::from(d.max_stack));
        let room: u64 = self
            .slots
            .iter()
            .map(|s| match s {
                Slot::Empty => u64::from(max),
                Slot::Stack { item: i, qty } if *i == item => u64::from(max.saturating_sub(*qty)),
                _ => 0,
            })
            .sum();
        if room < u64::from(qty) {
            return Err(Fail::Full);
        }
        let mut left = qty;
        for s in self.slots.iter_mut() {
            if let Slot::Stack { item: i, qty } = s
                && *i == item
            {
                let add = left.min(max.saturating_sub(*qty));
                *qty = (*qty).saturating_add(add);
                left = left.saturating_sub(add);
            }
        }
        for s in self.slots.iter_mut() {
            if left == 0 {
                break;
            }
            if *s == Slot::Empty {
                let add = left.min(max);
                *s = Slot::Stack { item, qty: add };
                left = left.saturating_sub(add);
            }
        }
        Ok(())
    }

    /// Takes from the last slots first.
    pub fn take(&mut self, item: ItemId, qty: u32) -> Result<(), Fail> {
        if self.count(item) < u64::from(qty) {
            return Err(Fail::NotPossible);
        }
        let mut left = qty;
        for s in self.slots.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if let Slot::Stack { item: i, qty } = s
                && *i == item
            {
                let sub = left.min(*qty);
                *qty = (*qty).saturating_sub(sub);
                left = left.saturating_sub(sub);
                if *qty == 0 {
                    *s = Slot::Empty;
                }
            }
        }
        Ok(())
    }

    /// Merges into a stack of the same item (up to the pile size, the rest
    /// stays), or swaps.
    pub fn move_item(&mut self, cat: &ValidatedCatalog, from: usize, to: usize) -> Result<(), Fail> {
        let (Some(Slot::Stack { item, qty }), Some(target)) =
            (self.slots.get(from).cloned(), self.slots.get(to).cloned())
        else {
            return Err(Fail::NotPossible);
        };
        if from == to || matches!(target, Slot::Unknown(_)) {
            return Err(Fail::NotPossible);
        }
        match target {
            Slot::Stack { item: t, qty: tq } if t == item => {
                let max = cat.item(item).map_or(1, |d| u32::from(d.max_stack));
                let moved = qty.min(max.saturating_sub(tq));
                let rest = qty.saturating_sub(moved);
                if let Some(s) = self.slots.get_mut(to) {
                    *s = Slot::Stack {
                        item,
                        qty: tq.saturating_add(moved),
                    };
                }
                if let Some(s) = self.slots.get_mut(from) {
                    *s = if rest == 0 {
                        Slot::Empty
                    } else {
                        Slot::Stack { item, qty: rest }
                    };
                }
            }
            _ => self.slots.swap(from, to),
        }
        Ok(())
    }

    fn quest_entry(&self, quest: QuestId) -> Option<&QuestEntry> {
        self.quests
            .binary_search_by_key(&quest, |q| q.quest)
            .ok()
            .and_then(|i| self.quests.get(i))
    }

    /// `now`: the zone's unix second, for the cooldown of repeatable quests.
    pub fn status(&self, cat: &ValidatedCatalog, quest: QuestId, now: u64) -> QuestStatus {
        match self.quest_entry(quest) {
            None => QuestStatus::NotStarted,
            Some(q) if !q.completed => QuestStatus::Active,
            // A repeatable quest may start again once completed and its
            // cooldown is over.
            Some(q)
                if cat.quest(quest).is_some_and(|d| {
                    d.repeatable && now >= q.completed_at.saturating_add(u64::from(d.cooldown_secs))
                }) =>
            {
                QuestStatus::NotStarted
            }
            Some(_) => QuestStatus::Completed,
        }
    }

    /// Whether the cooldown of a completed repeatable quest ended in
    /// `(from, to]`: the quest is offered again, so markers and the log
    /// change without any command.
    pub fn cooldown_ended(&self, cat: &ValidatedCatalog, from: u64, to: u64) -> bool {
        self.quests.iter().filter(|q| q.completed).any(|q| {
            cat.quest(q.quest).is_some_and(|d| {
                let end = q.completed_at.saturating_add(u64::from(d.cooldown_secs));
                d.repeatable && from < end && end <= to
            })
        })
    }

    /// `Quest(state)` condition: a completed repeatable quest is both.
    pub fn is(&self, cat: &ValidatedCatalog, quest: QuestId, state: QuestStatus, now: u64) -> bool {
        let s = self.status(cat, quest, now);
        s == state
            || (state == QuestStatus::Completed && self.quest_entry(quest).is_some_and(|q| q.completed))
    }

    pub fn ready(&self, cat: &ValidatedCatalog, quest: QuestId) -> bool {
        let (Some(q), Some(def)) = (self.quest_entry(quest), cat.quest(quest)) else {
            return false;
        };
        !q.completed
            && def.objectives.iter().enumerate().all(|(i, o)| match o {
                Objective::Item { item, qty } => self.count(*item) >= u64::from(*qty),
                Objective::Talk { .. } => q.talked & (1 << i) != 0,
                Objective::Kill { .. } | Objective::Craft { .. } | Objective::Reach { .. } => {
                    q.progress.get(i).copied().unwrap_or(0) >= o.needed()
                }
            })
    }

    /// Counts a deed for every active quest that asks for it; true when
    /// some count changed.
    pub fn deed(&mut self, cat: &ValidatedCatalog, deed: Deed) -> bool {
        let mut changed = false;
        for q in self.quests.iter_mut().filter(|q| !q.completed) {
            let Some(def) = cat.quest(q.quest) else { continue };
            for (i, o) in def.objectives.iter().enumerate() {
                let add = match (o, deed) {
                    (Objective::Kill { monster, .. }, Deed::Killed(m)) if *monster == m => 1,
                    (Objective::Craft { item, .. }, Deed::Crafted(it, n)) if *item == it => n,
                    (Objective::Reach { pos, radius }, Deed::At(at))
                        if pos.distance(at) <= u32::from(*radius) =>
                    {
                        1
                    }
                    _ => continue,
                };
                if q.progress.len() <= i {
                    q.progress.resize(i.saturating_add(1), 0);
                }
                if let Some(c) = q.progress.get_mut(i)
                    && *c < o.needed()
                {
                    *c = c.saturating_add(add).min(o.needed());
                    changed = true;
                }
            }
        }
        changed
    }

    /// Some active quest has a place to reach.
    pub fn travelling(&self, cat: &ValidatedCatalog) -> bool {
        self.quests.iter().filter(|q| !q.completed).any(|q| {
            cat.quest(q.quest)
                .is_some_and(|d| d.objectives.iter().any(|o| matches!(o, Objective::Reach { .. })))
        })
    }

    pub fn start(&mut self, cat: &ValidatedCatalog, quest: QuestId, now: u64) -> Result<(), Fail> {
        if self.status(cat, quest, now) != QuestStatus::NotStarted || cat.quest(quest).is_none() {
            return Err(Fail::NotPossible);
        }
        let completed_at = self.quest_entry(quest).map_or(0, |q| q.completed_at);
        let entry = QuestEntry {
            quest,
            completed: false,
            talked: 0,
            completed_at,
            progress: Vec::new(),
        };
        match self.quests.binary_search_by_key(&quest, |q| q.quest) {
            Ok(i) => {
                if let Some(q) = self.quests.get_mut(i) {
                    *q = entry;
                }
            }
            Err(i) => self.quests.insert(i, entry),
        }
        Ok(())
    }

    /// Takes the items of `Item` objectives, gives the rewards (techniques
    /// are taught by the caller, which holds the sheet).
    pub fn complete(&mut self, cat: &ValidatedCatalog, quest: QuestId, now: u64) -> Result<(), Fail> {
        if !self.ready(cat, quest) {
            return Err(Fail::NotPossible);
        }
        let def = cat.quest(quest).ok_or(Fail::NotPossible)?;
        for o in &def.objectives {
            if let Objective::Item { item, qty } = o {
                self.take(*item, *qty)?;
            }
        }
        for r in &def.rewards {
            match r {
                Reward::Item { item, qty } => self.give(cat, *item, *qty)?,
                Reward::Coins(n) => self.gain_coins(CoinFlow::QuestReward, *n),
                // The sheet is not here: whoever completes the quest
                // teaches `QuestDef::techniques` (`dialogue::choose`).
                Reward::Technique(_) => {}
            }
        }
        if let Ok(i) = self.quests.binary_search_by_key(&quest, |q| q.quest)
            && let Some(q) = self.quests.get_mut(i)
        {
            q.completed = true;
            q.completed_at = now;
        }
        Ok(())
    }

    /// What this player has to do with an NPC: hand in a quest (`Ready`),
    /// a `Talk` objective not yet done, or a quest to start, in that
    /// order. Any option of the NPC's dialogue counts, whatever its
    /// condition: the marker says the quest is there, not how to reach it.
    pub fn marker(
        &self,
        cat: &ValidatedCatalog,
        npc: NpcId,
        dialogue: Option<DialogueId>,
        now: u64,
    ) -> Option<QuestMarker> {
        let effects = || {
            dialogue
                .and_then(|d| cat.dialogue(d))
                .into_iter()
                .flat_map(|d| d.nodes.iter())
                .flat_map(|n| n.options.iter())
                .flat_map(|o| o.effects.iter())
        };
        if effects().any(|e| matches!(e, Effect::CompleteQuest(q) if self.ready(cat, *q))) {
            return Some(QuestMarker::Ready);
        }
        let talk = self.quests.iter().filter(|q| !q.completed).any(|q| {
            cat.quest(q.quest).is_some_and(|def| {
                def.objectives.iter().enumerate().any(
                    |(i, o)| matches!(o, Objective::Talk { npc: n } if *n == npc && q.talked & (1 << i) == 0),
                )
            })
        });
        if talk {
            return Some(QuestMarker::Talk);
        }
        effects()
            .any(|e| matches!(e, Effect::StartQuest(q) if self.status(cat, *q, now) == QuestStatus::NotStarted))
            .then_some(QuestMarker::Available)
    }

    /// The item of a backpack slot, if it is a plain stack.
    pub fn stack_at(&self, slot: usize) -> Option<ItemId> {
        match self.slots.get(slot) {
            Some(Slot::Stack { item, .. }) => Some(*item),
            _ => None,
        }
    }

    /// Marks `Talk` objectives with this NPC in active quests; true if any changed.
    pub fn talked_to(&mut self, cat: &ValidatedCatalog, npc: NpcId) -> bool {
        let mut changed = false;
        for q in self.quests.iter_mut().filter(|q| !q.completed) {
            let Some(def) = cat.quest(q.quest) else { continue };
            for (i, o) in def.objectives.iter().enumerate() {
                if matches!(o, Objective::Talk { npc: n } if *n == npc) && q.talked & (1 << i) == 0 {
                    q.talked |= 1 << i;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn affinity(&self, npc: NpcId) -> i32 {
        self.affinity
            .binary_search_by_key(&npc, |(n, _)| *n)
            .ok()
            .and_then(|i| self.affinity.get(i))
            .map_or(0, |(_, v)| *v)
    }

    pub fn change_affinity(&mut self, npc: NpcId, delta: i32) {
        let v = self
            .affinity(npc)
            .saturating_add(delta)
            .clamp(-AFFINITY_MAX, AFFINITY_MAX);
        match self.affinity.binary_search_by_key(&npc, |(n, _)| *n) {
            Ok(i) if v == 0 => {
                self.affinity.remove(i);
            }
            Ok(i) => {
                if let Some(e) = self.affinity.get_mut(i) {
                    e.1 = v;
                }
            }
            Err(_) if v == 0 => {}
            Err(i) => self.affinity.insert(i, (npc, v)),
        }
    }

    pub fn inventory_view(&self) -> crate::InventoryView {
        crate::InventoryView {
            coins: self.coins,
            slots: self
                .slots
                .iter()
                .map(|s| match s {
                    Slot::Stack { item, qty } => Some((*item, *qty)),
                    Slot::Held(item) => Some((*item, 1)),
                    _ => None,
                })
                .collect(),
            instances: Vec::new(),
            equipped: Vec::new(),
        }
    }

    /// The quest log; `from`: where the player stands, for each quest's
    /// next place.
    pub fn quest_views(&self, content: &Content, from: Option<base::TilePos>) -> Vec<QuestView> {
        let cat = &content.catalog;
        self.quests
            .iter()
            .filter_map(|q| {
                let def = cat.quest(q.quest)?;
                let objectives = def
                    .objectives
                    .iter()
                    .enumerate()
                    .map(|(i, o)| match o {
                        Objective::Item { item, qty } => ObjectiveView::Item {
                            item: *item,
                            have: u32::try_from(self.count(*item)).unwrap_or(u32::MAX),
                            need: *qty,
                        },
                        Objective::Talk { npc } => ObjectiveView::Talk {
                            npc: cat.npc(*npc).map_or(base::TextId::new(0), |n| n.name),
                            done: q.completed || q.talked & (1 << i) != 0,
                        },
                        Objective::Kill { monster, qty } => ObjectiveView::Kill {
                            monster: cat.monster(*monster).map_or(base::TextId::new(0), |m| m.name),
                            have: if q.completed {
                                *qty
                            } else {
                                q.progress.get(i).copied().unwrap_or(0)
                            },
                            need: *qty,
                        },
                        Objective::Craft { item, qty } => ObjectiveView::Craft {
                            item: *item,
                            have: if q.completed {
                                *qty
                            } else {
                                q.progress.get(i).copied().unwrap_or(0)
                            },
                            need: *qty,
                        },
                        Objective::Reach { pos, .. } => ObjectiveView::Reach {
                            pos: *pos,
                            done: q.completed || q.progress.get(i).copied().unwrap_or(0) > 0,
                        },
                    })
                    .collect();
                let goal = match from {
                    Some(from) if !q.completed => crate::goal::quest_goal(
                        cat,
                        q.quest,
                        |i| {
                            q.talked & (1 << i) != 0
                                || def
                                    .objectives
                                    .get(i)
                                    .is_some_and(|o| q.progress.get(i).copied().unwrap_or(0) >= o.needed())
                        },
                        |item| self.count(item),
                        from,
                    ),
                    _ => None,
                };
                Some(QuestView {
                    name: def.name,
                    description: def.description,
                    completed: q.completed,
                    objectives,
                    goal,
                })
            })
            .collect()
    }
}

//! Items with identity (phase 4): equipment equipped or in the
//! backpack. `ItemInstance` has no `Clone`: it moves by value, so the
//! simulation cannot copy an item. New instances take ids from the block
//! the binary reserved in the database (`Command::ItemIds`).

use crate::types::{EquipSlot, InstanceView, ItemPlace, SavedItem, SavedPower};
use base::{
    Attribute, CharacterName, IdBlock, ItemId, ItemInstanceId, ItemPower, Rarity, Rng64, TechniqueId,
};
use defs::{Attributes, EconomyFormulas, Equip, ValidatedCatalog};
use serde::{Deserialize, Serialize};

/// Below this many ids left the zone asks for a new block.
pub const LOW_ITEM_IDS: u64 = 64;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ItemInstance {
    pub id: ItemInstanceId,
    pub item: ItemId,
    pub hidden: Vec<(Attribute, u16)>,
    pub appraised: bool,
    /// Phase 7: blows left and the maximum of this piece. A repair at an
    /// NPC may lower the maximum, so it travels with the piece.
    pub durability: u32,
    pub max_durability: u32,
    /// Phase 8: who crafted it; shown on the item for good.
    pub crafter: Option<CharacterName>,
    /// Rarity of this piece: rolled when it was made.
    pub rarity: Rarity,
    /// Phase 10: mastery of its crafter, in hundredths of a
    /// percent of extra durability.
    pub mastery: u16,
    /// Phase 10: the power it rolled (Unique and above).
    pub power: Option<ItemPower>,
}

impl ItemInstance {
    /// A new item: id from the block, hidden bonuses maybe rolled, full
    /// durability and the item's own rarity.
    pub fn create(
        item: ItemId,
        cat: &ValidatedCatalog,
        ids: &mut IdBlock,
        rng: &mut Rng64,
    ) -> Option<ItemInstance> {
        let def = cat.item(item)?;
        let id = ids.take()?;
        let mut inst = ItemInstance {
            id,
            item,
            hidden: Vec::new(),
            appraised: false,
            durability: def.durability,
            max_durability: def.durability,
            crafter: None,
            rarity: def.rarity,
            mastery: 0,
            power: None,
        };
        inst.shape(cat, rng);
        Some(inst)
    }

    /// Sets what the rarity and the mastery give: full durability over the
    /// item's own, and maybe one of the item's powers. Called
    /// when the piece is made, again once a craft has rolled its rarity.
    pub fn shape(&mut self, cat: &ValidatedCatalog, rng: &mut Rng64) {
        let Some(def) = cat.item(self.item) else { return };
        let e = &cat.formulas().economy;
        let rarity = u64::from(EconomyFormulas::by_rarity(
            &e.rarity_durability_permille,
            self.rarity,
        ));
        let max = combat::permille(
            u64::from(def.durability),
            u32::try_from(rarity).unwrap_or(u32::MAX).saturating_add(1000),
        );
        let max = max.saturating_mul(u64::from(self.mastery).saturating_add(10_000)) / 10_000;
        self.max_durability = u32::try_from(max).unwrap_or(u32::MAX);
        self.durability = self.max_durability;
        // Hidden attributes: only some common pieces carry them; Epic and
        // up always.
        let chance = EconomyFormulas::by_rarity(&e.hidden_chance_permille, self.rarity);
        self.hidden = if chance > 0 && rng.chance(chance) {
            def.hidden
                .iter()
                .map(|h| {
                    (
                        h.attribute,
                        rng.range(u32::from(h.min.max(1).min(h.max)), u32::from(h.max)) as u16,
                    )
                })
                .filter(|(_, v)| *v > 0)
                .collect()
        } else {
            Vec::new()
        };
        let chance = EconomyFormulas::by_rarity(&e.power_chance_permille, self.rarity);
        self.power = None;
        if !def.powers.is_empty() && chance > 0 && rng.chance(chance) {
            let n = u32::try_from(def.powers.len()).unwrap_or(u32::MAX);
            let i = rng.range(0, n.saturating_sub(1)) as usize;
            self.power = def.powers.get(i).copied();
        }
    }

    /// Damage of a weapon or defense of an armor added by the rarity, in
    /// thousandths.
    pub fn power_permille(&self, cat: &ValidatedCatalog) -> u32 {
        EconomyFormulas::by_rarity(&cat.formulas().economy.rarity_power_permille, self.rarity)
    }

    /// What the player sees of this piece in `slot`.
    pub fn view(&self, slot: u16, cat: &ValidatedCatalog) -> InstanceView {
        InstanceView {
            slot,
            id: self.id.get(),
            hidden: self.appraised.then(|| self.hidden.clone()),
            durability: (self.durability, self.max_durability),
            crafter: self.crafter.clone(),
            rarity: self.rarity,
            power_permille: self.power_permille(cat),
            mastery: self.mastery,
            power: self.power,
        }
    }

    /// Wears the piece by `loss`; `true` when it just broke (phase 7).
    pub fn wear(&mut self, loss: u32) -> bool {
        if self.max_durability == 0 {
            return false;
        }
        self.durability = self.durability.saturating_sub(loss);
        self.durability == 0
    }

    /// Shares of the maximum this blow just crossed downwards, so the
    /// System warns once per threshold (`warn_durability_permille`).
    pub fn crossed(&self, before: u32, permille: u32) -> bool {
        if self.max_durability == 0 || permille == 0 {
            return false;
        }
        let at = combat::permille(u64::from(self.max_durability), permille);
        u64::from(before) > at && u64::from(self.durability) <= at
    }

    /// Repairs to the full maximum, which `max_loss_permille` of it lowers
    /// first: an NPC repair costs maximum durability, so a piece repaired
    /// again and again finally dies
    pub fn repair(&mut self, max_loss_permille: u32) {
        if self.max_durability == 0 {
            return;
        }
        let permille = u64::from(max_loss_permille.min(1000));
        // Rounded up, so a cheap piece with a small maximum also pays.
        let loss = u32::try_from(
            u64::from(self.max_durability)
                .saturating_mul(permille)
                .div_ceil(1000),
        )
        .unwrap_or(0);
        // Never below one blow: a repaired piece is always usable.
        self.max_durability = self.max_durability.saturating_sub(loss).max(1);
        self.durability = self.max_durability;
    }

    pub fn save(&self, cat: &ValidatedCatalog, place: ItemPlace) -> Option<SavedItem> {
        Some(SavedItem {
            id: self.id.get(),
            item: cat.item(self.item)?.id.clone(),
            place,
            hidden: self.hidden.clone(),
            appraised: self.appraised,
            durability: (self.durability, self.max_durability),
            crafter: self.crafter.as_ref().map(|c| c.to_string()),
            rarity: Some(self.rarity),
            mastery: self.mastery,
            power: self.power.and_then(|p| save_power(p, cat)),
        })
    }
}

/// Equipment and the backpack's items with identity of one player.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Gear {
    /// Sorted by slot.
    pub equipped: Vec<(EquipSlot, ItemInstance)>,
    /// By backpack slot (the slot holds `Slot::Held`).
    pub bag: Vec<(u16, ItemInstance)>,
    /// Saved items the running content does not have, or that found no
    /// room: kept as saved.
    pub kept: Vec<SavedItem>,
}

impl Gear {
    /// Splits the saved items; the backpack places are checked by the zone.
    pub fn import(items: &[SavedItem], cat: &ValidatedCatalog) -> Gear {
        let mut g = Gear::default();
        for s in items {
            let Some((item, def)) = cat.item_by_name(&s.item) else {
                g.kept.push(s.clone());
                continue;
            };
            // A piece saved before phase 7 has no durability of its own: it
            // takes the item's, whole, and keeps playing.
            let (durability, max_durability) = match s.durability {
                (0, 0) => (def.durability, def.durability),
                (cur, max) => (cur.min(max), max),
            };
            let inst = ItemInstance {
                id: ItemInstanceId::new(s.id),
                item,
                hidden: s.hidden.clone(),
                appraised: s.appraised,
                durability,
                max_durability,
                crafter: s.crafter.as_deref().and_then(|c| CharacterName::try_from(c).ok()),
                rarity: s.rarity.unwrap_or(def.rarity),
                mastery: s.mastery,
                // A power whose technique left the content is dropped.
                power: s.power.as_ref().and_then(|p| load_power(p, cat)),
            };
            match s.place {
                ItemPlace::Equipped(slot)
                    if slot_of(def.equip.as_ref()) == Some(slot) && g.slot(slot).is_none() =>
                {
                    g.equipped.push((slot, inst));
                }
                ItemPlace::Equipped(_) => g.bag.push((u16::MAX, inst)),
                ItemPlace::Backpack(i) => g.bag.push((i, inst)),
            }
        }
        g.equipped.sort_by_key(|(s, _)| *s);
        g
    }

    pub fn export(&self, cat: &ValidatedCatalog) -> Vec<SavedItem> {
        let mut out: Vec<SavedItem> = self
            .equipped
            .iter()
            .filter_map(|(s, i)| i.save(cat, ItemPlace::Equipped(*s)))
            .chain(
                self.bag
                    .iter()
                    .filter_map(|(s, i)| i.save(cat, ItemPlace::Backpack(*s))),
            )
            .collect();
        out.extend(self.kept.iter().cloned());
        out.sort_by_key(|s| s.id);
        out
    }

    pub fn slot(&self, slot: EquipSlot) -> Option<&ItemInstance> {
        self.equipped.iter().find(|(s, _)| *s == slot).map(|(_, i)| i)
    }

    pub fn take_bag(&mut self, slot: u16) -> Option<ItemInstance> {
        let i = self.bag.iter().position(|(s, _)| *s == slot)?;
        Some(self.bag.remove(i).1)
    }

    pub fn bag_item(&mut self, slot: u16) -> Option<&mut ItemInstance> {
        self.bag.iter_mut().find(|(s, _)| *s == slot).map(|(_, i)| i)
    }

    /// Puts `item` in `slot`, returning what was there.
    pub fn put_equipped(&mut self, slot: EquipSlot, item: ItemInstance) -> Option<ItemInstance> {
        let old = self.take_equipped(slot);
        let at = self.equipped.partition_point(|(s, _)| *s < slot);
        self.equipped.insert(at, (slot, item));
        old
    }

    pub fn take_equipped(&mut self, slot: EquipSlot) -> Option<ItemInstance> {
        let i = self.equipped.iter().position(|(s, _)| *s == slot)?;
        Some(self.equipped.remove(i).1)
    }

    /// Appraised bonuses and attribute powers of everything equipped.
    pub fn bonus(&self) -> Attributes {
        let mut a = Attributes::default();
        for (_, i) in &self.equipped {
            if i.appraised {
                for (attr, v) in &i.hidden {
                    let x = a.get_mut(*attr);
                    *x = x.saturating_add(*v);
                }
            }
            if let Some(ItemPower::Attribute(attr, v)) = i.power {
                let x = a.get_mut(attr);
                *x = x.saturating_add(v);
            }
        }
        a
    }

    /// Techniques the equipped pieces lend while worn.
    pub fn lent_techniques(&self) -> Vec<TechniqueId> {
        let mut v: Vec<TechniqueId> = self
            .equipped
            .iter()
            .filter_map(|(_, i)| match i.power {
                Some(ItemPower::Technique(t)) => Some(t),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Damage added by the weapon's rarity, in thousandths.
    pub fn weapon_permille(&self, cat: &ValidatedCatalog) -> u32 {
        self.slot(EquipSlot::Weapon).map_or(0, |i| i.power_permille(cat))
    }

    pub fn equip_def<'a>(&self, slot: EquipSlot, cat: &'a ValidatedCatalog) -> Option<&'a Equip> {
        cat.item(self.slot(slot)?.item)?.equip.as_ref()
    }

    /// The defense of armor and off hand, with what their rarity adds.
    pub fn defense(&self, cat: &ValidatedCatalog) -> u32 {
        let mut total = 0u64;
        for slot in [EquipSlot::Armor, EquipSlot::Offhand] {
            if let (Some(Equip::Armor { defense, .. } | Equip::Offhand { defense, .. }), Some(i)) =
                (self.equip_def(slot, cat), self.slot(slot))
            {
                total = total.saturating_add(combat::permille(
                    u64::from(*defense),
                    i.power_permille(cat).saturating_add(1000),
                ));
            }
        }
        u32::try_from(total).unwrap_or(u32::MAX)
    }

    /// Sprites of weapon, armor and off hand drawn over the body.
    pub fn visuals(&self, cat: &ValidatedCatalog) -> (u16, u16, u16) {
        let v = |slot| {
            self.equip_def(slot, cat)
                .map_or(0, |e| cat.sprite_index(e.visual()))
        };
        (v(EquipSlot::Weapon), v(EquipSlot::Armor), v(EquipSlot::Offhand))
    }

    /// The equipped piece with the brightest light, as `ItemId` + 1 (0:
    /// none); the first slot wins a tie.
    pub fn light(&self, cat: &ValidatedCatalog) -> u32 {
        let mut best: Option<(u8, base::ItemId)> = None;
        for (_, inst) in &self.equipped {
            if let Some(l) = cat.item(inst.item).and_then(|d| d.light)
                && best.is_none_or(|(r, _)| l.radius > r)
            {
                best = Some((l.radius, inst.item));
            }
        }
        best.map_or(0, |(_, i)| i.get().saturating_add(1))
    }
}

fn save_power(p: ItemPower, cat: &ValidatedCatalog) -> Option<SavedPower> {
    Some(match p {
        ItemPower::Technique(t) => SavedPower::Technique(cat.technique(t)?.id.clone()),
        ItemPower::Attribute(a, v) => SavedPower::Attribute(a, v),
    })
}

fn load_power(p: &SavedPower, cat: &ValidatedCatalog) -> Option<ItemPower> {
    Some(match p {
        SavedPower::Technique(t) => ItemPower::Technique(cat.technique_by_name(t)?.0),
        SavedPower::Attribute(a, v) => ItemPower::Attribute(*a, *v),
    })
}

/// Which slot an item goes to.
pub(crate) fn slot_of(e: Option<&Equip>) -> Option<EquipSlot> {
    match e? {
        Equip::Weapon { .. } => Some(EquipSlot::Weapon),
        Equip::Armor { .. } => Some(EquipSlot::Armor),
        Equip::Offhand { .. } => Some(EquipSlot::Offhand),
    }
}

/// Deterministic appraisal: one roll per item and player level, so asking
/// again only helps after leveling. Chance: 30% + 1% per DEX, up to 95%.
pub(crate) fn appraisal_succeeds(item: ItemInstanceId, level: u8, dex: u16, seed: u64) -> bool {
    let chance = u32::from(dex).saturating_mul(10).saturating_add(300).min(950);
    let mut rng = Rng64::new(seed ^ item.get().rotate_left(17) ^ u64::from(level).rotate_left(41));
    rng.chance(chance)
}

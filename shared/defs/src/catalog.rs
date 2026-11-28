use crate::appearance::AppearanceDef;
use crate::craft::{NodeDef, RecipeDef};
use crate::economy::{LootTableDef, ShopDef};
use crate::emote::EmoteDef;
use crate::formulas::{Formulas, parse_formulas};
use crate::item::{self, Equip, EquipFile, ItemDef};
use crate::locales::LocaleDef;
use crate::npc::{self, DialogueDef, NpcDef};
use crate::progress::{
    self, ClassDef, MonsterDef, ProficiencyDef, SpecialAttributeDef, TechniqueDef, TitleBonus, TitleDef,
    TitleTrigger, TitleTriggerFile,
};
use crate::quest::{self, Names, QuestDef};
use crate::zone::{ZoneDef, parse_zone};
use base::{
    ClassId, DialogueId, EmoteId, Fnv64, ItemId, LootTableId, MonsterId, NodeId, NpcId, ProficiencyId,
    QuestId, RecipeId, ShopId, TechniqueId, TextId, TitleId, ZoneId,
};
use i18n::{Bundle, KeyTable, Locale, Severity, TextProblem};
use std::collections::BTreeMap;
use std::fmt;

/// Content as loaded, not yet validated. Only `validate` turns it into
/// something the simulation accepts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// `(locale, [(file name, file text)])`.
    pub(crate) texts: Vec<(Locale, Vec<(String, String)>)>,
    pub(crate) zones: BTreeMap<String, RawZone>,
    /// `content/npcs/<id>.ron`.
    pub(crate) npcs: BTreeMap<String, String>,
    /// `content/npcs/dialogues/<id>.ron`.
    pub(crate) dialogues: BTreeMap<String, String>,
    /// `content/items/<id>.ron`.
    pub(crate) items: BTreeMap<String, String>,
    /// `content/quests/<id>.ron`.
    pub(crate) quests: BTreeMap<String, String>,
    /// `content/moderation/reasons.ron`: text keys an admin may use.
    pub(crate) moderation: Option<String>,
    /// `content/appearance/appearance.ron` (phase 5).
    pub(crate) appearance: Option<String>,
    /// `content/text/locales.ron`: the languages and which are
    /// required.
    pub(crate) locales: Option<String>,
    /// Phase 4: `monsters/`, `classes/`, `proficiencies/`, `techniques/`,
    /// `titles/` (`<id>.ron` each).
    pub(crate) monsters: BTreeMap<String, String>,
    pub(crate) classes: BTreeMap<String, String>,
    pub(crate) proficiencies: BTreeMap<String, String>,
    pub(crate) techniques: BTreeMap<String, String>,
    pub(crate) titles: BTreeMap<String, String>,
    pub(crate) special_attributes: BTreeMap<String, String>,
    /// Phase 7: `loot/`, `shops/`. Phase 8: `recipes/`, `nodes/`.
    pub(crate) loot_tables: BTreeMap<String, String>,
    pub(crate) shops: BTreeMap<String, String>,
    pub(crate) recipes: BTreeMap<String, String>,
    pub(crate) nodes: BTreeMap<String, String>,
    /// Phase 9: `emotes/`.
    pub(crate) emotes: BTreeMap<String, String>,
    /// `rankings/`.
    pub(crate) rankings: BTreeMap<String, String>,
    /// `dungeons/`.
    pub(crate) dungeons: BTreeMap<String, String>,
    /// `goals/`: server goals.
    pub(crate) goals: BTreeMap<String, String>,
    /// `counters.ron` and the three formula files.
    pub(crate) counters: Option<String>,
    pub(crate) progression_formulas: Option<String>,
    pub(crate) combat_formulas: Option<String>,
    pub(crate) economy_formulas: Option<String>,
    /// Every file, sorted by path: the content hash.
    pub(crate) files: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RawZone {
    pub zone: Option<String>,
    pub map: Option<String>,
    pub npcs: Option<String>,
    pub monsters: Option<String>,
    pub nodes: Option<String>,
}

/// Content that passed every check.
#[derive(Debug, Clone)]
pub struct ValidatedCatalog {
    keys: KeyTable,
    /// One per locale, in the order of `locales`.
    bundles: Vec<Bundle>,
    /// `text/locales.ron`.
    locales: Vec<LocaleDef>,
    /// Sorted by name; `ZoneId` is the index.
    zones: Vec<ZoneDef>,
    /// Sorted by id; `NpcId` is the index.
    npcs: Vec<NpcDef>,
    /// Sorted by id; `DialogueId` is the index.
    dialogues: Vec<DialogueDef>,
    /// Sorted by id; `ItemId` is the index.
    items: Vec<ItemDef>,
    /// Sorted by id; `QuestId` is the index.
    quests: Vec<QuestDef>,
    /// Text keys allowed as a moderation reason, in file order.
    moderation_reasons: Vec<String>,
    appearance: AppearanceDef,
    formulas: Formulas,
    /// Sorted by id; the id type is the index.
    monsters: Vec<MonsterDef>,
    classes: Vec<ClassDef>,
    proficiencies: Vec<ProficiencyDef>,
    techniques: Vec<TechniqueDef>,
    titles: Vec<TitleDef>,
    counters: Vec<String>,
    /// Sorted by id.
    special_attributes: Vec<SpecialAttributeDef>,
    /// Phase 7, sorted by id: shared loot tables and NPC shops.
    loot_tables: Vec<LootTableDef>,
    shops: Vec<ShopDef>,
    /// Phase 8, sorted by id: recipes and resource nodes.
    recipes: Vec<RecipeDef>,
    nodes: Vec<NodeDef>,
    /// Phase 9, sorted by id.
    emotes: Vec<EmoteDef>,
    /// Sorted by id.
    rankings: Vec<crate::RankingDef>,
    /// Sorted by id.
    dungeons: Vec<crate::DungeonDef>,
    /// Server goals, sorted by id.
    goals: Vec<crate::GoalDef>,
    /// Sprite names used by NPCs, monsters and equipment, sorted; entity
    /// `appearance` and equipment visuals are the position + 1 (0: none).
    sprites: Vec<String>,
    /// Icon names used by content, sorted; position + 1 (0: generic).
    icons: Vec<String>,
    hash: u64,
    warnings: Vec<ContentProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentProblem {
    /// `content/text/<locale>/` does not exist.
    MissingLocale(Locale),
    /// Something to look at that does not block (P04): an item with no
    /// way into the game, a text copied from another language.
    Notice {
        path: String,
        message: String,
    },
    /// `text/locales.ron`.
    Locales(String),
    /// An optional language with texts still missing: they show
    /// in English. A warning.
    Untranslated {
        locale: Locale,
        count: usize,
    },
    /// A language whose plural and number rules `i18n` does not know: it
    /// takes the English ones. A warning.
    UnknownRules(Locale),
    Text(TextProblem),
    Zone {
        zone: String,
        message: String,
    },
    /// `content/npcs/<id>.ron` is invalid or points at a missing key.
    Npc {
        npc: String,
        message: String,
    },
    /// `content/npcs/dialogues/<id>.ron` is invalid or points at a missing key.
    Dialogue {
        dialogue: String,
        message: String,
    },
    /// `content/moderation/reasons.ron` is invalid or points at a missing key.
    Moderation(String),
    /// `content/items/<id>.ron`.
    Item {
        item: String,
        message: String,
    },
    /// `content/quests/<id>.ron`.
    Quest {
        quest: String,
        message: String,
    },
    /// Phase 4 files: `path` relative to `content/`.
    File {
        path: String,
        message: String,
    },
}

impl ContentProblem {
    pub fn severity(&self) -> Severity {
        match self {
            ContentProblem::Untranslated { .. }
            | ContentProblem::UnknownRules(_)
            | ContentProblem::Notice { .. } => Severity::Warning,
            ContentProblem::MissingLocale(_)
            | ContentProblem::Locales(_)
            | ContentProblem::Zone { .. }
            | ContentProblem::Npc { .. }
            | ContentProblem::Dialogue { .. }
            | ContentProblem::Moderation(_)
            | ContentProblem::Item { .. }
            | ContentProblem::Quest { .. }
            | ContentProblem::File { .. } => Severity::Error,
            ContentProblem::Text(p) => p.severity(),
        }
    }
}

impl fmt::Display for ContentProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentProblem::MissingLocale(l) => write!(f, "error: [{l}] missing folder text/{l}/"),
            ContentProblem::Locales(m) => write!(f, "error: text/locales.ron: {m}"),
            ContentProblem::Untranslated { locale, count } => write!(
                f,
                "warning: [{locale}] {count} texts not translated yet: the English text is shown"
            ),
            ContentProblem::UnknownRules(l) => write!(
                f,
                "warning: [{l}] no plural and number rules for this language in i18n: the English ones are used"
            ),
            ContentProblem::Text(p) => write!(f, "{p}"),
            ContentProblem::Zone { zone, message } => write!(f, "error: zones/{zone}: {message}"),
            ContentProblem::Npc { npc, message } => write!(f, "error: npcs/{npc}.ron: {message}"),
            ContentProblem::Dialogue { dialogue, message } => {
                write!(f, "error: npcs/dialogues/{dialogue}.ron: {message}")
            }
            ContentProblem::Moderation(message) => {
                write!(f, "error: moderation/reasons.ron: {message}")
            }
            ContentProblem::Item { item, message } => write!(f, "error: items/{item}.ron: {message}"),
            ContentProblem::Quest { quest, message } => write!(f, "error: quests/{quest}.ron: {message}"),
            ContentProblem::File { path, message } => write!(f, "error: {path}: {message}"),
            ContentProblem::Notice { path, message } => write!(f, "warning: {path}: {message}"),
        }
    }
}

/// A text file with the English text of `keys`: what an
/// optional language shows until it is translated.
fn english_for(source: &[(&str, &str)], keys: &[String]) -> String {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Plain(String),
        Full { text: String },
    }
    let mut text: BTreeMap<String, String> = BTreeMap::new();
    for (_, file) in source {
        if let Ok(map) = ron::from_str::<BTreeMap<String, Entry>>(file) {
            for (k, e) in map {
                let (Entry::Plain(t) | Entry::Full { text: t }) = e;
                text.insert(k, t);
            }
        }
    }
    let mut out = String::from("{\n");
    for k in keys {
        let t = text.get(k).cloned().unwrap_or_default();
        out.push_str(&format!(
            "    {}: {},\n",
            ron::to_string(k).unwrap_or_default(),
            ron::to_string(&t).unwrap_or_else(|_| "\"\"".into())
        ));
    }
    out.push_str("}\n");
    out
}

impl Catalog {
    /// Development only (`--allow-missing-text`, `tools/editor`): every
    /// `en` key missing in another locale gets the English text, so new
    /// content can be played before it is translated. Returns what was
    /// filled; release never calls this.
    pub fn fill_missing_text(&mut self) -> Vec<(Locale, String)> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Entry {
            Plain(String),
            Full { text: String },
        }
        let Some((_, en)) = self.texts.iter().find(|(l, _)| *l == Locale::EN) else {
            return Vec::new();
        };
        let en_refs: Vec<(&str, &str)> = en.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
        let Ok(keys) = KeyTable::from_source(&en_refs) else {
            return Vec::new();
        };
        let mut source: BTreeMap<String, String> = BTreeMap::new();
        for (_, text) in en {
            if let Ok(map) = ron::from_str::<BTreeMap<String, Entry>>(text) {
                for (k, e) in map {
                    let t = match e {
                        Entry::Plain(t) | Entry::Full { text: t } => t,
                    };
                    source.insert(k, t);
                }
            }
        }
        let mut filled = Vec::new();
        let others: Vec<Locale> = self
            .texts
            .iter()
            .map(|(l, _)| *l)
            .filter(|l| *l != Locale::EN)
            .collect();
        for locale in others {
            let files: Vec<(String, String)> = self
                .texts
                .iter()
                .find(|(l, _)| *l == locale)
                .map(|(_, f)| f.clone())
                .unwrap_or_default();
            let refs: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
            let missing: Vec<String> = i18n::check_locale(&keys, locale, &refs)
                .into_iter()
                .filter(|p| p.kind == i18n::TextProblemKind::MissingKey)
                .filter_map(|p| p.key)
                .collect();
            if missing.is_empty() {
                continue;
            }
            let mut file = String::from("{\n");
            for k in &missing {
                let text = source.get(k).cloned().unwrap_or_default();
                let quoted = ron::to_string(&text).unwrap_or_else(|_| "\"\"".into());
                file.push_str(&format!(
                    "    {}: {quoted},\n",
                    ron::to_string(k).unwrap_or_default()
                ));
                filled.push((locale, k.clone()));
            }
            file.push_str("}\n");
            let entry = ("zz_missing_en.ron".to_owned(), file);
            match self.texts.iter_mut().find(|(l, _)| *l == locale) {
                Some((_, v)) => v.push(entry),
                None => self.texts.push((locale, vec![entry])),
            }
        }
        self.texts.sort_by_key(|(l, _)| *l);
        filled
    }

    /// Runs every check. On success, warnings stay in `ValidatedCatalog::warnings`.
    pub fn validate(self) -> Result<ValidatedCatalog, Vec<ContentProblem>> {
        let files_of = |locale: Locale| -> Option<Vec<(&str, &str)>> {
            self.texts
                .iter()
                .find(|(l, _)| *l == locale)
                .map(|(_, files)| files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect())
        };
        let folders: Vec<Locale> = self.texts.iter().map(|(l, _)| *l).collect();
        let (locales, mut problems) = match crate::locales::parse_locales(self.locales.as_deref(), &folders) {
            Ok(list) => (list, Vec::new()),
            Err(msgs) => (
                Vec::new(),
                msgs.into_iter().map(ContentProblem::Locales).collect(),
            ),
        };
        for d in &locales {
            if d.required && files_of(d.locale).is_none() {
                problems.push(ContentProblem::MissingLocale(d.locale));
            }
            if i18n::known_rules(d.locale).is_none() {
                problems.push(ContentProblem::UnknownRules(d.locale));
            }
        }

        let mut zones = Vec::with_capacity(self.zones.len());
        for (name, raw) in &self.zones {
            let zone_problem = |message: String| ContentProblem::Zone {
                zone: name.clone(),
                message,
            };
            match (&raw.zone, &raw.map) {
                (Some(z), Some(m)) => {
                    match parse_zone(
                        name,
                        z,
                        m,
                        raw.npcs.as_deref(),
                        raw.monsters.as_deref(),
                        raw.nodes.as_deref(),
                    ) {
                        Ok(def) => zones.push(def),
                        Err(msgs) => problems.extend(msgs.into_iter().map(zone_problem)),
                    }
                }
                (None, _) => problems.push(zone_problem("missing zone.ron".into())),
                (_, None) => problems.push(zone_problem("missing map.ron".into())),
            }
        }

        for (zone, message) in crate::zone::world_problems(&zones) {
            problems.push(ContentProblem::Zone { zone, message });
        }

        let source = files_of(Locale::SOURCE).unwrap_or_default();
        let keys = match KeyTable::from_source(&source) {
            Ok(k) => k,
            Err(i18n::I18nError::Text(p)) => {
                problems.extend(p.into_iter().map(ContentProblem::Text));
                return Err(problems);
            }
            Err(e) => {
                // `from_source` only returns `Text` errors; keep any other visible.
                problems.push(ContentProblem::Text(TextProblem {
                    locale: Locale::SOURCE,
                    file: String::new(),
                    key: None,
                    kind: i18n::TextProblemKind::BadFile(e.to_string()),
                }));
                return Err(problems);
            }
        };
        let mut moderation_reasons: Vec<String> = Vec::new();
        if let Some(text) = &self.moderation {
            match ron::from_str::<Vec<String>>(text) {
                Ok(list) => {
                    for reason in list {
                        if moderation_reasons.contains(&reason) {
                            problems.push(ContentProblem::Moderation(format!("duplicate reason `{reason}`")));
                        } else if keys.id(&reason).is_none() {
                            problems.push(ContentProblem::Moderation(format!(
                                "reason `{reason}` has no text key"
                            )));
                        } else {
                            moderation_reasons.push(reason);
                        }
                    }
                }
                Err(e) => problems.push(ContentProblem::Moderation(e.to_string())),
            }
        }

        let mut appearance = AppearanceDef::default();
        if let Some(text) = &self.appearance {
            match crate::appearance::parse_appearance(text, &keys) {
                Ok(a) => appearance = a,
                Err(msgs) => problems.extend(msgs.into_iter().map(|message| ContentProblem::File {
                    path: "appearance/appearance.ron".into(),
                    message,
                })),
            }
        }

        // Ids are positions in the sorted file lists, known before parsing,
        // so references between kinds resolve in any order.
        fn numbered<T>(map: &BTreeMap<String, String>, make: impl Fn(u32) -> T) -> BTreeMap<String, T> {
            map.keys()
                .enumerate()
                .filter_map(|(i, k)| Some((k.clone(), make(u32::try_from(i).ok()?))))
                .collect()
        }
        let item_ids = numbered(&self.items, ItemId::new);
        let npc_ids = numbered(&self.npcs, NpcId::new);
        let quest_ids = numbered(&self.quests, QuestId::new);
        let dialogue_ids = numbered(&self.dialogues, DialogueId::new);
        let monster_ids = numbered(&self.monsters, MonsterId::new);
        let class_ids = numbered(&self.classes, ClassId::new);
        let prof_ids = numbered(&self.proficiencies, ProficiencyId::new);
        let tech_ids = numbered(&self.techniques, TechniqueId::new);
        let table_ids = numbered(&self.loot_tables, LootTableId::new);
        let shop_ids = numbered(&self.shops, ShopId::new);
        let recipe_ids = numbered(&self.recipes, RecipeId::new);
        let node_ids = numbered(&self.nodes, NodeId::new);
        let names = Names {
            items: &item_ids,
            npcs: &npc_ids,
            quests: &quest_ids,
            recipes: &recipe_ids,
            techniques: &tech_ids,
            monsters: &monster_ids,
            goals: &self.goals,
            looks: &appearance,
        };
        let key =
            |problems: &mut Vec<ContentProblem>, k: String, problem: &dyn Fn(String) -> ContentProblem| {
                keys.id(&k).unwrap_or_else(|| {
                    problems.push(problem(format!("no text key `{k}`")));
                    TextId::new(0)
                })
            };

        let mut items: Vec<ItemDef> = Vec::with_capacity(self.items.len());
        for (id, text) in &self.items {
            let problem = |message| ContentProblem::Item {
                item: id.clone(),
                message,
            };
            match item::parse_item(id, text) {
                Ok(f) => {
                    let name = key(&mut problems, item::item_name_key(id), &problem);
                    let description = key(&mut problems, item::item_desc_key(id), &problem);
                    items.push(ItemDef {
                        id: id.clone(),
                        kind: f.kind,
                        max_stack: f.max_stack,
                        weight: f.weight,
                        binding: f.binding,
                        rarity: f.rarity,
                        use_effect: f.use_effect,
                        value: f.value,
                        durability: f.durability,
                        name,
                        description,
                        equip: match f.equip {
                            None => None,
                            Some(EquipFile::Armor { defense, visual }) => {
                                Some(Equip::Armor { defense, visual })
                            }
                            Some(EquipFile::Offhand { defense, visual }) => {
                                Some(Equip::Offhand { defense, visual })
                            }
                            Some(EquipFile::Weapon {
                                damage,
                                attack_ticks,
                                attribute,
                                proficiency,
                                range,
                                visual,
                                vigor,
                            }) => match prof_ids.get(&proficiency) {
                                Some(&proficiency) => Some(Equip::Weapon {
                                    damage,
                                    attack_ticks,
                                    attribute,
                                    proficiency,
                                    range,
                                    visual,
                                    vigor,
                                }),
                                None => {
                                    problems.push(problem(format!("proficiency `{proficiency}` not found")));
                                    None
                                }
                            },
                        },
                        hidden: f.hidden,
                        icon: f.icon,
                        light: f.light,
                        burn: f.burn,
                        unique: f.unique,
                        powers: f
                            .powers
                            .iter()
                            .filter_map(|p| match p {
                                item::PowerFile::Attribute(a, v) => Some(base::ItemPower::Attribute(*a, *v)),
                                item::PowerFile::Technique(t) => match tech_ids.get(t) {
                                    Some(&t) => Some(base::ItemPower::Technique(t)),
                                    None => {
                                        problems.push(problem(format!("powers: technique `{t}` not found")));
                                        None
                                    }
                                },
                            })
                            .collect(),
                    });
                }
                Err(message) => problems.push(problem(message)),
            }
        }

        let mut quests: Vec<QuestDef> = Vec::with_capacity(self.quests.len());
        for (id, text) in &self.quests {
            let problem = |message| ContentProblem::Quest {
                quest: id.clone(),
                message,
            };
            let name = key(&mut problems, quest::quest_name_key(id), &problem);
            let description = key(&mut problems, quest::quest_desc_key(id), &problem);
            match quest::parse_quest(id, text, &names, name, description) {
                Ok(q) => quests.push(q),
                Err(msgs) => problems.extend(msgs.into_iter().map(problem)),
            }
        }

        let mut dialogues: Vec<DialogueDef> = Vec::with_capacity(self.dialogues.len());
        for (id, text) in &self.dialogues {
            match npc::parse_dialogue(id, text, &keys, &names) {
                Ok(def) => dialogues.push(def),
                Err(msgs) => problems.extend(msgs.into_iter().map(|message| ContentProblem::Dialogue {
                    dialogue: id.clone(),
                    message,
                })),
            }
        }

        let mut npcs: Vec<NpcDef> = Vec::with_capacity(self.npcs.len());
        for (id, text) in &self.npcs {
            let file = match npc::parse_npc(id, text) {
                Ok(f) => f,
                Err(message) => {
                    problems.push(ContentProblem::Npc {
                        npc: id.clone(),
                        message,
                    });
                    continue;
                }
            };
            let name = match keys.id(&npc::name_key(id)) {
                Some(text) => text,
                None => {
                    problems.push(ContentProblem::Npc {
                        npc: id.clone(),
                        message: format!("no text key `{}`", npc::name_key(id)),
                    });
                    TextId::new(0)
                }
            };
            let dialogue = if file.dialogue.is_empty() {
                None
            } else {
                match dialogue_ids.get(&file.dialogue) {
                    Some(dialogue) => Some(*dialogue),
                    None => {
                        problems.push(ContentProblem::Npc {
                            npc: id.clone(),
                            message: format!("dialogue `{}` not found", file.dialogue),
                        });
                        None
                    }
                }
            };
            let shop = if file.shop.is_empty() {
                None
            } else {
                match shop_ids.get(&file.shop) {
                    Some(shop) => Some(*shop),
                    None => {
                        problems.push(ContentProblem::Npc {
                            npc: id.clone(),
                            message: format!("shop `{}` not found", file.shop),
                        });
                        None
                    }
                }
            };
            npcs.push(NpcDef {
                id: id.clone(),
                sprite: file.sprite,
                dialogue,
                shop,
                name,
            });
        }

        // Every spawn must name an NPC that exists.
        // A place to reach is walkable ground of some zone.
        for q in &quests {
            for o in &q.objectives {
                if let crate::Objective::Reach { pos, .. } = o {
                    let ok = crate::zone::zone_at(&zones, *pos)
                        .and_then(|z| z.tile_at(*pos))
                        .is_some_and(|t| !t.blocks_movement);
                    if !ok {
                        problems.push(ContentProblem::File {
                            path: format!("quests/{}.ron", q.id),
                            message: format!("Reach ({}, {}): not walkable ground of any zone", pos.x, pos.y),
                        });
                    }
                }
            }
        }
        // A teleport lands on walkable ground of some zone.
        for d in &dialogues {
            let effects = d.nodes.iter().flat_map(|n| &n.options).flat_map(|o| &o.effects);
            for e in effects {
                if let crate::Effect::Teleport(to) = e {
                    let ok = crate::zone::zone_at(&zones, *to)
                        .and_then(|z| z.tile_at(*to))
                        .is_some_and(|t| !t.blocks_movement);
                    if !ok {
                        problems.push(ContentProblem::File {
                            path: format!("npcs/dialogues/{}.ron", d.id),
                            message: format!(
                                "Teleport to ({}, {}): not walkable ground of any zone",
                                to.x, to.y
                            ),
                        });
                    }
                }
            }
        }
        for zone in &zones {
            for (i, spawn) in zone.npcs.iter().enumerate() {
                if !npc_ids.contains_key(&spawn.npc) {
                    problems.push(ContentProblem::Zone {
                        zone: zone.name.clone(),
                        message: format!("npcs.ron: entry {i}: npc `{}` not found", spawn.npc),
                    });
                }
            }
        }

        let phase4 = self.validate_phase4(&keys, &item_ids, &table_ids, &class_ids, &prof_ids, &mut problems);
        // A weapon's technique power needs that weapon in hand to be used.
        for i in &items {
            let Some(Equip::Weapon { proficiency, .. }) = &i.equip else {
                continue;
            };
            for p in &i.powers {
                if let base::ItemPower::Technique(t) = p
                    && let Some(d) = phase4.techniques.get(t.index())
                    && d.proficiency != *proficiency
                {
                    problems.push(ContentProblem::Item {
                        item: i.id.clone(),
                        message: format!("powers: technique `{}` needs another weapon's proficiency", d.id),
                    });
                }
            }
        }
        let loot_tables = {
            let p = &mut problems;
            crate::economy::validate_tables(&self.loot_tables, &item_ids, |path, message| {
                p.push(ContentProblem::File { path, message });
            })
        };
        let shops = {
            let p = &mut problems;
            crate::economy::validate_shops(&self.shops, &item_ids, &items, |path, message| {
                p.push(ContentProblem::File { path, message });
            })
        };
        // A relic is found or made, never bought.
        for shop in &shops {
            for st in &shop.stock {
                if items.get(st.item.index()).is_some_and(|d| d.unique) {
                    problems.push(ContentProblem::File {
                        path: format!("shops/{}.ron", shop.id),
                        message: "a shop cannot sell a unique item".into(),
                    });
                }
            }
        }
        let recipes = {
            let p = &mut problems;
            crate::craft::validate_recipes(&self.recipes, &keys, &item_ids, &prof_ids, |path, message| {
                p.push(ContentProblem::File { path, message });
            })
        };
        let nodes = {
            let p = &mut problems;
            crate::craft::validate_nodes(&self.nodes, &keys, &prof_ids, &table_ids, |path, message| {
                p.push(ContentProblem::File { path, message });
            })
        };
        let emotes = {
            let p = &mut problems;
            crate::emote::validate_emotes(&self.emotes, &keys, |path, message| {
                p.push(ContentProblem::File { path, message });
            })
        };
        let dungeons = {
            let mut out: Vec<crate::DungeonDef> = Vec::new();
            for (id, text) in &self.dungeons {
                let path = format!("dungeons/{id}.ron");
                let mut err = |m: String| {
                    problems.push(ContentProblem::File {
                        path: path.clone(),
                        message: m,
                    })
                };
                let f = match crate::dungeon::parse_dungeon(id, text) {
                    Ok(f) => f,
                    Err(m) => {
                        err(m);
                        continue;
                    }
                };
                let Some(zi) = zones.iter().position(|z| z.name == f.zone) else {
                    err(format!("zone `{}` not found", f.zone));
                    continue;
                };
                let zone_def = zones.get(zi);
                if zone_def.is_some_and(|z| z.instance.is_none()) {
                    err(format!("zone `{}` is not an instance", f.zone));
                }
                if out
                    .iter()
                    .any(|d| zones.get(d.zone.index()).is_some_and(|z| z.name == f.zone))
                {
                    err(format!("zone `{}` already has a dungeon", f.zone));
                }
                let boss = if f.boss.is_empty() {
                    None
                } else {
                    match phase4.monsters.iter().position(|m| m.id == f.boss) {
                        Some(i) => {
                            if zone_def.is_some_and(|z| !z.monsters.iter().any(|s| s.monster == f.boss)) {
                                err(format!("boss `{}` does not spawn in zone `{}`", f.boss, f.zone));
                            }
                            u32::try_from(i).ok().map(base::MonsterId::new)
                        }
                        None => {
                            err(format!("monster `{}` not found", f.boss));
                            None
                        }
                    }
                };
                let Some(name) = keys.id(&crate::dungeon::dungeon_name_key(id)) else {
                    err(format!("text `{}` missing", crate::dungeon::dungeon_name_key(id)));
                    continue;
                };
                out.push(crate::DungeonDef {
                    id: id.clone(),
                    name,
                    zone: base::ZoneId::new(u32::try_from(zi).unwrap_or(u32::MAX)),
                    party: f.party,
                    level: f.level,
                    boss,
                    lockout: f.lockout,
                });
            }
            out
        };
        let rankings = {
            let p = &mut problems;
            let (monsters, quests, dungeon_files) = (&self.monsters, &self.quests, &self.dungeons);
            crate::rank::validate_rankings(
                &self.rankings,
                &keys,
                &phase4.counters,
                &prof_ids,
                |family, key| match progress::counter_key_kind(family) {
                    Some("monster") => monsters.contains_key(key),
                    Some("quest") => quests.contains_key(key),
                    Some("dungeon") => dungeon_files.contains_key(key),
                    _ => false,
                },
                |path, message| p.push(ContentProblem::File { path, message }),
            )
        };
        let goals = {
            let p = &mut problems;
            let (monsters, quests, dungeon_files) = (&self.monsters, &self.quests, &self.dungeons);
            crate::goal::validate_goals(
                &self.goals,
                &keys,
                &phase4.counters,
                |family, key| match progress::counter_key_kind(family) {
                    Some("monster") => monsters.contains_key(key),
                    Some("quest") => quests.contains_key(key),
                    Some("dungeon") => dungeon_files.contains_key(key),
                    _ => false,
                },
                |path, message| p.push(ContentProblem::File { path, message }),
            )
        };
        for zone in &zones {
            for (i, spawn) in zone.nodes.iter().enumerate() {
                if !node_ids.contains_key(&spawn.node) {
                    problems.push(ContentProblem::Zone {
                        zone: zone.name.clone(),
                        message: format!("nodes.ron: entry {i}: node `{}` not found", spawn.node),
                    });
                }
            }
        }
        for zone in &zones {
            for (i, spawn) in zone.monsters.iter().enumerate() {
                if !monster_ids.contains_key(&spawn.monster) {
                    problems.push(ContentProblem::Zone {
                        zone: zone.name.clone(),
                        message: format!("monsters.ron: entry {i}: monster `{}` not found", spawn.monster),
                    });
                }
            }
        }

        let mut bundles = Vec::with_capacity(locales.len());
        for d in &locales {
            let locale = d.locale;
            // A required language without its folder was reported above.
            if d.required && files_of(locale).is_none() {
                continue;
            }
            let mut files = files_of(locale).unwrap_or_default();
            let found = i18n::check_locale(&keys, locale, &files);
            let (missing, other): (Vec<_>, Vec<_>) = found
                .into_iter()
                .partition(|p| p.kind == i18n::TextProblemKind::MissingKey);
            problems.extend(other.into_iter().map(ContentProblem::Text));
            // An optional language shows the English text of what it lacks.
            let filler = if d.required {
                problems.extend(missing.into_iter().map(ContentProblem::Text));
                None
            } else if missing.is_empty() {
                None
            } else {
                problems.push(ContentProblem::Untranslated {
                    locale,
                    count: missing.len(),
                });
                let lacking: Vec<String> = missing.into_iter().filter_map(|p| p.key).collect();
                Some(english_for(&source, &lacking))
            };
            if let Some(f) = &filler {
                files.push(("zz_english.ron", f.as_str()));
            }
            if let Ok(b) = Bundle::from_text(locale, &keys, &files) {
                bundles.push(b);
            }
        }
        // Graphics by name.
        let mut sprites = std::collections::BTreeSet::new();
        let mut icons = std::collections::BTreeSet::new();
        let visuals = items.iter().filter_map(|i| match &i.equip {
            Some(
                Equip::Weapon { visual, .. } | Equip::Armor { visual, .. } | Equip::Offhand { visual, .. },
            ) => Some(visual),
            None => None,
        });
        let used_sprites = npcs
            .iter()
            .map(|n| (format!("npcs/{}.ron", n.id), &n.sprite))
            .chain(
                phase4
                    .monsters
                    .iter()
                    .map(|m| (format!("monsters/{}.ron", m.id), &m.sprite)),
            )
            .chain(visuals.map(|v| ("items/".to_owned(), v)))
            .chain(nodes.iter().map(|n| (format!("nodes/{}.ron", n.id), &n.sprite)))
            .chain(
                appearance
                    .lists
                    .iter()
                    .flatten()
                    .flat_map(|o| [&o.sprite, &o.clothes])
                    .map(|s| ("appearance/appearance.ron".to_owned(), s)),
            );
        for (path, name) in used_sprites {
            if name.is_empty() {
                continue;
            }
            if npc::valid_id(name) {
                sprites.insert(name.clone());
            } else {
                problems.push(ContentProblem::File {
                    path,
                    message: format!("sprite `{name}` must be [a-z0-9_]"),
                });
            }
        }
        let used_icons = items
            .iter()
            .map(|i| (format!("items/{}.ron", i.id), &i.icon))
            .chain(
                phase4
                    .monsters
                    .iter()
                    .map(|m| (format!("monsters/{}.ron", m.id), &m.icon)),
            )
            .chain(
                phase4
                    .classes
                    .iter()
                    .map(|c| (format!("classes/{}.ron", c.id), &c.icon)),
            )
            .chain(
                phase4
                    .techniques
                    .iter()
                    .map(|t| (format!("techniques/{}.ron", t.id), &t.icon)),
            )
            .chain(
                phase4
                    .titles
                    .iter()
                    .map(|t| (format!("titles/{}.ron", t.id), &t.icon)),
            )
            .chain(
                phase4
                    .special_attributes
                    .iter()
                    .map(|t| (format!("special_attributes/{}.ron", t.id), &t.icon)),
            )
            .chain(emotes.iter().map(|e| (format!("emotes/{}.ron", e.id), &e.icon)));
        for (path, name) in used_icons {
            if name.is_empty() {
                continue;
            }
            if npc::valid_id(name) {
                icons.insert(name.clone());
            } else {
                problems.push(ContentProblem::File {
                    path,
                    message: format!("icon `{name}` must be [a-z0-9_]"),
                });
            }
        }
        if problems.iter().any(|p| p.severity() == Severity::Error) {
            return Err(problems);
        }
        let mut hash = Fnv64::new();
        for (path, text) in &self.files {
            hash.write_str(path);
            hash.write_str(text);
        }
        let mut valid = ValidatedCatalog {
            keys,
            bundles,
            locales,
            zones,
            npcs,
            dialogues,
            items,
            quests,
            moderation_reasons,
            appearance,
            formulas: phase4.formulas,
            monsters: phase4.monsters,
            classes: phase4.classes,
            proficiencies: phase4.proficiencies,
            techniques: phase4.techniques,
            titles: phase4.titles,
            counters: phase4.counters,
            special_attributes: phase4.special_attributes,
            loot_tables,
            shops,
            recipes,
            nodes,
            emotes,
            rankings,
            dungeons,
            goals,
            sprites: sprites.into_iter().collect(),
            icons: icons.into_iter().collect(),
            hash: hash.finish(),
            warnings: problems,
        };
        let notices = valid.notices();
        valid.warnings.extend(notices);
        Ok(valid)
    }
}

impl ValidatedCatalog {
    /// Sprite names in use, sorted (`appearance` = position + 1).
    pub fn sprites(&self) -> &[String] {
        &self.sprites
    }

    /// Icon names in use, sorted (icon number = position + 1).
    pub fn icons(&self) -> &[String] {
        &self.icons
    }

    /// Number sent for a sprite name: position + 1; 0 for none.
    pub fn sprite_index(&self, name: &str) -> u16 {
        index_of(&self.sprites, name)
    }

    /// Number sent for an icon name: position + 1; 0 for the generic one.
    pub fn icon_index(&self, name: &str) -> u16 {
        index_of(&self.icons, name)
    }

    pub fn keys(&self) -> &KeyTable {
        &self.keys
    }

    /// The languages, in list order.
    pub fn locales(&self) -> &[LocaleDef] {
        &self.locales
    }

    pub fn bundle(&self, locale: Locale) -> Option<&Bundle> {
        self.bundles.iter().find(|b| b.locale() == locale)
    }

    pub fn zone(&self, id: ZoneId) -> Option<&ZoneDef> {
        self.zones.get(id.index())
    }

    pub fn zone_by_name(&self, name: &str) -> Option<(ZoneId, &ZoneDef)> {
        let i = self.zones.iter().position(|z| z.name == name)?;
        Some((ZoneId::new(u32::try_from(i).ok()?), self.zones.get(i)?))
    }

    pub fn zones(&self) -> impl Iterator<Item = (ZoneId, &ZoneDef)> {
        self.zones
            .iter()
            .enumerate()
            .map(|(i, z)| (ZoneId::new(i as u32), z))
    }

    pub fn npc(&self, id: NpcId) -> Option<&NpcDef> {
        self.npcs.get(id.index())
    }

    pub fn npc_by_name(&self, id: &str) -> Option<(NpcId, &NpcDef)> {
        let i = self.npcs.iter().position(|n| n.id == id)?;
        Some((NpcId::new(u32::try_from(i).ok()?), self.npcs.get(i)?))
    }

    pub fn npcs(&self) -> impl Iterator<Item = (NpcId, &NpcDef)> {
        self.npcs
            .iter()
            .enumerate()
            .map(|(i, n)| (NpcId::new(i as u32), n))
    }

    pub fn dialogue(&self, id: DialogueId) -> Option<&DialogueDef> {
        self.dialogues.get(id.index())
    }

    pub fn dialogues(&self) -> impl Iterator<Item = (DialogueId, &DialogueDef)> {
        self.dialogues
            .iter()
            .enumerate()
            .map(|(i, d)| (DialogueId::new(i as u32), d))
    }

    pub fn item(&self, id: ItemId) -> Option<&ItemDef> {
        self.items.get(id.index())
    }

    pub fn item_by_name(&self, id: &str) -> Option<(ItemId, &ItemDef)> {
        let i = self.items.binary_search_by(|d| d.id.as_str().cmp(id)).ok()?;
        Some((ItemId::new(u32::try_from(i).ok()?), self.items.get(i)?))
    }

    /// In `ItemId` order: the `Definitions` list.
    pub fn items(&self) -> &[ItemDef] {
        &self.items
    }

    pub fn quest(&self, id: QuestId) -> Option<&QuestDef> {
        self.quests.get(id.index())
    }

    pub fn quest_by_name(&self, id: &str) -> Option<(QuestId, &QuestDef)> {
        let i = self.quests.binary_search_by(|d| d.id.as_str().cmp(id)).ok()?;
        Some((QuestId::new(u32::try_from(i).ok()?), self.quests.get(i)?))
    }

    pub fn quests(&self) -> &[QuestDef] {
        &self.quests
    }

    /// Character creation lists (phase 5).
    pub fn appearance(&self) -> &AppearanceDef {
        &self.appearance
    }

    /// Text keys allowed as a moderation reason (`tools/admin`).
    pub fn moderation_reasons(&self) -> &[String] {
        &self.moderation_reasons
    }

    /// Hash of every content file: identifies this content version (text
    /// packs, replay).
    pub fn content_hash(&self) -> u64 {
        self.hash
    }

    pub fn warnings(&self) -> &[ContentProblem] {
        &self.warnings
    }

    /// Content without an icon (`icon: ""`, the generic one) as
    /// `(folder, id)`: items, monsters, classes, techniques, titles, emotes.
    pub fn without_icon(&self) -> Vec<(&'static str, &str)> {
        fn pick<'a>(folder: &'static str, id: &'a str, icon: &str) -> Option<(&'static str, &'a str)> {
            icon.is_empty().then_some((folder, id))
        }
        self.items
            .iter()
            .filter_map(|d| pick("items", &d.id, &d.icon))
            .chain(
                self.monsters
                    .iter()
                    .filter_map(|d| pick("monsters", &d.id, &d.icon)),
            )
            .chain(
                self.classes
                    .iter()
                    .filter_map(|d| pick("classes", &d.id, &d.icon)),
            )
            .chain(
                self.techniques
                    .iter()
                    .filter_map(|d| pick("techniques", &d.id, &d.icon)),
            )
            .chain(self.titles.iter().filter_map(|d| pick("titles", &d.id, &d.icon)))
            .chain(self.emotes.iter().filter_map(|d| pick("emotes", &d.id, &d.icon)))
            .collect()
    }

    /// P04: what the review used to catch by eye.
    fn notices(&self) -> Vec<ContentProblem> {
        let mut out: Vec<ContentProblem> = self
            .unsourced_items()
            .into_iter()
            .map(|id| ContentProblem::Notice {
                path: format!("items/{id}.ron"),
                message: "no way into the game (no drop, shop, node, quest, dialogue or recipe with sourced materials)"
                    .to_owned(),
            })
            .collect();
        out.extend(self.copied_texts());
        out
    }

    /// Items a player can never get: no monster drop, loot table rolled by
    /// a monster or node, shop, quest reward, dialogue gift, or recipe whose
    /// materials all have a source.
    pub fn unsourced_items(&self) -> Vec<&str> {
        let mut has = vec![false; self.items.len()];
        let mark = |id: ItemId, has: &mut Vec<bool>| {
            if let Some(h) = has.get_mut(id.index()) {
                *h = true;
            }
        };
        let mut rolled: Vec<LootTableId> = self.nodes.iter().map(|n| n.loot).collect();
        for m in &self.monsters {
            m.loot.iter().for_each(|l| mark(l.item, &mut has));
            rolled.extend(m.loot_table);
        }
        for t in rolled.iter().filter_map(|&t| self.loot_table(t)) {
            t.entries.iter().for_each(|l| mark(l.item, &mut has));
        }
        for s in &self.shops {
            s.stock.iter().for_each(|s| mark(s.item, &mut has));
        }
        for q in &self.quests {
            for r in &q.rewards {
                if let crate::Reward::Item { item, .. } = r {
                    mark(*item, &mut has);
                }
            }
        }
        for d in &self.dialogues {
            for e in d.nodes.iter().flat_map(|n| &n.options).flat_map(|o| &o.effects) {
                if let crate::Effect::GiveItem { item, .. } | crate::Effect::TopUpItem { item, .. } = e {
                    mark(*item, &mut has);
                }
            }
        }
        // Recipes until nothing changes: a result counts once its
        // materials do.
        loop {
            let mut changed = false;
            for r in &self.recipes {
                let ready = r.materials.iter().all(|m| has.get(m.item.index()) == Some(&true));
                if ready && has.get(r.result.index()) == Some(&false) {
                    mark(r.result, &mut has);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        self.items
            .iter()
            .zip(&has)
            .filter(|(_, h)| !**h)
            .map(|(i, _)| i.id.as_str())
            .collect()
    }

    /// A long sentence that was copied, not translated (the Portuguese
    /// that sat in `ja`, lacunas.md): the same as the English one, or all
    /// in Latin letters in a language whose texts mostly are not.
    fn copied_texts(&self) -> Vec<ContentProblem> {
        const MIN_CHARS: usize = 16;
        let latin = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| c.is_ascii() || ('\u{c0}'..='\u{24f}').contains(&c))
        };
        let long = |s: &str| s.chars().count() >= MIN_CHARS && s.contains(' ');
        let en = self.bundles.iter().find(|b| b.locale() == Locale::EN);
        let mut out = Vec::new();
        for b in self.bundles.iter().filter(|b| b.locale() != Locale::EN) {
            let texts: Vec<(TextId, &str, String)> = self
                .keys
                .iter()
                .filter_map(|(id, key)| Some((id, key, b.get(id)?.literal())))
                .collect();
            let latin_share = texts.iter().filter(|(_, _, t)| latin(t)).count() * 2;
            let non_latin = latin_share < texts.len();
            for (id, key, lit) in &texts {
                if !long(lit) {
                    continue;
                }
                let same = en.and_then(|e| e.get(*id)) == b.get(*id);
                let why = if same {
                    "the same as in en"
                } else if non_latin && latin(lit) {
                    "all in Latin letters"
                } else {
                    continue;
                };
                out.push(ContentProblem::Notice {
                    path: format!("text/{}", b.locale().code()),
                    message: format!("`{key}` is {why}: copied, not translated?"),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::from_text;
    use i18n::Locale;

    const TEN: [&str; 10] = ["en", "pt-BR", "zh", "it", "id", "de", "ru", "es", "ja", "ko"];

    fn all_locales(text: &str) -> Vec<(String, String)> {
        TEN.iter()
            .map(|l| (format!("text/{l}/a.ron"), text.to_owned()))
            .collect()
    }

    fn refs(v: &[(String, String)]) -> Vec<(&str, &str)> {
        v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
    }

    /// P04: an item nobody can get and a sentence copied from English
    /// warn without blocking.
    #[test]
    fn unsourced_items_and_copied_texts_warn() {
        let mut files = Vec::new();
        for l in TEN {
            let copied = if l == "de" {
                "A long sentence in English"
            } else {
                l
            };
            files.push((
                format!("text/{l}/a.ron"),
                format!(
                    r#"{{ "item.orphan.name": "{copied}", "item.orphan.desc": "x", "item.bread.name": "b", "item.bread.desc": "x", "item.cake.name": "c", "item.cake.desc": "x", "recipe.cake.name": "r", "recipe.cake.desc": "x", "proficiency.cooking.name": "p", "proficiency.unarmed.name": "u" }}"#
                ),
            ));
        }
        if let Some(en) = files.iter_mut().find(|(p, _)| p == "text/en/a.ron") {
            en.1 = en.1.replace(
                r#""item.orphan.name": "en""#,
                r#""item.orphan.name": "A long sentence in English""#,
            );
        }
        for (p, t) in [
            (
                "zones/start/zone.ron",
                "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 0, y: 0))",
            ),
            (
                "zones/start/map.ron",
                r#"(legend: { '.': (floor: 1) }, rows: [".."])"#,
            ),
            ("items/orphan.ron", "(kind: Material, max_stack: 5)"),
            ("items/bread.ron", "(kind: Material, max_stack: 5, value: 5)"),
            ("items/cake.ron", "(kind: Material, max_stack: 5)"),
            ("shops/s.ron", r#"(stock: [(item: "bread", qty: 0)])"#),
            ("proficiencies/cooking.ron", "(kind: Production)"),
            ("proficiencies/unarmed.ron", "(kind: Weapon)"),
            (
                "recipes/cake.ron",
                r#"(proficiency: "cooking", materials: [(item: "bread", qty: 1)], result: "cake", xp: 1)"#,
            ),
        ] {
            files.push((p.into(), t.into()));
        }
        let c = from_text(&refs(&files)).unwrap().validate().unwrap();
        assert_eq!(
            c.unsourced_items(),
            vec!["orphan"],
            "the cake comes from bought bread"
        );
        let w: Vec<String> = c.warnings().iter().map(|w| w.to_string()).collect();
        assert!(w.iter().any(|w| w.contains("items/orphan.ron")), "{w:?}");
        assert!(
            w.iter()
                .any(|w| w.contains("text/de") && w.contains("item.orphan.name") && w.contains("en")),
            "{w:?}"
        );
    }

    #[test]
    fn valid_content() {
        let mut files =
            all_locales(r#"{ "system.ok": "{count, plural, one {a} few {b} many {c} other {d}}" }"#);
        files.push((
            "zones/start/zone.ron".into(),
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 0, y: 0))".into(),
        ));
        files.push((
            "zones/start/map.ron".into(),
            r#"(legend: { '.': (floor: 1) }, rows: [".."])"#.into(),
        ));
        let c = from_text(&refs(&files)).unwrap().validate().unwrap();
        assert_eq!(c.keys().len(), 1);
        assert!(c.bundle(Locale::from_static("ko")).is_some());
        assert!(c.warnings().is_empty());
        let (id, z) = c.zone_by_name("start").unwrap();
        assert_eq!((id.get(), z.width), (0, 2));
    }

    #[test]
    fn an_optional_language_shows_english_and_only_warns() {
        let mut files = vec![
            (
                "text/en/a.ron".to_owned(),
                r#"{ "system.ok": "Fine", "system.two": "Two" }"#.to_owned(),
            ),
            (
                "text/fr/a.ron".to_owned(),
                r#"{ "system.ok": "Bien" }"#.to_owned(),
            ),
            (
                "text/locales.ron".to_owned(),
                r#"[(code: "en"), (code: "fr"), (code: "xx-test")]"#.to_owned(),
            ),
        ];
        files.push((
            "zones/start/zone.ron".into(),
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 0, y: 0))".into(),
        ));
        files.push((
            "zones/start/map.ron".into(),
            r#"(legend: { '.': (floor: 1) }, rows: [".."])"#.into(),
        ));
        let c = from_text(&refs(&files)).unwrap().validate().unwrap();
        let fr = c.bundle(Locale::from_static("fr")).unwrap();
        let two = c.keys().id("system.two").unwrap();
        assert_eq!(i18n::format(fr, &base::TextRef::new(two)), "Two");
        // `xx-test` has no folder at all: every text in English.
        assert!(c.bundle(Locale::from_static("xx-test")).is_some());
        let w: Vec<String> = c.warnings().iter().map(|w| w.to_string()).collect();
        assert!(w.iter().any(|w| w.contains("[fr] 1 texts")), "{w:?}");
        assert!(
            w.iter().any(|w| w.contains("[xx-test]") && w.contains("rules")),
            "{w:?}"
        );
        // Required, the same gap is an error.
        files[2].1 = r#"[(code: "en"), (code: "fr", required: true)]"#.to_owned();
        assert!(from_text(&refs(&files)).unwrap().validate().is_err());
        // A folder that is not listed is an error.
        files[2].1 = r#"[(code: "en")]"#.to_owned();
        assert!(from_text(&refs(&files)).unwrap().validate().is_err());
    }

    #[test]
    fn missing_locale_key_and_zone_file() {
        let mut files = all_locales(r#"{ "system.ok": "x", "system.two": "y" }"#);
        files.retain(|(p, _)| !p.starts_with("text/ja/"));
        // `ja` is listed and required, but its folder is gone.
        files.push((
            "text/locales.ron".into(),
            format!(
                "[{}]",
                TEN.map(|c| format!("(code: {c:?}, required: true)")).join(", ")
            ),
        ));
        if let Some(de) = files.iter_mut().find(|(p, _)| p.starts_with("text/de/")) {
            de.1 = r#"{ "system.ok": "x" }"#.into();
        }
        files.push(("zones/start/zone.ron".into(), "()".into()));
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(text.len(), 3, "{text:?}");
        assert!(text[0].contains("[ja] missing folder"));
        assert!(text[1].contains("zones/start: missing map.ron"));
        assert!(text[2].contains("[de]") && text[2].contains("system.two"));
    }

    #[test]
    fn bad_paths() {
        assert!(from_text(&[("text/en/a.txt", "")]).is_err());
        assert!(from_text(&[("text/x_y/a.ron", "")]).is_err());
        assert!(from_text(&[("items/x/a.ron", "")]).is_err());
        assert!(from_text(&[("scripts/a.ron", "")]).is_err());
        assert!(from_text(&[("zones/a/other.ron", "")]).is_err());
    }

    fn npc_content() -> Vec<(String, String)> {
        let text = r#"{
            "npc.elder.name": "Elder",
            "npc.elder.greeting": "Welcome.",
            "npc.elder.about": "I watch the road.",
            "npc.elder.option.about": "Who are you?",
            "npc.elder.option.bye": "Farewell.",
        }"#;
        let mut files = all_locales(text);
        files.push((
            "zones/start/zone.ron".into(),
            "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 0, y: 0))".into(),
        ));
        files.push((
            "zones/start/map.ron".into(),
            r#"(legend: { '.': (floor: 1) }, rows: ["...", "..."])"#.into(),
        ));
        files.push((
            "zones/start/npcs.ron".into(),
            r#"[(npc: "elder", pos: (x: 1, y: 1))]"#.into(),
        ));
        files.push((
            "npcs/elder.ron".into(),
            r#"(sprite: "elder", dialogue: "elder")"#.into(),
        ));
        files.push((
            "npcs/dialogues/elder.ron".into(),
            r#"(
                start: "greeting",
                nodes: {
                    "greeting": (
                        text: "npc.elder.greeting",
                        options: [
                            (text: "npc.elder.option.about", next: "about"),
                            (text: "npc.elder.option.bye"),
                        ],
                    ),
                    "about": (text: "npc.elder.about", options: []),
                },
            )"#
            .into(),
        ));
        files
    }

    #[test]
    fn npcs_and_dialogues() {
        let files = npc_content();
        let c = from_text(&refs(&files)).unwrap().validate().unwrap();
        let (id, npc) = c.npc_by_name("elder").unwrap();
        assert_eq!(npc.sprite, "elder");
        let dialogue = npc.dialogue.expect("dialogue");
        let tree = c.dialogue(dialogue).unwrap();
        assert_eq!(tree.start, 1);
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[1].options.len(), 2);
        assert_eq!(tree.nodes[1].options[0].next, Some(0));
        assert_eq!(tree.nodes[1].options[1].next, None);
        assert_eq!(c.zone_by_name("start").unwrap().1.npcs.len(), 1);
        assert_eq!(c.npc(id).unwrap().id, "elder");
    }

    #[test]
    fn npc_problems() {
        let mut files = npc_content();
        if let Some(text) = files.iter_mut().find(|(p, _)| p == "text/en/a.ron") {
            text.1 = r#"{
                "npc.elder.name": "Elder",
                "npc.elder.greeting": "Welcome.",
                "npc.elder.option.about": "Who are you?",
                "npc.elder.option.bye": "Farewell.",
            }"#
            .into();
        }
        if let Some(npc) = files.iter_mut().find(|(p, _)| p == "npcs/elder.ron") {
            npc.1 = r#"(sprite: "elder", dialogue: "missing")"#.into();
        }
        if let Some(spawns) = files.iter_mut().find(|(p, _)| p == "zones/start/npcs.ron") {
            spawns.1 = r#"[(npc: "ghost", pos: (x: 1, y: 1))]"#.into();
        }
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert!(
            text.iter()
                .any(|p| p.contains("npcs/elder.ron") && p.contains("missing")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|p| p.contains("npc `ghost` not found")),
            "{text:?}"
        );
        assert!(text.iter().any(|p| p.contains("npc.elder.about")), "{text:?}");
    }

    #[test]
    fn two_npcs_on_one_tile_fail() {
        let mut files = npc_content();
        if let Some(spawns) = files.iter_mut().find(|(p, _)| p == "zones/start/npcs.ron") {
            spawns.1 = r#"[(npc: "elder", pos: (x: 1, y: 1)), (npc: "elder", pos: (x: 1, y: 1))]"#.into();
        }
        let failed = match from_text(&refs(&files)) {
            Err(e) => e.to_string(),
            Ok(c) => match c.validate() {
                Err(p) => p.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "),
                Ok(_) => String::new(),
            },
        };
        assert!(failed.contains("taken by another NPC"), "{failed}");
    }

    fn quest_content() -> Vec<(String, String)> {
        let mut files = npc_content();
        for (path, text) in files.iter_mut() {
            if path.starts_with("text/") {
                *text = text.replace(
                    "\"npc.elder.name\": \"Elder\",",
                    "\"npc.elder.name\": \"Elder\", \"item.herb.name\": \"Herb\", \"item.herb.desc\": \"A herb.\", \"quest.herbs.name\": \"Herbs\", \"quest.herbs.desc\": \"Bring herbs.\",",
                );
            }
        }
        files.push((
            "items/herb.ron".into(),
            "(kind: Material, max_stack: 10, weight: 5)".into(),
        ));
        files.push((
            "quests/herbs.ron".into(),
            r#"(objectives: [Talk(npc: "elder"), Item(item: "herb", qty: 2)], rewards: [Coins(5)])"#.into(),
        ));
        if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
            d.1 = d.1.replace(
                r#"(text: "npc.elder.option.bye"),"#,
                r#"(text: "npc.elder.option.bye",
                    when: All([Quest(quest: "herbs", state: NotStarted), Not(Item(item: "herb", qty: 1))]),
                    effects: [StartQuest("herbs"), GiveItem(item: "herb", qty: 2), TopUpItem(item: "herb", qty: 3)]),"#,
            );
        }
        files
    }

    #[test]
    fn items_quests_and_rules() {
        use crate::{Condition, Effect, Objective};
        let files = quest_content();
        let c = from_text(&refs(&files)).unwrap().validate().unwrap();
        let (herb, def) = c.item_by_name("herb").unwrap();
        assert_eq!((def.max_stack, def.weight), (10, 5));
        let (_, q) = c.quest_by_name("herbs").unwrap();
        assert_eq!(q.objectives[1], Objective::Item { item: herb, qty: 2 });
        let tree = c
            .dialogue(c.npc_by_name("elder").unwrap().1.dialogue.unwrap())
            .unwrap();
        let bye = &tree.nodes[1].options[1];
        assert!(matches!(bye.when, Some(Condition::All(ref v)) if v.len() == 2));
        assert_eq!(bye.effects.len(), 3);
        assert_eq!(bye.effects[2], Effect::TopUpItem { item: herb, qty: 3 });
    }

    #[test]
    fn rules_point_at_missing_content() {
        let mut files = quest_content();
        if let Some(d) = files.iter_mut().find(|(p, _)| p == "npcs/dialogues/elder.ron") {
            d.1 =
                d.1.replace("TopUpItem(item: \"herb\"", "TopUpItem(item: \"nope\"")
                    .replace("GiveItem(item: \"herb\"", "GiveItem(item: \"rock\"");
        }
        if let Some(q) = files.iter_mut().find(|(p, _)| p == "quests/herbs.ron") {
            q.1 = q.1.replace("Talk(npc: \"elder\")", "Talk(npc: \"ghost\")");
        }
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text = problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("item `nope` not found"), "{text}");
        assert!(text.contains("item `rock` not found"), "{text}");
        assert!(text.contains("quests/herbs.ron: npc `ghost` not found"), "{text}");
    }

    #[test]
    fn a_title_counter_key_must_name_something() {
        let mut files = quest_content();
        files.push(("counters.ron".into(), r#"["kills", "deaths"]"#.into()));
        files.push((
            "titles/ghost_hunter.ron".into(),
            r#"(trigger: Counter(family: "kills", key: "ghost", at: 3))"#.into(),
        ));
        files.push((
            "titles/survivor.ron".into(),
            r#"(trigger: Counter(family: "deaths", key: "", at: 3))"#.into(),
        ));
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text = problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("titles/ghost_hunter.ron: counter `kills` has no key `ghost`"),
            "{text}"
        );
        assert!(!text.contains("titles/survivor.ron: counter"), "{text}");
    }

    #[test]
    fn a_title_proficiency_must_exist() {
        let mut files = quest_content();
        files.push((
            "titles/axe_master.ron".into(),
            r#"(trigger: Proficiency("axe", 20))"#.into(),
        ));
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text = problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("titles/axe_master.ron: proficiency `axe` not found"),
            "{text}"
        );
    }

    #[test]
    fn a_class_title_requirement_must_exist() {
        let mut files = quest_content();
        files.push((
            "classes/hermit.ron".into(),
            r#"(tier: 1, level: 1, requires: [Title("lost_soul")])"#.into(),
        ));
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text = problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("classes/hermit.ron: title `lost_soul` not found"),
            "{text}"
        );
    }

    #[test]
    fn a_title_bonus_must_name_something() {
        let mut files = quest_content();
        files.push((
            "titles/odd.ron".into(),
            r#"(trigger: Level(2), bonuses: [MaxHp(10), Special("grit", 5)], techniques: ["fireball"])"#
                .into(),
        ));
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text = problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("titles/odd.ron: special attribute `grit` not found"),
            "{text}"
        );
        assert!(
            text.contains("titles/odd.ron: technique `fireball` not found"),
            "{text}"
        );
    }

    #[test]
    fn a_title_cannot_replace_itself_or_nothing() {
        let mut files = quest_content();
        for (p, t) in [
            (
                "titles/loop_a.ron",
                r#"(trigger: Level(2), replaces: ["loop_b"])"#,
            ),
            (
                "titles/loop_b.ron",
                r#"(trigger: Level(3), replaces: ["loop_a"])"#,
            ),
            ("titles/lost.ron", r#"(trigger: Level(4), replaces: ["ghost"])"#),
        ] {
            files.push((p.into(), t.into()));
        }
        let problems = from_text(&refs(&files)).unwrap().validate().unwrap_err();
        let text = problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("titles/loop_a.ron: a title cannot replace itself"),
            "{text}"
        );
        assert!(
            text.contains("titles/loop_b.ron: a title cannot replace itself"),
            "{text}"
        );
        assert!(
            text.contains("titles/lost.ron: title `ghost` not found"),
            "{text}"
        );
    }
}

/// Phase 4 definitions, checked (built by `Catalog::validate_phase4`).
#[derive(Default)]
pub(crate) struct Phase4 {
    pub formulas: Formulas,
    pub monsters: Vec<MonsterDef>,
    pub classes: Vec<ClassDef>,
    pub proficiencies: Vec<ProficiencyDef>,
    pub techniques: Vec<TechniqueDef>,
    pub titles: Vec<TitleDef>,
    pub counters: Vec<String>,
    pub special_attributes: Vec<SpecialAttributeDef>,
}

impl ValidatedCatalog {
    pub fn formulas(&self) -> &Formulas {
        &self.formulas
    }

    pub fn monster(&self, id: MonsterId) -> Option<&MonsterDef> {
        self.monsters.get(id.index())
    }

    pub fn monster_by_name(&self, id: &str) -> Option<(MonsterId, &MonsterDef)> {
        let i = self.monsters.binary_search_by(|m| m.id.as_str().cmp(id)).ok()?;
        Some((MonsterId::new(u32::try_from(i).ok()?), self.monsters.get(i)?))
    }

    pub fn class(&self, id: ClassId) -> Option<&ClassDef> {
        self.classes.get(id.index())
    }

    pub fn class_by_name(&self, id: &str) -> Option<(ClassId, &ClassDef)> {
        let i = self.classes.binary_search_by(|m| m.id.as_str().cmp(id)).ok()?;
        Some((ClassId::new(u32::try_from(i).ok()?), self.classes.get(i)?))
    }

    pub fn classes(&self) -> impl Iterator<Item = (ClassId, &ClassDef)> {
        self.classes
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((ClassId::new(u32::try_from(i).ok()?), c)))
    }

    pub fn proficiency(&self, id: ProficiencyId) -> Option<&ProficiencyDef> {
        self.proficiencies.get(id.index())
    }

    pub fn proficiency_by_name(&self, id: &str) -> Option<(ProficiencyId, &ProficiencyDef)> {
        let i = self
            .proficiencies
            .binary_search_by(|m| m.id.as_str().cmp(id))
            .ok()?;
        Some((
            ProficiencyId::new(u32::try_from(i).ok()?),
            self.proficiencies.get(i)?,
        ))
    }

    pub fn technique(&self, id: TechniqueId) -> Option<&TechniqueDef> {
        self.techniques.get(id.index())
    }

    pub fn technique_by_name(&self, id: &str) -> Option<(TechniqueId, &TechniqueDef)> {
        let i = self.techniques.binary_search_by(|m| m.id.as_str().cmp(id)).ok()?;
        Some((TechniqueId::new(u32::try_from(i).ok()?), self.techniques.get(i)?))
    }

    pub fn techniques(&self) -> impl Iterator<Item = (TechniqueId, &TechniqueDef)> {
        self.techniques
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((TechniqueId::new(u32::try_from(i).ok()?), c)))
    }

    pub fn title(&self, id: TitleId) -> Option<&TitleDef> {
        self.titles.get(id.index())
    }

    pub fn title_by_name(&self, id: &str) -> Option<(TitleId, &TitleDef)> {
        let i = self.titles.binary_search_by(|m| m.id.as_str().cmp(id)).ok()?;
        Some((TitleId::new(u32::try_from(i).ok()?), self.titles.get(i)?))
    }

    /// Special attributes, sorted by id.
    pub fn special_attributes(&self) -> &[SpecialAttributeDef] {
        &self.special_attributes
    }

    pub fn titles(&self) -> impl Iterator<Item = (TitleId, &TitleDef)> {
        self.titles
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((TitleId::new(u32::try_from(i).ok()?), c)))
    }

    /// Counter families on (`counters.ron`).
    pub fn counter_families(&self) -> &[String] {
        &self.counters
    }

    /// Shared loot table (phase 7).
    pub fn loot_table(&self, id: LootTableId) -> Option<&LootTableDef> {
        self.loot_tables.get(id.index())
    }

    pub fn loot_table_by_name(&self, id: &str) -> Option<(LootTableId, &LootTableDef)> {
        let i = self
            .loot_tables
            .binary_search_by(|t| t.id.as_str().cmp(id))
            .ok()?;
        Some((LootTableId::new(u32::try_from(i).ok()?), self.loot_tables.get(i)?))
    }

    pub fn shop(&self, id: ShopId) -> Option<&ShopDef> {
        self.shops.get(id.index())
    }

    pub fn shop_by_name(&self, id: &str) -> Option<(ShopId, &ShopDef)> {
        let i = self.shops.binary_search_by(|s| s.id.as_str().cmp(id)).ok()?;
        Some((ShopId::new(u32::try_from(i).ok()?), self.shops.get(i)?))
    }

    /// Recipe (phase 8).
    pub fn recipe(&self, id: RecipeId) -> Option<&RecipeDef> {
        self.recipes.get(id.index())
    }

    pub fn recipe_by_name(&self, id: &str) -> Option<(RecipeId, &RecipeDef)> {
        let i = self.recipes.binary_search_by(|r| r.id.as_str().cmp(id)).ok()?;
        Some((RecipeId::new(u32::try_from(i).ok()?), self.recipes.get(i)?))
    }

    pub fn recipes(&self) -> impl Iterator<Item = (RecipeId, &RecipeDef)> {
        self.recipes
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((RecipeId::new(u32::try_from(i).ok()?), r)))
    }

    /// Resource node (phase 8).
    /// Emote (phase 9); `EmoteId` is the position, sorted by id.
    pub fn emote(&self, id: EmoteId) -> Option<&EmoteDef> {
        self.emotes.get(id.index())
    }

    pub fn emote_by_name(&self, id: &str) -> Option<(EmoteId, &EmoteDef)> {
        let i = self.emotes.binary_search_by(|e| e.id.as_str().cmp(id)).ok()?;
        Some((EmoteId::new(u32::try_from(i).ok()?), self.emotes.get(i)?))
    }

    /// Dungeons, sorted by id.
    pub fn dungeons(&self) -> &[crate::DungeonDef] {
        &self.dungeons
    }

    /// The dungeon that runs in `zone`, if any.
    pub fn dungeon_of_zone(&self, zone: ZoneId) -> Option<&crate::DungeonDef> {
        self.dungeons.iter().find(|d| d.zone == zone)
    }

    /// Rankings, sorted by id.
    pub fn rankings(&self) -> &[crate::RankingDef] {
        &self.rankings
    }

    /// Server goals, sorted by id.
    pub fn goals(&self) -> &[crate::GoalDef] {
        &self.goals
    }

    pub fn emotes(&self) -> impl Iterator<Item = (EmoteId, &EmoteDef)> {
        self.emotes
            .iter()
            .enumerate()
            .filter_map(|(i, e)| Some((EmoteId::new(u32::try_from(i).ok()?), e)))
    }

    pub fn node(&self, id: NodeId) -> Option<&NodeDef> {
        self.nodes.get(id.index())
    }

    pub fn node_by_name(&self, id: &str) -> Option<(NodeId, &NodeDef)> {
        let i = self.nodes.binary_search_by(|n| n.id.as_str().cmp(id)).ok()?;
        Some((NodeId::new(u32::try_from(i).ok()?), self.nodes.get(i)?))
    }
}

impl Catalog {
    pub(crate) fn validate_phase4(
        &self,
        keys: &KeyTable,
        items: &BTreeMap<String, ItemId>,
        table_ids: &BTreeMap<String, LootTableId>,
        class_ids: &BTreeMap<String, ClassId>,
        prof_ids: &BTreeMap<String, ProficiencyId>,
        problems: &mut Vec<ContentProblem>,
    ) -> Phase4 {
        let mut out = Phase4::default();
        let mut err = |path: String, message: String| problems.push(ContentProblem::File { path, message });
        match parse_formulas(
            self.progression_formulas.as_deref(),
            self.combat_formulas.as_deref(),
            self.economy_formulas.as_deref(),
        ) {
            Ok(f) => out.formulas = f,
            Err(msgs) => msgs.into_iter().for_each(|m| err("formulas".into(), m)),
        }
        let key = |path: &str, k: String, err: &mut dyn FnMut(String, String)| {
            keys.id(&k).unwrap_or_else(|| {
                err(path.to_owned(), format!("no text key `{k}`"));
                TextId::new(0)
            })
        };
        for (id, text) in &self.proficiencies {
            let path = format!("proficiencies/{id}.ron");
            match progress::parse_proficiency(id, text) {
                Ok(f) => out.proficiencies.push(ProficiencyDef {
                    id: id.clone(),
                    name: key(&path, progress::proficiency_name_key(id), &mut err),
                    kind: f.kind,
                }),
                Err(m) => err(path, m),
            }
        }
        if !self.proficiencies.is_empty() && !prof_ids.contains_key(progress::UNARMED) {
            err(
                "proficiencies/".into(),
                format!("`{}.ron` is required", progress::UNARMED),
            );
        }
        let title_ids: BTreeMap<String, TitleId> = self
            .titles
            .keys()
            .enumerate()
            .filter_map(|(i, k)| Some((k.clone(), TitleId::new(u32::try_from(i).ok()?))))
            .collect();
        let tame_names = crate::tame::TameNames {
            items,
            proficiencies: prof_ids,
            titles: &title_ids,
            classes: class_ids,
        };
        for (id, text) in &self.monsters {
            let path = format!("monsters/{id}.ron");
            let f = match progress::parse_monster(id, text) {
                Ok(f) => f,
                Err(m) => {
                    err(path, m);
                    continue;
                }
            };
            let loot = progress::resolve_loot(f.loot, items).unwrap_or_else(|m| {
                err(path.clone(), m);
                Vec::new()
            });
            let loot_table = if f.loot_table.is_empty() {
                None
            } else {
                match table_ids.get(&f.loot_table) {
                    Some(&id) => Some(id),
                    None => {
                        err(path.clone(), format!("loot table `{}` not found", f.loot_table));
                        None
                    }
                }
            };
            let tame = match f.tame {
                None => None,
                Some(t) => match crate::tame::resolve(t, &tame_names) {
                    Ok(t) => Some(t),
                    Err(m) => {
                        err(path.clone(), m);
                        None
                    }
                },
            };
            out.monsters.push(MonsterDef {
                id: id.clone(),
                name: key(&path, progress::monster_name_key(id), &mut err),
                level: f.level,
                hp: f.hp,
                attributes: f.attributes,
                damage: f.damage,
                defense: f.defense,
                weapon_prof: f.weapon_prof,
                attack_ticks: f.attack_ticks,
                aggro_radius: f.aggro_radius,
                leash: f.leash,
                range: f.range.max(1),
                respawn_ticks: f.respawn_ticks,
                xp: f.xp,
                sprite: f.sprite,
                icon: f.icon,
                coins: f.coins,
                loot,
                loot_table,
                phases: f.phases,
                tame,
            });
        }
        // Class techniques resolve once the techniques have ids.
        let mut class_techniques: Vec<(String, Vec<String>)> = Vec::new();
        let title_ids: BTreeMap<String, TitleId> = self
            .titles
            .keys()
            .enumerate()
            .filter_map(|(i, k)| Some((k.clone(), TitleId::new(u32::try_from(i).ok()?))))
            .collect();
        for (id, text) in &self.classes {
            let path = format!("classes/{id}.ron");
            let f = match progress::parse_class(id, text) {
                Ok(f) => f,
                Err(m) => {
                    err(path, m);
                    continue;
                }
            };
            let parent = match &f.parent {
                None => None,
                Some(p) => match (
                    class_ids.get(p),
                    self.classes.get(p).map(|t| progress::parse_class(p, t)),
                ) {
                    (Some(&pid), Some(Ok(pf))) if pf.tier + 1 == f.tier && pf.level < f.level => Some(pid),
                    _ => {
                        err(
                            path.clone(),
                            format!("parent `{p}` must exist, be one tier below and earlier"),
                        );
                        None
                    }
                },
            };
            let mut requires = Vec::new();
            for r in f.requires {
                match progress::resolve_req(r, prof_ids, &title_ids) {
                    Ok(r) => requires.push(r),
                    Err(m) => err(path.clone(), m),
                }
            }
            let mut growth = Vec::new();
            for (p, mult) in f.growth {
                match prof_ids.get(&p) {
                    Some(&id) => growth.push((id, mult)),
                    None => err(path.clone(), format!("growth: proficiency `{p}` not found")),
                }
            }
            for sp in &f.specials {
                if !self.special_attributes.contains_key(&sp.special) {
                    err(
                        path.clone(),
                        format!("special attribute `{}` not found", sp.special),
                    );
                }
            }
            class_techniques.push((path.clone(), f.techniques));
            out.classes.push(ClassDef {
                id: id.clone(),
                name: key(&path, progress::class_name_key(id), &mut err),
                description: key(&path, progress::class_desc_key(id), &mut err),
                tier: f.tier,
                parent,
                level: f.level,
                requires,
                auto_points: f.auto_points,
                growth,
                craft_xp_permille: f.craft_xp_permille,
                quality_cap: f.quality_cap,
                icon: f.icon,
                specials: f.specials,
                techniques: Vec::new(),
                unique: f.unique,
            });
        }
        for (id, text) in &self.techniques {
            let path = format!("techniques/{id}.ron");
            let f = match progress::parse_technique(id, text) {
                Ok(f) => f,
                Err(m) => {
                    err(path, m);
                    continue;
                }
            };
            let Some(&proficiency) = prof_ids.get(&f.proficiency) else {
                err(path, format!("proficiency `{}` not found", f.proficiency));
                continue;
            };
            for (sp, _) in &f.requires_special {
                if !self.special_attributes.contains_key(sp) {
                    err(path.clone(), format!("special attribute `{sp}` not found"));
                }
            }
            out.techniques.push(TechniqueDef {
                id: id.clone(),
                name: key(&path, progress::technique_name_key(id), &mut err),
                description: key(&path, progress::technique_desc_key(id), &mut err),
                proficiency,
                unlock_at: f.unlock_at,
                cooldown_ticks: f.cooldown_ticks,
                vigor: f.vigor,
                mp: f.mp,
                damage_permille: f.damage_permille,
                heal_permille: f.heal_permille,
                revive_permille: f.revive_permille,
                growth_permille: f.growth_permille,
                range: f.range,
                icon: f.icon,
                buff: f.buff,
                requires_special: f.requires_special,
                taught_only: f.taught_only,
                area: f.area,
                status: f.status,
            });
        }
        if let Some(text) = &self.counters {
            match progress::parse_counters(text) {
                Ok(c) => out.counters = c,
                Err(m) => err("counters.ron".into(), m),
            }
        }
        for (id, text) in &self.special_attributes {
            let path = format!("special_attributes/{id}.ron");
            match progress::parse_special_attribute(id, text) {
                Ok(f) => {
                    let mut scales = Vec::new();
                    for (p, k) in f.scales {
                        match prof_ids.get(&p) {
                            Some(&pid) => scales.push((pid, k)),
                            None => err(path.clone(), format!("scales: proficiency `{p}` not found")),
                        }
                    }
                    out.special_attributes.push(SpecialAttributeDef {
                        id: id.clone(),
                        name: key(&path, progress::special_name_key(id), &mut err),
                        description: key(&path, progress::special_desc_key(id), &mut err),
                        icon: f.icon,
                        scales,
                    });
                }
                Err(m) => err(path, m),
            }
        }
        let technique_ids: BTreeMap<&str, TechniqueId> = out
            .techniques
            .iter()
            .enumerate()
            .filter_map(|(i, t)| Some((t.id.as_str(), TechniqueId::new(u32::try_from(i).ok()?))))
            .collect();
        for (class, (path, names)) in out.classes.iter_mut().zip(class_techniques) {
            for t in names {
                match technique_ids.get(t.as_str()) {
                    Some(&tid) => class.techniques.push(tid),
                    None => err(path.clone(), format!("technique `{t}` not found")),
                }
            }
        }
        let mut direct_replaces: Vec<(String, Vec<String>)> = Vec::new();
        for (id, text) in &self.titles {
            let path = format!("titles/{id}.ron");
            let f = match progress::parse_title(id, text) {
                Ok(f) => f,
                Err(m) => {
                    err(path, m);
                    continue;
                }
            };
            if let TitleTriggerFile::Counter { family, key, .. } = &f.trigger {
                if !out.counters.contains(family) {
                    err(
                        path.clone(),
                        format!("counter family `{family}` is not declared in counters.ron"),
                    );
                }
                // A key that names nothing would never be counted, so the
                // title could never be earned.
                let exists = match progress::counter_key_kind(family) {
                    _ if key.is_empty() => true,
                    Some("monster") => self.monsters.contains_key(key),
                    Some("quest") => self.quests.contains_key(key),
                    Some("dungeon") => self.dungeons.contains_key(key),
                    _ => false,
                };
                if !exists {
                    err(path.clone(), format!("counter `{family}` has no key `{key}`"));
                }
            }
            let trigger = match f.trigger {
                TitleTriggerFile::Level(l) => TitleTrigger::Level(l),
                TitleTriggerFile::Counter { family, key, at } => TitleTrigger::Counter { family, key, at },
                TitleTriggerFile::Attribute(a, v) => TitleTrigger::Attribute(a, v),
                TitleTriggerFile::Karma(v) => {
                    if !(-crate::KARMA_MAX..=crate::KARMA_MAX).contains(&v) {
                        err(
                            path.clone(),
                            format!("karma {v} must be -{0} to {0}", crate::KARMA_MAX),
                        );
                    }
                    TitleTrigger::Karma(v)
                }
                TitleTriggerFile::Proficiency(p, v) => match prof_ids.get(&p) {
                    Some(&id) => TitleTrigger::Proficiency(id, v),
                    None => {
                        err(path, format!("proficiency `{p}` not found"));
                        continue;
                    }
                },
            };
            for b in &f.bonuses {
                if let TitleBonus::Special(s, _) = b
                    && !self.special_attributes.contains_key(s)
                {
                    err(path.clone(), format!("special attribute `{s}` not found"));
                }
            }
            let mut techniques = Vec::with_capacity(f.techniques.len());
            for t in &f.techniques {
                match technique_ids.get(t.as_str()) {
                    Some(&tid) => techniques.push(tid),
                    None => err(path.clone(), format!("technique `{t}` not found")),
                }
            }
            out.titles.push(TitleDef {
                id: id.clone(),
                name: key(&path, progress::title_name_key(id), &mut err),
                description: key(&path, progress::title_desc_key(id), &mut err),
                trigger,
                icon: f.icon,
                bonuses: f.bonuses,
                techniques,
                replaces: Vec::new(),
                announce: f.announce,
                unique: f.unique,
            });
            direct_replaces.push((path, f.replaces));
        }
        close_replaces(&mut out.titles, &direct_replaces, &mut err);
        out
    }
}

/// Resolves each title's `replaces` to ids and closes it: a title also
/// replaces what the titles it replaces replace. A title may not replace
/// itself, directly or around a loop.
fn close_replaces(
    titles: &mut [TitleDef],
    direct: &[(String, Vec<String>)],
    err: &mut dyn FnMut(String, String),
) {
    let index: BTreeMap<&str, usize> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); titles.len()];
    for (i, (path, names)) in direct.iter().enumerate() {
        for name in names {
            match index.get(name.as_str()) {
                Some(&j) => {
                    if let Some(e) = edges.get_mut(i) {
                        e.push(j);
                    }
                }
                None => err(path.clone(), format!("title `{name}` not found")),
            }
        }
    }
    for (i, (path, _)) in direct.iter().enumerate() {
        let mut seen = vec![false; titles.len()];
        let mut stack: Vec<usize> = edges.get(i).cloned().unwrap_or_default();
        while let Some(j) = stack.pop() {
            if seen.get(j).copied().unwrap_or(true) {
                continue;
            }
            if let Some(s) = seen.get_mut(j) {
                *s = true;
            }
            stack.extend(edges.get(j).into_iter().flatten().copied());
        }
        if seen.get(i).copied().unwrap_or(false) {
            err(
                path.clone(),
                "a title cannot replace itself (directly or in a loop)".into(),
            );
        }
        let ids: Vec<TitleId> = seen
            .iter()
            .enumerate()
            .filter(|&(j, &s)| s && j != i)
            .filter_map(|(j, _)| Some(TitleId::new(u32::try_from(j).ok()?)))
            .collect();
        if let Some(t) = titles.get_mut(i) {
            t.replaces = ids;
        }
    }
}

fn index_of(list: &[String], name: &str) -> u16 {
    if name.is_empty() {
        return 0;
    }
    list.binary_search_by(|n| n.as_str().cmp(name))
        .ok()
        .and_then(|i| u16::try_from(i + 1).ok())
        .unwrap_or(0)
}

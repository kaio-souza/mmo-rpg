use base::{
    Attribute, CharacterName, ClassId, Direction, EntityId, ItemId, PlayerId, ProficiencyId, Rarity,
    RecipeId, TechniqueId, TextId, TextRef, Tick, TilePos, TitleId,
};
use defs::Attributes;
use serde::{Deserialize, Serialize};

/// What a character brings into a zone and takes out of it. Content is
/// named by text id (`"glow_herb"`), never by numeric id: numeric ids
/// change when content changes; the zone converts on the way in and out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterSheet {
    pub name: CharacterName,
    /// Look chosen at creation, by text id in `LOOK_LISTS` order (body,
    /// skin, hairstyle, hair color); empty ids draw the first option.
    pub look: [String; 4],
    /// Copper.
    pub coins: u64,
    /// One entry per backpack slot (`BACKPACK_SLOTS`); shorter is padded.
    pub backpack: Vec<Option<SavedStack>>,
    pub quests: Vec<SavedQuest>,
    /// Affinity with NPCs, by NPC id; zero is not stored.
    pub affinity: Vec<(String, i32)>,
    /// Phase 4: level, attributes, class, proficiencies, techniques,
    /// titles and counters.
    pub progress: SavedProgress,
    /// Items with identity (equipment), equipped or in the backpack.
    pub items: Vec<SavedItem>,
    /// Recipes the character knows (phase 8), by text id.
    pub recipes: Vec<String>,
    /// Current HP; 0 enters with full HP.
    pub hp: u32,
    /// Unix second before which the character cannot enter (death).
    pub locked_until: u64,
    /// Alpha (sheet v8): coins each shop paid this character since the
    /// unix second its purse opened, by shop id.
    pub shop_sales: Vec<(String, u64, u64)>,
}

/// Progression by text id. `level == 0`: a sheet from before phase 4,
/// which starts at level 1.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedProgress {
    pub level: u8,
    pub xp: u64,
    pub max_level: u8,
    pub attributes: Attributes,
    pub free_points: u16,
    pub class: Option<String>,
    /// `(id, points, xp)`.
    pub proficiencies: Vec<(String, u8, u64)>,
    /// `(id, level, uses)`.
    pub techniques: Vec<(String, u8, u64)>,
    pub titles: Vec<String>,
    /// Declared counters: `(family, key, value)`.
    pub counters: Vec<(String, String, u64)>,
}

/// An item with identity as saved. The record of an item, not
/// the item: only `Zone` turns it into a live `ItemInstance` (no `Clone`),
/// and the database keeps each id with one owner (`item_owners`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedItem {
    pub id: u64,
    pub item: String,
    pub place: ItemPlace,
    /// Rolled bonuses, hidden until appraised.
    pub hidden: Vec<(Attribute, u16)>,
    pub appraised: bool,
    /// Phase 7: durability left and the maximum of this piece. `(0, 0)`
    /// on an older save takes the item's own durability.
    pub durability: (u32, u32),
    /// Phase 8: who crafted it.
    pub crafter: Option<String>,
    /// Rarity rolled for this piece (phase 8); older saves take the
    /// item's own.
    pub rarity: Option<Rarity>,
    /// Phase 10: mastery of its crafter, hundredths of a percent.
    pub mastery: u16,
    /// Phase 10: the power it rolled.
    pub power: Option<SavedPower>,
}

/// A power as saved, by text id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavedPower {
    Technique(String),
    Attribute(Attribute, u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ItemPlace {
    Equipped(EquipSlot),
    Backpack(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EquipSlot {
    Weapon,
    Armor,
    /// Phase 11: the other hand (a torch).
    Offhand,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 3] = [EquipSlot::Weapon, EquipSlot::Armor, EquipSlot::Offhand];

    /// Position in `ALL` (the worn pieces of `InventoryView`).
    pub fn index(self) -> u16 {
        match self {
            EquipSlot::Weapon => 0,
            EquipSlot::Armor => 1,
            EquipSlot::Offhand => 2,
        }
    }
}

impl CharacterSheet {
    /// A new character: empty backpack, no coins.
    pub fn new(name: CharacterName) -> CharacterSheet {
        CharacterSheet {
            name,
            look: Default::default(),
            coins: 0,
            backpack: Vec::new(),
            quests: Vec::new(),
            affinity: Vec::new(),
            progress: SavedProgress::default(),
            items: Vec::new(),
            recipes: Vec::new(),
            hp: 0,
            locked_until: 0,
            shop_sales: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub qty: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQuest {
    pub quest: String,
    pub completed: bool,
    /// Bit `i`: `Talk` objective `i` done.
    pub talked: u8,
    /// Unix second of the last completion (0: never); a repeatable quest
    /// with a cooldown starts again only after it (alpha, sheet v7).
    pub completed_at: u64,
}

/// Everything that changes a zone arrives as a command, so the command
/// stream alone reproduces the zone (replay).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// `pos: None` puts the character at the zone spawn.
    Enter {
        player: PlayerId,
        sheet: Box<CharacterSheet>,
        pos: Option<TilePos>,
    },
    Leave {
        player: PlayerId,
    },
    /// Path by A*, at most `MAX_MOVE_DISTANCE` tiles away; replaces the current walk.
    Move {
        player: PlayerId,
        destination: TilePos,
    },
    Stop {
        player: PlayerId,
    },
    /// Opens the dialogue of a nearby NPC.
    Talk {
        player: PlayerId,
        target: EntityId,
    },
    /// Picks an option of the player's open dialogue (position among the
    /// options shown).
    ChooseOption {
        player: PlayerId,
        option: u16,
    },
    /// Merges or swaps two backpack slots.
    MoveItem {
        player: PlayerId,
        from: u16,
        to: u16,
    },
    /// Selects a target: the weapon attacks it on its own interval.
    Attack {
        player: PlayerId,
        target: EntityId,
    },
    UseTechnique {
        player: PlayerId,
        technique: TechniqueId,
        target: EntityId,
    },
    /// Spends free points (permanent).
    AllocatePoints {
        player: PlayerId,
        points: Attributes,
    },
    ChooseClass {
        player: PlayerId,
        class: ClassId,
    },
    /// Equips the item of a backpack slot (the old one goes to that slot).
    Equip {
        player: PlayerId,
        slot: u16,
    },
    Unequip {
        player: PlayerId,
        slot: EquipSlot,
    },
    /// Tries to reveal the hidden attributes of a backpack item.
    Appraise {
        player: PlayerId,
        slot: u16,
    },
    /// Fallen player gives up waiting: dies now.
    GiveUp {
        player: PlayerId,
    },
    /// Item ids reserved in the database for this zone (`[start, end)`).
    ItemIds {
        start: u64,
        end: u64,
    },
    /// Phase 5: uses one item of a backpack stack (`use` of a consumable).
    UseItem {
        player: PlayerId,
        slot: u16,
    },
    /// Uses one item of a backpack stack on another player (a
    /// revival item on a fallen one nearby).
    UseItemOn {
        player: PlayerId,
        slot: u16,
        target: EntityId,
    },
    /// Phase 5: closes the player's open dialogue.
    CloseDialogue {
        player: PlayerId,
    },
    /// Phase 7: opens the shop of a nearby NPC.
    OpenShop {
        player: PlayerId,
        target: EntityId,
    },
    CloseShop {
        player: PlayerId,
    },
    /// Buys from the open shop, by line of its stock.
    Buy {
        player: PlayerId,
        index: u16,
        qty: u32,
    },
    /// Sells from a backpack slot to the open shop.
    Sell {
        player: PlayerId,
        slot: u16,
        qty: u32,
    },
    /// Repairs the equipment of a backpack slot at the open shop.
    Repair {
        player: PlayerId,
        slot: u16,
    },
    /// Phase 8: crafts one batch of a known recipe.
    Craft {
        player: PlayerId,
        recipe: RecipeId,
    },
    /// Gathers a resource node within reach.
    Gather {
        player: PlayerId,
        target: EntityId,
    },
    /// Phase 11: switches a tile of the zone within reach (a
    /// brazier lit or put out).
    UseTile {
        player: PlayerId,
        pos: TilePos,
    },
    /// Phase 9: invites a player to the group (the inviter's, or a new one).
    PartyInvite {
        player: PlayerId,
        target: EntityId,
    },
    /// Answers the latest invite to the group.
    AnswerInvite {
        player: PlayerId,
        accept: bool,
    },
    LeaveParty {
        player: PlayerId,
    },
    /// The leader removes a member.
    PartyKick {
        player: PlayerId,
        target: EntityId,
    },
    /// The leader hands the lead to a member.
    PartyLead {
        player: PlayerId,
        target: EntityId,
    },
    /// Asks a nearby player to trade; when both have asked each other the
    /// trade opens.
    TradeRequest {
        player: PlayerId,
        target: EntityId,
    },
    /// Sets what this side offers: backpack slots with quantities, and
    /// coins. Any change takes both confirmations back.
    TradeOffer {
        player: PlayerId,
        items: Vec<(u16, u32)>,
        coins: u64,
    },
    /// Confirms the current offers; when both sides did, the swap happens.
    TradeConfirm {
        player: PlayerId,
    },
    TradeCancel {
        player: PlayerId,
    },
    /// Phase 9: a gesture over the character, seen by whoever sees it.
    Emote {
        player: PlayerId,
        emote: base::EmoteId,
    },
    /// Phase 10: a player handed over by a neighbor zone (the seam): same
    /// entity id, tile, HP, vigor, MP and walk.
    Arrive {
        player: PlayerId,
        arrival: Box<crate::Arrival>,
    },
    /// Phase 10: what changed on a neighbor's side of the seam (its ghosts
    /// here). An empty diff still says the neighbor is up.
    Ghosts {
        from: base::ZoneId,
        diff: crate::GhostDiff,
    },
    /// Alpha: the open shop appraises a backpack piece for coins.
    ShopAppraise {
        player: PlayerId,
        slot: u16,
    },
    /// A production class mends a backpack piece with the main
    /// material of its recipe, anywhere, without a shop.
    SelfRepair {
        player: PlayerId,
        slot: u16,
    },
    /// Rule 4: backpack slots `(slot, qty)` and coins leave the
    /// sheet into custody for the character `to`; the binary writes the
    /// sheet and the parcel in one transaction and answers `EscrowDone`.
    EscrowDeposit {
        player: PlayerId,
        /// The sender's character in the database, given back with a
        /// refusal the zone cannot return (`TickOutput::escrow_lost`).
        from: base::CharacterId,
        items: Vec<(u16, u32)>,
        coins: u64,
        to: CharacterName,
    },
    /// The database answer to `TickOutput::escrows`: `ok: false` gives the
    /// parcel back to the player.
    EscrowDone {
        ticket: u64,
        ok: bool,
    },
    /// A parcel taken out of custody for `player` (row `escrow` of the
    /// database): into the backpack, or back in `TickOutput::delivered`.
    Deliver {
        player: PlayerId,
        escrow: u64,
        parcel: Box<EscrowParcel>,
    },
    /// Unique titles and classes other characters of the server hold
    /// (`title:<id>`, `class:<id>`): out of reach for this player.
    Taken {
        player: PlayerId,
        keys: Vec<String>,
    },
    /// Another character holds this unique title or class: the player
    /// loses it.
    Revoke {
        player: PlayerId,
        key: String,
    },
    /// Unique items that exist on the server (`item:<id>`): this zone never
    /// makes them while they do.
    TakenItems {
        keys: Vec<String>,
    },
    /// Every player of the zone goes to this tile (an instance whose time
    /// ran out).
    Evict {
        to: TilePos,
    },
}

/// What custody holds, in saved form (rule 4). Each piece keeps
/// its id, crafter, rarity, durability and hidden bonuses: the database
/// moves it, never copies it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowParcel {
    pub coins: u64,
    pub stacks: Vec<SavedStack>,
    pub items: Vec<SavedItem>,
}

impl EscrowParcel {
    pub fn is_empty(&self) -> bool {
        self.coins == 0 && self.stacks.is_empty() && self.items.is_empty()
    }
}

/// A deposit to write: the sheet without the parcel, the parcel and who
/// receives it, all in one transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowOut {
    pub ticket: u64,
    pub player: PlayerId,
    pub from: base::CharacterId,
    pub to: CharacterName,
    pub parcel: EscrowParcel,
    pub sheet: CharacterSheet,
    pub pos: TilePos,
}

/// Result of a `Deliver`: the sheet to save now, or the parcel to put back
/// in custody (no room, or the player is gone).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveredOut {
    pub player: PlayerId,
    pub escrow: u64,
    pub result: Result<(CharacterSheet, TilePos), EscrowParcel>,
}

/// Where coins came from or went (phase 7), for the economy telemetry
/// Sources create
/// coins, sinks destroy them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CoinFlow {
    /// Source: dropped by a monster.
    MonsterLoot,
    /// Source: a quest reward.
    QuestReward,
    /// Source: a dialogue handing coins over.
    Gift,
    /// Source: sold to an NPC.
    ShopSale,
    /// Sink: bought from an NPC.
    ShopPurchase,
    /// Sink: a repair.
    Repair,
    /// Sink: a dialogue taking coins.
    Fee,
    /// Sink: an NPC appraisal (alpha).
    Appraise,
}

impl CoinFlow {
    pub const ALL: [CoinFlow; 8] = [
        CoinFlow::MonsterLoot,
        CoinFlow::QuestReward,
        CoinFlow::Gift,
        CoinFlow::ShopSale,
        CoinFlow::ShopPurchase,
        CoinFlow::Repair,
        CoinFlow::Fee,
        CoinFlow::Appraise,
    ];

    /// Sources create coins; sinks destroy them.
    pub fn creates(self) -> bool {
        matches!(
            self,
            CoinFlow::MonsterLoot | CoinFlow::QuestReward | CoinFlow::Gift | CoinFlow::ShopSale
        )
    }

    /// Name in the telemetry and in `admin report economy`.
    pub fn name(self) -> &'static str {
        match self {
            CoinFlow::MonsterLoot => "monster_loot",
            CoinFlow::QuestReward => "quest_reward",
            CoinFlow::Gift => "gift",
            CoinFlow::ShopSale => "shop_sale",
            CoinFlow::ShopPurchase => "shop_purchase",
            CoinFlow::Repair => "repair",
            CoinFlow::Fee => "fee",
            CoinFlow::Appraise => "appraise",
        }
    }
}

/// What a player has to do with an NPC, shown over its head (phase 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QuestMarker {
    /// A quest the player can start.
    Available,
    /// A `Talk` objective of an active quest.
    Talk,
    /// A quest the player can hand in here.
    Ready,
}

/// Overload level of the zone, measured by the binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum LoadLevel {
    #[default]
    N0,
    N1,
    N2,
    N3,
}

pub struct TickInput<'a> {
    pub commands: &'a [Command],
    /// Wall clock, read by the binary; the simulation never reads the clock.
    pub now_unix: u64,
    pub load: LoadLevel,
}

/// Cleared and reused every tick.
#[derive(Debug, Default)]
pub struct TickOutput {
    pub tick: Tick,
    /// Result of each `Enter`, in command order.
    pub entered: Vec<(PlayerId, EntityId, TilePos)>,
    /// Result of each `Leave`, in command order, with the last position
    /// (the binary saves both).
    pub left: Vec<(PlayerId, CharacterSheet, TilePos)>,
    /// Only players whose view changed, sorted by `PlayerId`.
    pub views: Vec<(PlayerId, View)>,
    /// Full state of every entity in any `View::entered`, sorted by id, once each:
    /// the binary encodes each one once and shares it.
    pub entities: Vec<EntityState>,
    /// Dialogue nodes shown or closed this tick, in command order.
    pub dialogue: Vec<DialogueOut>,
    /// `MoveItem` that could not be done, in command order.
    pub refused: Vec<(PlayerId, Refusal)>,
    /// Backpacks that changed this tick (or entered), sorted by player.
    pub inventories: Vec<(PlayerId, InventoryView)>,
    /// Quest logs that changed this tick (or entered), sorted by player.
    pub quest_logs: Vec<(PlayerId, Vec<QuestView>)>,
    /// The content changed this tick (`Zone::reload`): every player gets
    /// the new definitions before the rest of the tick.
    pub reloaded: bool,
    /// The reload changed the tiles (same size): clients need new chunks.
    pub map_changed: bool,
    /// Phase 11: tiles a player switched this tick, with their
    /// new chunk word, in order.
    pub tiles: Vec<(TilePos, u16)>,
    /// System messages (level up, loot, titles...), in order: the client
    /// shows them in a System window.
    pub system: Vec<(PlayerId, TextRef)>,
    /// Routine System messages (XP, proficiency and technique levels, an
    /// item used): only the log (phase 5).
    pub notes: Vec<(PlayerId, TextRef)>,
    /// Quest markers of every NPC of the zone, for players whose quests
    /// changed (or entered), sorted by player (phase 5).
    pub markers: Vec<(PlayerId, Vec<(EntityId, QuestMarker)>)>,
    /// Blows this tick, for whoever sees the target.
    pub combat: Vec<CombatOut>,
    /// Support techniques used this tick (heal, buff, revive),
    /// which land no blow: `(user, target, technique)`, for their sound.
    pub supports: Vec<(EntityId, EntityId, base::TechniqueId)>,
    /// Own sheets that changed this tick (or entered), sorted by player.
    pub stats: Vec<(PlayerId, StatsView)>,
    /// Characters that died for good this tick: the binary saves the sheet
    /// (with `locked_until`) and takes them out.
    pub dead: Vec<PlayerId>,
    /// The zone is running out of item ids: send `Command::ItemIds`.
    pub need_item_ids: bool,
    /// Shops opened, changed or closed this tick (phase 7), sorted by player.
    pub shops: Vec<(PlayerId, Option<ShopView>)>,
    /// NPCs of the zone that keep a shop, for players who entered or whose
    /// content was reloaded (phase 7), sorted by player.
    pub shop_npcs: Vec<(PlayerId, Vec<EntityId>)>,
    /// Coins created and destroyed this tick, for the telemetry (phase 7).
    pub coins: Vec<(CoinFlow, u64)>,
    /// Phase 9: group of each player whose group changed (`None`: left it),
    /// sorted by player.
    pub parties: Vec<(PlayerId, Option<PartyView>)>,
    /// Trade of each player whose trade changed (`None`: it ended), sorted
    /// by player.
    pub trades: Vec<(PlayerId, Option<TradeView>)>,
    /// Invites and trade requests to deliver, in order.
    pub invites: Vec<InviteOut>,
    /// Emotes this tick: `(who sees it, who gestures, which)`. The emoter
    /// sees its own.
    pub emotes: Vec<(PlayerId, EntityId, base::EmoteId)>,
    /// Phase 10: players that crossed the seam this tick, for the zone that
    /// owns their tile now. They left this zone (not in `left`: nothing to
    /// save yet, the next zone has them).
    pub handoffs: Vec<(PlayerId, base::ZoneId, Box<crate::Arrival>)>,
    /// Phase 10: every entity near each neighbor's side of the seam, sorted
    /// by id; the binary sends each neighbor the difference (ghosts).
    pub border: Vec<(base::ZoneId, Vec<EntityState>)>,
    /// Rule 4: deposits to write (sheet and parcel together).
    pub escrows: Vec<EscrowOut>,
    /// Refused deposits the zone could not give back (player gone or bag
    /// full): `(ticket, sender, parcel)`, back to custody for the sender.
    pub escrow_lost: Vec<(u64, base::CharacterId, EscrowParcel)>,
    /// Results of `Deliver`, in command order.
    pub delivered: Vec<DeliveredOut>,
    /// Titles earned this tick that the whole server may hear of
    /// (`Announce::First` or `Everyone`), in order: the gateway records
    /// server firsts and tells everyone.
    pub feats: Vec<(PlayerId, base::TitleId)>,
    /// Unique titles and classes a player just got (`title:<id>`,
    /// `class:<id>`), in order: the gateway claims them for the server.
    pub claims: Vec<(PlayerId, String)>,
    /// Unique items gone this tick (broken or sold): `item:<id>`, free to
    /// exist again.
    pub released: Vec<String>,
    /// Dungeon runs cleared this tick (its boss died), in order.
    pub clears: Vec<ClearOut>,
}

/// A dungeon run cleared: who was inside and how long the run took.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearOut {
    /// Dungeon id.
    pub dungeon: String,
    /// Every player in the zone, sorted.
    pub players: Vec<PlayerId>,
    /// Seconds since the copy opened.
    pub secs: u64,
}

/// A group invite or trade request for `to`, from the entity `from`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteOut {
    pub to: PlayerId,
    pub from: EntityId,
    pub kind: InviteKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InviteKind {
    Party,
    Trade,
}

/// A group as its members see it (phase 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyView {
    pub leader: EntityId,
    /// Leader first, then by join order.
    pub members: Vec<EntityId>,
}

/// A trade as one side sees it (phase 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeView {
    pub with: EntityId,
    /// Both sides asked: offers may be set.
    pub open: bool,
    /// My offer: backpack slot and quantity.
    pub mine: Vec<(u16, u32)>,
    pub mine_coins: u64,
    /// What the other side offers.
    pub theirs: Vec<(ItemId, u32)>,
    /// Phase 10: beside each line of `theirs`, the piece when it is an
    /// item with identity (rarity, crafter, power: what it is worth).
    pub theirs_pieces: Vec<Option<InstanceView>>,
    pub theirs_coins: u64,
    pub mine_confirmed: bool,
    pub theirs_confirmed: bool,
}

/// The open shop as the client sees it (phase 7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopView {
    pub npc: EntityId,
    /// One line per stock entry, in file order.
    pub stock: Vec<ShopLineView>,
    /// What the shop pays for an item, in thousandths of its value.
    pub pays_permille: u32,
    /// Thousandths of the missing value charged for a repair (0: no repairs).
    pub repair_permille: u32,
    pub price_permille: u32,
    /// Thousandths of the value charged for an appraisal (0: none).
    pub appraise_permille: u32,
    /// Coins the shop still pays this player and seconds until it refills;
    /// `None`: no limit.
    pub purse: Option<(u64, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopLineView {
    pub item: ItemId,
    /// Pieces left; `None`: unlimited.
    pub qty: Option<u32>,
    pub price: u64,
}

impl TickOutput {
    pub fn clear(&mut self) {
        self.entered.clear();
        self.left.clear();
        self.views.clear();
        self.entities.clear();
        self.dialogue.clear();
        self.refused.clear();
        self.inventories.clear();
        self.quest_logs.clear();
        self.reloaded = false;
        self.map_changed = false;
        self.tiles.clear();
        self.system.clear();
        self.notes.clear();
        self.markers.clear();
        self.combat.clear();
        self.supports.clear();
        self.stats.clear();
        self.dead.clear();
        self.need_item_ids = false;
        self.shops.clear();
        self.shop_npcs.clear();
        self.coins.clear();
        self.parties.clear();
        self.trades.clear();
        self.invites.clear();
        self.emotes.clear();
        self.handoffs.clear();
        self.border.clear();
        self.escrows.clear();
        self.escrow_lost.clear();
        self.delivered.clear();
        self.feats.clear();
        self.claims.clear();
        self.clears.clear();
        self.released.clear();
    }
}

/// Result of `Talk`/`ChooseOption`, translated by the binary into frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogueOut {
    /// Current node of the dialogue; the player may answer with an option index.
    Show {
        player: PlayerId,
        npc: EntityId,
        text: TextId,
        options: Vec<TextId>,
    },
    /// The dialogue is over.
    End { player: PlayerId, npc: EntityId },
    /// `Talk` or `ChooseOption` that cannot be done.
    Refused { player: PlayerId, reason: Refusal },
}

/// Why a command was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The NPC is farther than `TALK_RADIUS`.
    TooFar,
    /// No such entity, no dialogue on it, no dialogue open, or an option
    /// that does not exist.
    NoDialogue,
    /// An effect gives items and the backpack has no room.
    InventoryFull,
    /// The option's condition no longer holds, an effect takes what the
    /// player lacks, a quest cannot start or complete.
    RequirementsNotMet,
    /// `MoveItem` with an empty, unknown or out of range slot.
    InvalidSlot,
    /// Target missing, dead, out of reach, out of sight, or in a safe region.
    InvalidTarget,
    /// Technique not learned, on cooldown, or not enough vigor.
    NotReady,
    /// Fallen players cannot act.
    Fallen,
    /// Phase 7: the entity keeps no shop, or no shop is open.
    NoShop,
    /// Not enough coins.
    NotEnoughCoins,
    /// The shop ran out of that line.
    OutOfStock,
    /// The shop does not trade that item, or it needs no repair.
    NotTraded,
    /// Phase 8: recipe unknown or materials missing.
    CannotCraft,
    /// The node is out of reach or still empty for this character.
    NothingToGather,
    /// Phase 9: no group, no invite to answer, or not the leader.
    NoParty,
    PartyFull,
    /// No open trade, or the request is not answered yet.
    NoTrade,
    /// The offers changed or no longer hold: nothing was swapped.
    TradeChanged,
    /// Phase 11: no tile of this zone to switch there.
    NothingToUse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatOut {
    pub attacker: EntityId,
    pub target: EntityId,
    /// `None`: missed.
    pub damage: Option<u32>,
    pub crit: bool,
    pub killed: bool,
    /// The technique of the blow (`None`: automatic attack).
    pub technique: Option<base::TechniqueId>,
}

/// The player's own sheet, as the status window shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsView {
    pub level: u8,
    pub xp: u64,
    pub xp_next: u64,
    pub hp: u32,
    pub max_hp: u32,
    pub vigor: u32,
    pub max_vigor: u32,
    pub mp: u32,
    pub max_mp: u32,
    /// With equipment bonuses.
    pub attributes: Attributes,
    /// The sheet's alone.
    pub base_attributes: Attributes,
    /// A production class: appraises by itself.
    pub appraises: bool,
    pub free_points: u16,
    pub class: Option<ClassId>,
    pub classes_available: Vec<ClassId>,
    pub proficiencies: Vec<(ProficiencyId, u8)>,
    pub techniques: Vec<(TechniqueId, u8)>,
    pub titles: Vec<TitleId>,
    pub equipment: Vec<(EquipSlot, ItemId)>,
    pub defense: u32,
    pub fallen: bool,
    /// Phase 5: ticks until each technique is ready again (only those
    /// resting) and until the global cooldown ends.
    pub cooldowns: Vec<(TechniqueId, u32)>,
    pub global_cooldown: u32,
    /// Phase 8: recipes this character knows.
    pub recipes: Vec<RecipeId>,
    /// Special attributes above 0, by name.
    pub specials: Vec<(TextId, u8)>,
}

/// Backpack as the client sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryView {
    pub coins: u64,
    pub slots: Vec<Option<(ItemId, u32)>>,
    /// Backpack slots holding items with identity: id and, once appraised,
    /// the hidden bonuses.
    pub instances: Vec<InstanceView>,
    /// Phase 10: the worn pieces (slot 0 weapon, 1 armor).
    pub equipped: Vec<InstanceView>,
}

/// A backpack slot holding an item with identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceView {
    pub slot: u16,
    pub id: u64,
    /// Hidden bonuses, once appraised.
    pub hidden: Option<Vec<(Attribute, u16)>>,
    /// Durability left and the maximum of this piece (phase 7).
    pub durability: (u32, u32),
    /// Who crafted it (phase 8).
    pub crafter: Option<CharacterName>,
    pub rarity: Rarity,
    /// Phase 10: damage or defense added by the rarity, in
    /// thousandths; the crafter's mastery; the power it carries.
    pub power_permille: u32,
    pub mastery: u16,
    pub power: Option<base::ItemPower>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestView {
    pub name: TextId,
    pub description: TextId,
    pub completed: bool,
    pub objectives: Vec<ObjectiveView>,
    /// Where to go next (global tile), if the world has a place.
    pub goal: Option<base::TilePos>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectiveView {
    Item {
        item: ItemId,
        have: u32,
        need: u32,
    },
    /// `npc`: the NPC's name.
    Talk {
        npc: TextId,
        done: bool,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub entered: Vec<EntityId>,
    pub left: Vec<EntityId>,
    pub moved: Vec<(EntityId, TilePos, Direction)>,
    /// Known entities whose state changed (HP, equipment): full state in
    /// `TickOutput::entities`.
    pub updated: Vec<EntityId>,
}

impl View {
    pub fn is_empty(&self) -> bool {
        self.entered.is_empty() && self.left.is_empty() && self.moved.is_empty() && self.updated.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Player,
    Npc,
    Monster,
    /// Phase 8: a resource node.
    Node,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityLabel {
    Player(CharacterName),
    Npc(TextId),
    Monster(TextId),
    Node(TextId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityState {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: TilePos,
    pub direction: Direction,
    pub appearance: u16,
    pub label: EntityLabel,
    /// HP in hundredths of the maximum (100 for entities without HP).
    pub health: u8,
    /// Visual layers of weapon, armor and off hand (0: none).
    pub equipment: (u16, u16, u16),
    /// Phase 11: the equipped item whose light is drawn around
    /// the entity, as `ItemId` + 1 (0: none).
    pub light: u32,
    pub level: u8,
    /// Players: positions in the creation lists.
    pub look: base::Look,
}

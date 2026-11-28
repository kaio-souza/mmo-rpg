use crate::ChunkTiles;
use crate::account::{CharacterSummary, DeviceToken, Password, Preference, SessionToken};
use base::{
    AccountName, Arg, Attribute, BoundedString, CharacterId, CharacterName, ChatText, ClassId, Direction,
    EmoteId, EntityId, ItemId, ItemKind, Locale, Look, ProficiencyId, Rarity, RecipeId, TechniqueId, TextId,
    TextRef, TilePos, TitleId,
};
use serde::{Deserialize, Serialize};

/// Client intentions. Never results: `Move { destination }`, not "I am at".
///
/// Variant order is part of the wire format: append only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// First message. Unknown locale codes become `en`.
    Hello {
        version: u16,
        #[serde(with = "crate::wire_locale")]
        locale: Locale,
    },
    /// Only in `--dev` mode and from localhost.
    DevLogin {
        name: CharacterName,
    },
    /// The server finds the path and walks tile by tile; replaces the previous move.
    Move {
        destination: TilePos,
    },
    Stop,
    Ping {
        n: u32,
    },
    /// A chunk the client lost; ignored outside the view radius.
    RequestChunk {
        cx: i32,
        cy: i32,
    },
    /// Creates the account and opens a session (phase 2). Protocol 24
    /// `proof` answers the connection's last `Challenge`.
    Register {
        account: AccountName,
        password: Password,
        proof: u64,
    },
    /// Protocol 24: `device` is the token this device got from
    /// an earlier `Session` of the account; `proof` answers the last
    /// `Challenge`.
    Login {
        account: AccountName,
        password: Password,
        device: Option<DeviceToken>,
        proof: u64,
    },
    /// Opens a session without the password; the token is used up.
    Resume {
        token: SessionToken,
    },
    /// Name and look chosen on the creation screen (protocol 6).
    CreateCharacter {
        name: CharacterName,
        look: Look,
    },
    Enter {
        character: CharacterId,
    },
    SetPreferences(Preference),
    /// Changes the password (phase 2); answers with a new `Session`.
    ChangePassword {
        old: Password,
        new: Password,
    },
    /// Opens the dialogue of a nearby NPC (phase 3).
    Talk {
        target: EntityId,
    },
    /// Picks an option of the open dialogue; `option` is its position.
    ChooseOption {
        option: u16,
    },
    /// Asks for the text pack of a locale (phase 3; the client caches it).
    RequestTextPack {
        #[serde(with = "crate::wire_locale")]
        locale: Locale,
    },
    /// Merges or swaps two backpack slots (phase 3).
    MoveItem {
        from: u16,
        to: u16,
    },
    /// Phase 4 (protocol 5): selects a target for the weapon.
    Attack {
        target: EntityId,
    },
    UseTechnique {
        technique: TechniqueId,
        target: EntityId,
    },
    /// Free points per attribute, in `Attribute::ALL` order (permanent).
    AllocatePoints {
        points: [u16; 6],
    },
    ChooseClass {
        class: ClassId,
    },
    /// Equips the item in a backpack slot.
    Equip {
        slot: u16,
    },
    Unequip {
        slot: EquipSlot,
    },
    Appraise {
        slot: u16,
    },
    /// Fallen: stop waiting for help.
    GiveUp,
    /// Phase 5 (protocol 6): a chat line. Muted accounts get `Muted`.
    Chat {
        channel: ChatChannel,
        text: ChatText,
    },
    /// An admin command typed in the chat; other roles get `NotAllowed`.
    Admin(AdminCommand),
    /// Reports a player: the server keeps the last chat lines the reporter
    /// could see (`admin reports`).
    ReportChat {
        name: CharacterName,
    },
    /// Phase 5, part D (protocol 7): uses one item of a backpack stack (a
    /// consumable with `use`).
    UseItem {
        slot: u16,
    },
    /// Closes the open dialogue (Esc); the server answers `DialogueEnd`.
    CloseDialogue,
    /// Phase 7 (protocol 9): opens the shop of a nearby NPC; the server
    /// answers `Shop`.
    OpenShop {
        target: EntityId,
    },
    CloseShop,
    /// Buys `qty` pieces of the shop line at `index` in `ShopView::stock`.
    Buy {
        index: u16,
        qty: u32,
    },
    /// Sells `qty` of a backpack slot (equipment: always one piece).
    Sell {
        slot: u16,
        qty: u32,
    },
    /// Repairs the equipment of a backpack slot at the open shop.
    Repair {
        slot: u16,
    },
    /// Phase 8 (protocol 9): crafts one batch of a known recipe.
    Craft {
        recipe: RecipeId,
    },
    /// Gathers a resource node within reach.
    Gather {
        target: EntityId,
    },
    /// Phase 9 (protocol 10): invites a player to the group.
    PartyInvite {
        target: EntityId,
    },
    /// Answers the latest group invite.
    AnswerInvite {
        accept: bool,
    },
    LeaveParty,
    /// Leader only.
    PartyKick {
        target: EntityId,
    },
    /// Leader only: hands the lead over.
    PartyLead {
        target: EntityId,
    },
    /// Asks a nearby player to trade; the same request from the other side
    /// opens the trade.
    TradeRequest {
        target: EntityId,
    },
    /// What this side offers: `(backpack slot, quantity)` and copper. Any
    /// change takes both confirmations back.
    TradeOffer {
        items: Vec<(u16, u32)>,
        coins: u64,
    },
    TradeConfirm,
    TradeCancel,
    /// Phase 9 (protocol 11): a gesture over the character.
    Emote {
        emote: EmoteId,
    },
    /// Protocol 15: uses one item of a backpack stack on another
    /// player, a revival item on a fallen one nearby.
    UseItemOn {
        slot: u16,
        target: EntityId,
    },
    /// Protocol 17: the open shop appraises the piece of a backpack slot,
    /// for coins (`ShopView::appraise_permille`).
    ShopAppraise {
        slot: u16,
    },
    /// Protocol 18: a production class mends the piece of a
    /// backpack slot with the main material of its recipe, anywhere.
    SelfRepair {
        slot: u16,
    },
    /// Protocol 19: deletes a character of the account from the
    /// selection screen. `name` is typed again by the player to confirm;
    /// answers with `Characters`.
    DeleteCharacter {
        character: CharacterId,
        name: CharacterName,
    },
    /// Protocol 21: switches a tile within reach whose word has
    /// `TILE_SWITCH` (lights or puts out a brazier). Refused with
    /// `NothingToUse` or `TooFar`; the new tile comes as a `Chunk`.
    UseTile {
        pos: TilePos,
    },
    /// Protocol 28: `/ranking [id]`. Without an id, the list of the
    /// game's rankings; with one, its lines. Answered with System lines.
    Ranking {
        id: Option<RankingId>,
    },
    /// Protocol 28: `/firsts`, the server firsts so far (System lines).
    ServerFirsts,
    /// Protocol 31: `/goals`, the server goals and how far the server got
    /// (System lines).
    ServerGoals,
}

/// Id of a ranking of `content/rankings/` (`hunters`).
pub type RankingId = BoundedString<32>;

/// Where a chat line goes (phase 5). Party, clan, trade and global channels
/// arrive with their systems (append only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatChannel {
    /// Players within the view radius.
    Local,
    /// One player, by character name (`/w NAME`).
    Whisper(CharacterName),
}

/// Short reason id of `content/moderation/reasons.ron` (`harassment` for
/// `moderation.reason.harassment`).
pub type ReasonId = BoundedString<32>;

/// In-game admin commands. Typed fields:
/// the client parses the chat text, the server checks the role first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminCommand {
    /// Drops the character, no sanction.
    Kick {
        name: CharacterName,
    },
    Mute {
        name: CharacterName,
        secs: u32,
        reason: ReasonId,
    },
    Suspend {
        name: CharacterName,
        secs: u32,
        reason: ReasonId,
    },
    Ban {
        name: CharacterName,
        reason: ReasonId,
    },
    /// The admin's zone writes its replay log to disk.
    Replay,
    /// Account, zone and position of a character.
    Who {
        name: CharacterName,
    },
    /// Protocol 19: announced restart in `secs` seconds; 0
    /// cancels the one scheduled.
    Maintenance {
        secs: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EquipSlot {
    Weapon,
    Armor,
    /// Protocol 22: the other hand (a torch).
    Offhand,
}

/// State and answers from the server. Variant order is part of the wire
/// format: append only (`encode_diff` relies on `Diff`'s index).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        your_id: EntityId,
        pos: TilePos,
        tick: u32,
        content_hash: u64,
        /// Protocol 18: tiles the player sees (with line of
        /// sight); the client shades what lies beyond.
        view_radius: u16,
    },
    Chunk {
        cx: i32,
        cy: i32,
        tiles: ChunkTiles,
    },
    Snapshot {
        entities: Vec<Entity>,
    },
    Diff(Diff),
    Pong {
        n: u32,
    },
    /// No text: the client shows `error.<code>`.
    Error {
        code: ErrorCode,
        args: Vec<Arg>,
    },
    /// Answer to `Register`, `Login` and `Resume`. Protocol 24:
    /// a new `device` token after a password (the client keeps it), and
    /// the wrong passwords typed for the account since its last login.
    Session {
        token: SessionToken,
        characters: Vec<CharacterSummary>,
        device: Option<DeviceToken>,
        failed_logins: u32,
    },
    /// Answer to `CreateCharacter`.
    Characters {
        characters: Vec<CharacterSummary>,
    },
    /// Place in the entry queue; `estimate_secs` 0 means no estimate yet.
    QueuePosition {
        position: u32,
        estimate_secs: u32,
    },
    /// One dialogue node: text and the options the player may pick, in
    /// order (`ChooseOption { option }` is the position). At most
    /// `MAX_DIALOGUE_OPTIONS` options.
    Dialogue {
        npc: EntityId,
        text: TextRef,
        options: Vec<TextRef>,
    },
    /// The dialogue closed (last option, or the player walked away).
    DialogueEnd {
        npc: EntityId,
    },
    /// Compressed text pack of one locale, indexed by `TextId` (phase 3),
    /// cut in `parts` parts of at most `TEXT_PACK_PART` bytes, sent in order.
    TextPack {
        #[serde(with = "crate::wire_locale")]
        locale: Locale,
        content_hash: u64,
        part: u16,
        parts: u16,
        data: Vec<u8>,
    },
    /// Content reloaded: the client uses its cached pack for this hash or
    /// asks for it again.
    TextPackChanged {
        content_hash: u64,
    },
    /// What the client shows about content; `ItemId` is the position in
    /// `items`.
    Definitions {
        content_hash: u64,
        items: Vec<ItemInfo>,
        /// Indexed by `ClassId`, `TechniqueId`, `ProficiencyId`, `TitleId`.
        classes: Vec<NamedInfo>,
        techniques: Vec<TechniqueInfo>,
        proficiencies: Vec<NamedInfo>,
        titles: Vec<NamedInfo>,
        /// Protocol 8: sprite names (`a-z0-9_`); entity `appearance` and
        /// equipment visuals are the position + 1, 0 is none
        sprites: Vec<String>,
        /// Protocol 8: icon names; `icon` fields are the position + 1, 0 is
        /// the generic icon of the kind.
        icons: Vec<String>,
        /// Protocol 9: indexed by `RecipeId`.
        recipes: Vec<RecipeInfo>,
        /// Protocol 11: emote names, indexed by `EmoteId`.
        emotes: Vec<TextId>,
        /// Protocol 17: what each title gives, indexed by `TitleId`.
        title_bonuses: Vec<Vec<TitleBonusInfo>>,
        /// Protocol 20: the picture of each emote (icon number, as in
        /// `icons`; 0: none, the name is shown), indexed by `EmoteId`.
        emote_icons: Vec<u16>,
        /// Protocol 23: every zone of the world.
        zones: Vec<ZoneInfo>,
    },
    /// The whole backpack (`BACKPACK_SLOTS` slots) and the coins, in copper.
    Inventory {
        coins: u64,
        slots: Vec<Option<ItemSlot>>,
        /// Slots with items with identity: `(slot, instance id, hidden
        /// bonuses once appraised)`.
        instances: Vec<InstanceInfo>,
        /// Protocol 14: the worn pieces, `slot` 0 weapon and 1 armor.
        equipped: Vec<InstanceInfo>,
    },
    /// Active and completed quests.
    QuestLog {
        quests: Vec<QuestEntry>,
    },
    /// A System message (level up, loot, title...), translated by the
    /// client. `window: false` (protocol 7): routine, only the log.
    System {
        text: TextRef,
        window: bool,
    },
    /// Blows seen this tick.
    Combat {
        events: Vec<CombatEvent>,
    },
    /// The player's own status window.
    Stats(Box<StatsInfo>),
    /// Phase 5 (protocol 6): a chat line. A whisper comes back to its
    /// sender too, with the same `channel` (the receiver) and `from`.
    ChatMessage {
        channel: ChatChannel,
        from: CharacterName,
        text: ChatText,
    },
    /// Character creation lists, sent with `Session` (before the text pack
    /// is needed in the game, the client asks for it in the selection).
    Appearances(AppearanceInfo),
    /// Phase 5, part D (protocol 7): what the player has to do with each NPC of
    /// the zone (NPCs without a marker are left out). Sent on entering and
    /// whenever the quest log changes; replaces the previous list.
    NpcMarkers {
        markers: Vec<(EntityId, QuestMarker)>,
    },
    /// Phase 7 (protocol 9): which NPCs of the zone keep a shop, so the
    /// client can mark them. Sent on entering and after a content reload.
    Shops {
        npcs: Vec<EntityId>,
    },
    /// The open shop. Replaces the previous one.
    Shop(Box<ShopView>),
    /// The shop closed (the player walked away or asked to close).
    ShopEnd,
    /// Phase 9 (protocol 10): someone invited you to a group or asked to
    /// trade.
    Invite {
        from: EntityId,
        kind: InviteKind,
    },
    /// The player's group. Replaces the previous one.
    Party(Box<PartyView>),
    /// The player is in no group (any more).
    PartyEnd,
    /// The player's trade. Replaces the previous one.
    Trade(Box<TradeView>),
    /// The trade ended: done, cancelled or expired (the System says which).
    TradeEnd,
    /// Someone in view (or this player) made a gesture.
    Emote {
        entity: EntityId,
        emote: EmoteId,
    },
    /// Phase 10: the world's clock, sent on entering and every minute.
    /// The client counts on from `secs_of_day` (0: midnight) on its own.
    WorldTime {
        secs_of_day: u32,
        day_secs: u32,
    },
    /// Protocol 19: the server restarts in `in_secs` seconds
    /// (sent at 10 min, 5 min, 1 min and 10 s, and when scheduled). A client
    /// that loses the connection after this waits for the server to come back.
    Maintenance {
        in_secs: u32,
    },
    /// The announced restart was called off.
    MaintenanceCancelled,
    /// Protocol 23: support techniques used (heal, buff,
    /// revive), which land no blow, for whoever sees the user or the
    /// target: their sound plays where the user stands.
    TechniqueUsed {
        events: Vec<TechniqueUse>,
    },
    /// Protocol 24: after `Hello` and after each `Login` or
    /// `Register`, the seed the next one's `proof` must use, and how many
    /// leading zero bits its hash needs (0 while the server is calm).
    Challenge {
        seed: [u8; 16],
        bits: u8,
    },
}

/// One support technique used (protocol 23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechniqueUse {
    pub user: EntityId,
    pub target: EntityId,
    pub technique: TechniqueId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InviteKind {
    Party,
    Trade,
}

/// A group as its members see it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyView {
    pub leader: EntityId,
    /// Leader first, then by join order.
    pub members: Vec<EntityId>,
}

/// A trade as one side sees it.
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
    /// Protocol 14: beside each line of `theirs`, the piece when it is an
    /// item with identity.
    pub theirs_pieces: Vec<Option<InstanceInfo>>,
    pub theirs_coins: u64,
    pub mine_confirmed: bool,
    pub theirs_confirmed: bool,
}

/// What the player sees of an NPC shop (protocol 9). Prices are already
/// in copper, with the city's variation applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopView {
    pub npc: EntityId,
    pub stock: Vec<ShopLine>,
    /// What the shop pays for the player's items, in thousandths of their
    /// value (0: it buys nothing).
    pub pays_permille: u32,
    /// Thousandths of the missing value charged for a repair (0: no repairs).
    pub repair_permille: u32,
    /// The city's price variation, applied to both (thousandths).
    pub price_permille: u32,
    /// Protocol 17: thousandths of the value charged to appraise a piece
    /// (0: no appraisals).
    pub appraise_permille: u32,
    /// Coins the shop still pays this player before it refills, and the
    /// seconds until it does; `None`: no limit.
    pub purse: Option<(u64, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopLine {
    pub item: ItemId,
    /// Pieces left; `None`: unlimited.
    pub qty: Option<u32>,
    /// Copper for one piece.
    pub price: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestMarker {
    /// A quest to start (`!`).
    Available,
    /// A `Talk` objective of an active quest.
    Talk,
    /// A quest to hand in (`?`).
    Ready,
}

/// One option of a creation list: name, tone (`0xRRGGBB`) and, for
/// bodies and hairstyles, the sprites of its layers by name (`a-z0-9_`;
/// empty: none). Names, not sprite numbers: the creation screen draws them
/// before `Definitions` (which numbers the sprites) arrives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookOptionInfo {
    pub name: TextId,
    pub color: u32,
    /// The layer, tinted by `color` of the skin (body) or hair color (hairstyle).
    pub sprite: String,
    /// Bodies: clothes over the body, not tinted.
    pub clothes: String,
}

/// The four lists in `Look` field order: body, skin, hairstyle, hair color.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceInfo {
    pub lists: [Vec<LookOptionInfo>; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatEvent {
    pub attacker: EntityId,
    pub target: EntityId,
    /// `None`: missed.
    pub damage: Option<u32>,
    pub crit: bool,
    pub killed: bool,
    /// Protocol 23: the technique of the blow; `None` for the
    /// automatic attack.
    pub technique: Option<TechniqueId>,
}

/// Protocol 23: a zone of the world, for the client's zone music.
/// Corners inclusive, global tiles; zones never overlap.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneInfo {
    /// Folder name of the zone (`aurelia`).
    pub name: String,
    pub min: TilePos,
    pub max: TilePos,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsInfo {
    pub level: u8,
    pub xp: u64,
    pub xp_next: u64,
    pub hp: u32,
    pub max_hp: u32,
    pub vigor: u32,
    pub max_vigor: u32,
    /// Protocol 13.
    pub mp: u32,
    pub max_mp: u32,
    /// In `Attribute::ALL` order, with equipment bonuses.
    pub attributes: [u16; 6],
    pub free_points: u16,
    pub class: Option<ClassId>,
    pub classes_available: Vec<ClassId>,
    pub proficiencies: Vec<(ProficiencyId, u8)>,
    pub techniques: Vec<(TechniqueId, u8)>,
    pub titles: Vec<TitleId>,
    pub equipment: Vec<(EquipSlot, ItemId)>,
    pub defense: u32,
    pub fallen: bool,
    /// Protocol 7: ticks until each resting technique is ready, and until
    /// the global cooldown ends, when this was sent.
    pub cooldowns: Vec<(TechniqueId, u32)>,
    pub global_cooldown: u32,
    /// Protocol 30: ticks until each resting consumable may be used again.
    pub item_cooldowns: Vec<(ItemId, u32)>,
    /// Protocol 9: recipes this character knows (phase 8).
    pub recipes: Vec<RecipeId>,
    /// Protocol 17: the attributes of the sheet alone, without equipment
    /// and titles, in `Attribute::ALL` order.
    pub base_attributes: [u16; 6],
    /// The class can appraise pieces by itself (a production class).
    pub appraises: bool,
    /// Protocol 18: special attributes above 0, by name.
    pub specials: Vec<(TextId, u8)>,
    /// Protocol 18: the class mends its own pieces with materials.
    pub self_repairs: bool,
}

/// A recipe as the crafting window shows it (protocol 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeInfo {
    pub name: TextId,
    pub description: TextId,
    pub proficiency: ProficiencyId,
    pub min_points: u8,
    /// `(item, pieces)`.
    pub materials: Vec<(ItemId, u32)>,
    pub result: ItemId,
    pub qty: u32,
}

/// A technique as the tooltip shows it (protocol 7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechniqueInfo {
    pub name: TextId,
    pub description: TextId,
    pub vigor: u32,
    /// Protocol 13: MP spent.
    pub mp: u32,
    pub cooldown_ticks: u32,
    pub range: u16,
    /// Damage multiplier at level 1 and added per level, in thousandths.
    pub damage_permille: u32,
    pub growth_permille: u32,
    /// Protocol 8: position + 1 in `Definitions::icons`; 0: generic.
    pub icon: u16,
    /// Protocol 12: proficiency of the weapon it needs in hand (`None`:
    /// empty hands). Same value as `ItemInfo::weapon` of that weapon.
    pub proficiency: Option<ProficiencyId>,
    /// Protocol 15: magic, it needs no weapon in hand.
    pub any_weapon: bool,
    /// Protocol 15: support, heals a standing player (thousandths of their
    /// HP at level 1) or stands a fallen one up; 0 when it does not.
    pub heal_permille: u32,
    pub revive_permille: u32,
    /// Protocol 18: a buff on the caster: defense raised by
    /// these thousandths at level 1 (0: not a buff), for these ticks.
    pub buff_permille: u32,
    pub buff_ticks: u32,
    /// Protocol 23: content id (`cleave`), for its sound.
    pub id: String,
}

/// What using one item of a consumable stack does (protocol 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UseInfo {
    pub hp: u32,
    pub vigor: u32,
    pub cooldown_ticks: u32,
    /// Protocol 15: a revival item, used on a fallen player nearby, who
    /// stands up with these thousandths of their HP (0: not one).
    pub revive_permille: u16,
    /// Protocol 25: MP restored (the mana tonic).
    pub mp: u32,
}

/// One bonus of a title, for its tooltip (protocol 17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitleBonusInfo {
    Attribute(base::Attribute, u16),
    MaxHp(u32),
    MaxVigor(u32),
    /// Percent more damage; with a level, only up to that level.
    DamagePercent(u16, Option<u8>),
    /// Percent more XP from monsters; with a level, only up to that level.
    XpPercent(u16, Option<u8>),
    /// Points of a special attribute, by its name.
    Special(TextId, u8),
    /// Protocol 25: percent more maximum MP; with a level, only up to it.
    MaxMpPercent(u16, Option<u8>),
}

/// Name and description of a class, technique, proficiency or title.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedInfo {
    pub name: TextId,
    pub description: Option<TextId>,
    /// Protocol 8: position + 1 in `Definitions::icons`; 0: generic.
    pub icon: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemInfo {
    pub name: TextId,
    pub description: TextId,
    pub kind: ItemKind,
    pub rarity: Rarity,
    pub max_stack: u16,
    /// Grams.
    pub weight: u32,
    /// Protocol 7: consumables that can be used.
    pub use_effect: Option<UseInfo>,
    /// Protocol 9: copper the item is worth (0: no NPC trades it).
    pub value: u64,
    /// Protocol 9: equipment only, blows it takes when new (0: no wear).
    pub durability: u32,
    /// Protocol 8: position + 1 in `Definitions::icons`; 0: generic.
    pub icon: u16,
    /// Protocol 12: weapons only, the proficiency it trains and that its
    /// techniques need.
    pub weapon: Option<ProficiencyId>,
    /// Protocol 22: light around the wearer while equipped.
    pub light: Option<LightInfo>,
    /// Protocol 22: durability lost per minute equipped (0: none).
    pub burn: u32,
    /// Protocol 30: what the piece does when worn (equipment only).
    pub equip: Option<EquipInfo>,
}

/// Protocol 30: an equipment piece's numbers, for its tooltip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipInfo {
    /// Weapons: damage range and ticks between blows; 0 for the rest.
    pub damage: (u32, u32),
    pub attack_ticks: u32,
    /// Weapons: tiles a blow reaches.
    pub range: u16,
    /// Armor and off-hand: defense it adds.
    pub defense: u32,
    /// Weapons: attribute that adds to the damage (index in `Attribute::ALL`).
    pub attribute: u8,
}

/// Protocol 22: an item's light; `radius` in tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightInfo {
    pub radius: u8,
    pub color: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSlot {
    pub item: ItemId,
    pub qty: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestState {
    Active,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestEntry {
    pub name: TextId,
    pub description: TextId,
    pub state: QuestState,
    pub objectives: Vec<ObjectiveView>,
    /// Protocol 18: the tile to head for next, if the world
    /// has a place for it.
    pub goal: Option<TilePos>,
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
    /// Protocol 29: defeat monsters; `monster`: its name.
    Kill {
        monster: TextId,
        have: u32,
        need: u32,
    },
    /// Protocol 29: craft items.
    Craft {
        item: ItemId,
        have: u32,
        need: u32,
    },
    /// Protocol 29: reach a place (global tile).
    Reach {
        pos: base::TilePos,
        done: bool,
    },
}

pub(crate) const SNAPSHOT_VARIANT: u32 = 2;
pub(crate) const DIFF_VARIANT: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    pub tick: u32,
    pub entered: Vec<Entity>,
    pub left: Vec<EntityId>,
    pub moved: Vec<(EntityId, TilePos, Direction)>,
    /// Known entities whose HP or look changed: their full state again.
    pub updated: Vec<Entity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: TilePos,
    pub direction: Direction,
    pub appearance: u16,
    pub name: EntityName,
    /// HP in hundredths of the maximum.
    pub health: u8,
    /// Visual layers of weapon, armor and (protocol 22) off hand (0: none).
    pub equipment: (u16, u16, u16),
    /// Protocol 22: the equipped item whose light shines around the
    /// entity, `ItemId` + 1 (its `ItemInfo::light`); 0: none.
    pub light: u32,
    pub level: u8,
    /// Players: the look chosen at creation (protocol 6); others: default.
    pub look: Look,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Player,
    Npc,
    Monster,
    /// Phase 8: a resource node, gathered with `Gather`.
    Node,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityName {
    Player(CharacterName),
    /// Translated on the client with the text pack.
    Npc(TextId),
    Monster(TextId),
    Node(TextId),
}

/// Variant order is part of the wire format: append only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    IncompatibleVersion,
    NotAllowed,
    InvalidName,
    NameTaken,
    TooFast,
    InvalidMessage,
    ServerFull,
    ZoneBusy,
    ZoneFailure,
    AccountTaken,
    WrongCredentials,
    /// `args`: seconds until the next try.
    TooManyAttempts,
    SessionExpired,
    TooManyCharacters,
    AlreadyOnline,
    NoSuchCharacter,
    /// The server failed (database); the client may try again later.
    Internal,
    /// `args`: seconds left and the reason (`Arg::Ref`).
    AccountSuspended,
    /// `args`: the reason (`Arg::Ref`).
    AccountBanned,
    /// `args`: seconds left.
    IpBlocked,
    /// `CreateCharacter` or `Enter` with a temporary password.
    PasswordChangeRequired,
    /// `Talk` with an NPC farther than the talk radius.
    TooFar,
    /// `Talk` with an entity that has no dialogue, or `ChooseOption` with no
    /// dialogue open or an option that does not exist.
    NoDialogue,
    /// An option gives items and the backpack has no room; nothing changed.
    InventoryFull,
    /// An option takes what the player lacks, a quest cannot start or
    /// complete, or a script failed; nothing changed.
    RequirementsNotMet,
    /// `MoveItem` with an empty or out of range slot.
    InvalidSlot,
    /// Attack or technique on a missing, dead or protected target.
    InvalidTarget,
    /// Technique not learned, on cooldown, or not enough vigor.
    NotReady,
    /// Fallen characters cannot act.
    Fallen,
    /// The character died and rests; `args`: hours left.
    CharacterResting,
    /// A chat line from a muted account; `args`: seconds left.
    Muted,
    /// Whisper, report or admin command naming a character that is not
    /// online (or does not exist).
    PlayerOffline,
    /// Phase 7: `OpenShop` on an entity with no shop, or buying, selling
    /// or repairing with no shop open.
    NoShop,
    /// Not enough coins for the purchase or the repair.
    NotEnoughCoins,
    /// The shop ran out of that line, or the player asked for more than it
    /// has.
    OutOfStock,
    /// The shop does not trade that item, or the item needs no repair.
    NotTraded,
    /// Phase 8: recipe not known, proficiency too low, or missing
    /// materials.
    CannotCraft,
    /// The node is out of reach, or still empty for this character.
    NothingToGather,
    /// Phase 9: no group, no invite to answer, or not the leader.
    NoParty,
    /// The group already has six members.
    PartyFull,
    /// No open trade to offer in or confirm.
    NoTrade,
    /// The offers no longer hold or do not fit: nothing was swapped.
    TradeChanged,
    /// Protocol 21: no tile to switch there (`UseTile`).
    NothingToUse,
    /// Protocol 24: the new password contains the account name.
    PasswordHasName,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 44] = [
        ErrorCode::IncompatibleVersion,
        ErrorCode::NotAllowed,
        ErrorCode::InvalidName,
        ErrorCode::NameTaken,
        ErrorCode::TooFast,
        ErrorCode::InvalidMessage,
        ErrorCode::ServerFull,
        ErrorCode::ZoneBusy,
        ErrorCode::ZoneFailure,
        ErrorCode::AccountTaken,
        ErrorCode::WrongCredentials,
        ErrorCode::TooManyAttempts,
        ErrorCode::SessionExpired,
        ErrorCode::TooManyCharacters,
        ErrorCode::AlreadyOnline,
        ErrorCode::NoSuchCharacter,
        ErrorCode::Internal,
        ErrorCode::AccountSuspended,
        ErrorCode::AccountBanned,
        ErrorCode::IpBlocked,
        ErrorCode::PasswordChangeRequired,
        ErrorCode::TooFar,
        ErrorCode::NoDialogue,
        ErrorCode::InventoryFull,
        ErrorCode::RequirementsNotMet,
        ErrorCode::InvalidSlot,
        ErrorCode::InvalidTarget,
        ErrorCode::NotReady,
        ErrorCode::Fallen,
        ErrorCode::CharacterResting,
        ErrorCode::Muted,
        ErrorCode::PlayerOffline,
        ErrorCode::NoShop,
        ErrorCode::NotEnoughCoins,
        ErrorCode::OutOfStock,
        ErrorCode::NotTraded,
        ErrorCode::CannotCraft,
        ErrorCode::NothingToGather,
        ErrorCode::NoParty,
        ErrorCode::PartyFull,
        ErrorCode::NoTrade,
        ErrorCode::TradeChanged,
        ErrorCode::NothingToUse,
        ErrorCode::PasswordHasName,
    ];

    /// Text key the client translates: `error.<code>`.
    pub const fn text_key(self) -> &'static str {
        match self {
            ErrorCode::IncompatibleVersion => "error.incompatible_version",
            ErrorCode::NotAllowed => "error.not_allowed",
            ErrorCode::InvalidName => "error.invalid_name",
            ErrorCode::NameTaken => "error.name_taken",
            ErrorCode::TooFast => "error.too_fast",
            ErrorCode::InvalidMessage => "error.invalid_message",
            ErrorCode::ServerFull => "error.server_full",
            ErrorCode::ZoneBusy => "error.zone_busy",
            ErrorCode::ZoneFailure => "error.zone_failure",
            ErrorCode::AccountTaken => "error.account_taken",
            ErrorCode::WrongCredentials => "error.wrong_credentials",
            ErrorCode::TooManyAttempts => "error.too_many_attempts",
            ErrorCode::SessionExpired => "error.session_expired",
            ErrorCode::TooManyCharacters => "error.too_many_characters",
            ErrorCode::AlreadyOnline => "error.already_online",
            ErrorCode::NoSuchCharacter => "error.no_such_character",
            ErrorCode::Internal => "error.internal",
            ErrorCode::AccountSuspended => "error.account_suspended",
            ErrorCode::AccountBanned => "error.account_banned",
            ErrorCode::IpBlocked => "error.ip_blocked",
            ErrorCode::PasswordChangeRequired => "error.password_change_required",
            ErrorCode::TooFar => "error.too_far",
            ErrorCode::NoDialogue => "error.no_dialogue",
            ErrorCode::InventoryFull => "error.inventory_full",
            ErrorCode::RequirementsNotMet => "error.requirements_not_met",
            ErrorCode::InvalidSlot => "error.invalid_slot",
            ErrorCode::InvalidTarget => "error.invalid_target",
            ErrorCode::NotReady => "error.not_ready",
            ErrorCode::Fallen => "error.fallen",
            ErrorCode::CharacterResting => "error.character_resting",
            ErrorCode::Muted => "error.muted",
            ErrorCode::PlayerOffline => "error.player_offline",
            ErrorCode::NoShop => "error.no_shop",
            ErrorCode::NotEnoughCoins => "error.not_enough_coins",
            ErrorCode::OutOfStock => "error.out_of_stock",
            ErrorCode::NotTraded => "error.not_traded",
            ErrorCode::CannotCraft => "error.cannot_craft",
            ErrorCode::NothingToGather => "error.nothing_to_gather",
            ErrorCode::NoParty => "error.no_party",
            ErrorCode::PartyFull => "error.party_full",
            ErrorCode::NoTrade => "error.no_trade",
            ErrorCode::TradeChanged => "error.trade_changed",
            ErrorCode::NothingToUse => "error.nothing_to_use",
            ErrorCode::PasswordHasName => "error.password_has_name",
        }
    }
}

/// An item with identity in the backpack (protocol 9: durability, who
/// made it and the rarity it was rolled with).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub slot: u16,
    pub id: u64,
    /// Hidden bonuses, once appraised.
    pub hidden: Option<Vec<(Attribute, u16)>>,
    /// Durability left and the maximum of this piece (0: no wear).
    pub durability: (u32, u32),
    /// Who crafted it.
    pub crafter: Option<CharacterName>,
    /// Rarity of this piece: rolled at the craft, else the item's own.
    pub rarity: Rarity,
    /// Protocol 14: damage or defense the rarity adds, in
    /// thousandths; the crafter's mastery in hundredths of a percent of
    /// durability; the power it carries.
    pub power_permille: u32,
    pub mastery: u16,
    pub power: Option<base::ItemPower>,
}

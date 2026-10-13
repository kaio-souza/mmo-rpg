//! The in-game interface: status bars, target, hotbar, log and chat,
//! minimap, System windows, inventory, character sheet and quest log.

use crate::fonts::Role;
use crate::overlay::{self, FloatText, Overhead};
use crate::paint::Rect;
use crate::theme::Color;
use crate::ui::{DragSource, Frame, UiAction};
use base::Arg;

mod anim;
#[cfg(test)]
pub(crate) use anim::covers;
mod crafting;
mod social;
mod trade;
pub use social::{PartyMember, PartyModel, PlayerMenu, Support};
mod windows;
use windows::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    System,
    /// Quest progress: System color, without the System window.
    Quest,
    Chat,
    Combat,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub kind: LineKind,
    pub text: String,
}

/// One line of a trade offer: a backpack slot (mine) or a position in
/// the other side's offer (theirs), and the piece as the tooltip shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeLine {
    pub slot: u16,
    pub qty: u32,
    /// Mine: how many the backpack slot holds (the most that can go);
    /// theirs: 0.
    pub max: u32,
    pub name: String,
    pub icon: Option<[f32; 4]>,
    pub rarity: base::Rarity,
    pub details: Vec<String>,
}

/// An open trade (phase 9, window since).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeModel {
    pub with: String,
    /// Both sides asked: offers may be set. Else waiting for the other.
    pub open: bool,
    /// Not open yet and the other side asked: the window offers
    /// Accept and Decline instead of waiting.
    pub asked: bool,
    pub mine: Vec<TradeLine>,
    pub mine_coins: u64,
    pub theirs: Vec<TradeLine>,
    pub theirs_coins: u64,
    pub mine_confirmed: bool,
    pub theirs_confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetModel {
    pub name: String,
    pub level: u8,
    /// Hundredths of the maximum.
    pub health: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BagSlot {
    pub name: String,
    pub count: u32,
    /// Item with identity: can be equipped and appraised.
    pub unique: bool,
    /// Hidden bonuses once appraised, already written out.
    pub appraised: Option<String>,
    /// Phase 5: the tooltip under the name (description, what using it
    /// does) and whether a click uses it.
    pub description: String,
    pub details: Vec<String>,
    pub usable: bool,
    /// Phase 6b: the item's icon in the sprite sheet (`Painter::image`);
    /// `None`: the name is written instead.
    pub icon: Option<[f32; 4]>,
    /// Phase 7: durability left and the maximum of this piece; `None`:
    /// nothing that wears out.
    pub durability: Option<(u32, u32)>,
    /// Phase 7: the open shop deals in it, so a click sells one.
    pub can_sell: bool,
    /// Phase 7: the open shop repairs it and the piece is worn.
    pub can_repair: bool,
    /// Alpha: a piece not appraised yet, and the character appraises it
    /// alone (a production class) / the open shop appraises it.
    pub can_appraise: bool,
    pub can_shop_appraise: bool,
    /// Worn, and the character mends it alone with materials (a
    /// production class).
    pub can_self_repair: bool,
    /// Colors the name and the slot's edge.
    pub rarity: base::Rarity,
}

/// What a character wears in one slot (0 weapon, 1 armor, 2 off hand).
#[derive(Clone, Debug, PartialEq)]
pub struct EquippedModel {
    pub slot: u8,
    pub name: String,
    pub icon: Option<[f32; 4]>,
    /// Tooltip lines under the name.
    pub details: Vec<String>,
    pub rarity: base::Rarity,
    /// Durability left and the maximum; `None`: the piece does not wear.
    pub durability: Option<(u32, u32)>,
}

/// A volume the Options window sets: the whole, or one channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VolumeSlot {
    Master,
    Effects,
    Ambience,
    Music,
    Interface,
}

impl VolumeSlot {
    pub const ALL: [VolumeSlot; 5] = [
        VolumeSlot::Master,
        VolumeSlot::Effects,
        VolumeSlot::Ambience,
        VolumeSlot::Music,
        VolumeSlot::Interface,
    ];

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            VolumeSlot::Master => "ui.options.master",
            VolumeSlot::Effects => "ui.options.effects",
            VolumeSlot::Ambience => "ui.options.ambience",
            VolumeSlot::Music => "ui.options.music",
            VolumeSlot::Interface => "ui.options.interface",
        }
    }
}

/// The Sound part of the Options window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundModel {
    /// Per mille, in [`VolumeSlot::ALL`] order.
    pub volumes: Vec<(VolumeSlot, u16)>,
    pub muted: bool,
    /// False: no sound server; the window says so.
    pub live: bool,
}

/// The help window: what each key does and a few tips.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelpModel {
    /// `(what it does, key)`, already written out.
    pub keys: Vec<(String, String)>,
    /// Mouse and chat, one line each.
    pub tips: Vec<String>,
}

/// One line of the open shop (phase 7).
#[derive(Clone, Debug, PartialEq)]
pub struct ShopLineModel {
    pub name: String,
    pub icon: Option<[f32; 4]>,
    pub price: u64,
    /// Pieces left; `None`: unlimited.
    pub qty: Option<u32>,
    /// The player can pay for one.
    pub affordable: bool,
    pub tooltip: Vec<String>,
    pub rarity: base::Rarity,
}

/// The open shop (phase 7).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShopModel {
    pub title: String,
    pub lines: Vec<ShopLineModel>,
    pub repairs: bool,
    /// Alpha: the shop appraises pieces.
    pub appraises: bool,
    /// What the shop still pays the player, written out (`None`: no limit).
    pub purse: Option<String>,
    /// What the player holds, already written out (`You have 120 copper`).
    pub coins: String,
    pub close: String,
}

/// One recipe of the crafting window (phase 8).
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeModel {
    pub id: base::RecipeId,
    pub name: String,
    pub description: String,
    pub icon: Option<[f32; 4]>,
    /// `(material, have, need)`, already written out.
    pub materials: Vec<(String, u32, u32)>,
    /// Materials and proficiency are enough.
    pub can: bool,
    /// Why not, when `can` is false.
    pub blocked: String,
    /// The craft it belongs to (the proficiency's name): one tab each.
    pub group: String,
    /// What it makes, to group the list (weapons, armor...).
    pub kind: base::ItemKind,
    /// Pieces made per craft.
    pub qty: u32,
    pub rarity: base::Rarity,
}

/// A technique in the hotbar (phase 5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HotbarSlot {
    pub name: String,
    /// Tooltip lines under the name.
    pub tooltip: Vec<String>,
    /// Part of the cooldown still to run, 1 (just used) to 0 (ready).
    pub cooldown: f32,
    /// Seconds left, already written out (`None` when ready).
    pub cooldown_text: Option<String>,
    /// Enough vigor and a target: the slot is lit.
    pub usable: bool,
    /// Phase 6b: the technique's icon (`Painter::image`).
    pub icon: Option<[f32; 4]>,
    /// Consumables: how many are left in the backpack, in the corner.
    pub count: Option<String>,
}

/// A technique the character knows, in its window (K).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TechniqueModel {
    pub id: base::TechniqueId,
    pub name: String,
    pub level: u8,
    pub icon: Option<[f32; 4]>,
    pub tooltip: Vec<String>,
    /// Already in the hotbar.
    pub in_bar: bool,
}

/// Something sent to the server whose answer has not come back yet: the
/// slot it touched pulses meanwhile, so the click is seen at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub bag: Option<u16>,
    pub shop: Option<u16>,
    pub recipe: Option<base::RecipeId>,
}

/// What the log shows (phase 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogTab {
    #[default]
    All,
    Chat,
    Combat,
    System,
}

impl LogTab {
    const ALL: [LogTab; 4] = [LogTab::All, LogTab::Chat, LogTab::Combat, LogTab::System];

    fn key(self) -> &'static str {
        match self {
            LogTab::All => "ui.log.all",
            LogTab::Chat => "ui.log.chat",
            LogTab::Combat => "ui.log.combat",
            LogTab::System => "ui.log.system",
        }
    }

    /// Errors show in every tab: they answer what the player just did.
    pub fn shows(self, kind: LineKind) -> bool {
        match self {
            LogTab::All => true,
            LogTab::Chat => matches!(kind, LineKind::Chat | LineKind::Error),
            LogTab::Combat => matches!(kind, LineKind::Combat | LineKind::Error),
            LogTab::System => matches!(kind, LineKind::System | LineKind::Quest | LineKind::Error),
        }
    }
}

/// What a click on the backpack does while a shop is open (alpha: one
/// mode at a time, so a sale is never a repair by mistake).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShopMode {
    #[default]
    Sell,
    Repair,
    Appraise,
}

impl ShopMode {
    fn key(self) -> &'static str {
        match self {
            ShopMode::Sell => "ui.shop.mode_sell",
            ShopMode::Repair => "ui.shop.mode_repair",
            ShopMode::Appraise => "ui.shop.mode_appraise",
        }
    }

    fn hint_key(self) -> &'static str {
        match self {
            ShopMode::Sell => "ui.shop.hint_sell",
            ShopMode::Repair => "ui.shop.hint_repair",
            ShopMode::Appraise => "ui.shop.hint_appraise",
        }
    }

    fn click_key(self) -> &'static str {
        match self {
            ShopMode::Sell => "ui.item.click_sell",
            ShopMode::Repair => "ui.item.click_repair",
            ShopMode::Appraise => "ui.item.click_appraise",
        }
    }

    fn cannot_key(self) -> &'static str {
        match self {
            ShopMode::Sell => "ui.item.cannot_sell",
            ShopMode::Repair => "ui.item.cannot_repair",
            ShopMode::Appraise => "ui.item.cannot_appraise",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetModel {
    /// In `Attribute::ALL` order, with equipment and titles.
    pub attributes: [u16; 6],
    /// Alpha: the sheet's own, without equipment and titles.
    pub base_attributes: [u16; 6],
    /// Alpha: each title's name and tooltip lines (description, bonuses).
    pub title_tips: Vec<Vec<String>>,
    pub free_points: u16,
    pub class: Option<String>,
    pub classes_available: Vec<String>,
    pub titles: Vec<String>,
    pub proficiencies: Vec<(String, u8)>,
    pub defense: u32,
    /// Special attributes above 0 (faith), by name.
    pub specials: Vec<(String, u8)>,
}

/// One quest of the log, already written out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestModel {
    pub name: String,
    pub description: String,
    pub completed: bool,
    /// `(objective, done)`.
    pub objectives: Vec<(String, bool)>,
    /// Where to head next ("up-right, 40 tiles"), when known.
    pub goal: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dot {
    Me,
    Player,
    Npc,
    Monster,
    /// An NPC with a quest marker.
    Quest,
    /// Where an active quest leads; on the edge when farther.
    Goal,
}

/// Tiles around the player, row by row: 0 unknown, 1 floor, 2 wall.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Minimap {
    pub side: u16,
    pub tiles: Vec<u8>,
    /// Offsets from the center, in tiles.
    pub dots: Vec<(i16, i16, Dot)>,
}

/// A window the menu bar opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuWindow {
    Inventory,
    Sheet,
    Techniques,
    Quests,
    Crafting,
    Help,
    /// Sound now; keys later.
    Options,
}

impl MenuWindow {
    pub const ALL: [MenuWindow; 7] = [
        MenuWindow::Inventory,
        MenuWindow::Sheet,
        MenuWindow::Techniques,
        MenuWindow::Quests,
        MenuWindow::Crafting,
        MenuWindow::Help,
        MenuWindow::Options,
    ];

    fn title_key(self) -> &'static str {
        match self {
            MenuWindow::Inventory => "ui.inventory.title",
            MenuWindow::Sheet => "ui.sheet.title",
            MenuWindow::Techniques => "ui.techniques.title",
            MenuWindow::Quests => "ui.quests.title",
            MenuWindow::Crafting => "ui.crafting.title",
            MenuWindow::Help => "ui.help.title",
            MenuWindow::Options => "ui.options.title",
        }
    }

    fn hint_key(self) -> &'static str {
        match self {
            MenuWindow::Inventory => "ui.menu.inventory",
            MenuWindow::Sheet => "ui.menu.sheet",
            MenuWindow::Techniques => "ui.menu.techniques",
            MenuWindow::Quests => "ui.menu.quests",
            MenuWindow::Crafting => "ui.menu.crafting",
            MenuWindow::Help => "ui.menu.help",
            MenuWindow::Options => "ui.menu.options",
        }
    }
}

/// One button of the menu bar: what opens, its icon, its key and whether
/// something in it wants the player's attention.
/// A gesture of the emote list (phase 9; picture: protocol 20).
#[derive(Clone, Debug, PartialEq)]
pub struct EmoteEntry {
    pub id: base::EmoteId,
    pub name: String,
    pub icon: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuButton {
    pub window: MenuWindow,
    pub icon: Option<[f32; 4]>,
    /// Label of the key that opens it (empty: unbound).
    pub key: String,
    /// Pulses: unspent points, a class to choose.
    pub alert: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudModel {
    /// The menu bar under the minimap.
    pub menu: Vec<MenuButton>,
    pub name: String,
    pub level: u8,
    pub hp: (u64, u64),
    pub vigor: (u64, u64),
    pub mp: (u64, u64),
    pub xp: (u64, u64),
    pub target: Option<TargetModel>,
    /// Ten slots: techniques.
    pub hotbar: Vec<Option<HotbarSlot>>,
    /// Key label of each hotbar slot.
    pub hotbar_keys: Vec<String>,
    pub log: Vec<LogLine>,
    /// System messages that arrived this frame.
    pub system: Vec<String>,
    pub coins: u64,
    pub bag: Vec<Option<BagSlot>>,
    /// What the character wears: 0 weapon, 1 armor.
    pub equipped: Vec<EquippedModel>,
    /// Error lines that arrived this frame: shown over the hotbar too.
    pub errors: Vec<String>,
    /// The help window (F1).
    pub help: HelpModel,
    /// The Options window.
    pub sound: SoundModel,
    pub sheet: SheetModel,
    /// Active quests first, then the completed ones.
    pub quests: Vec<QuestModel>,
    pub minimap: Minimap,
    pub fallen: bool,
    /// Open dialogue: NPC text and options.
    pub dialogue: Option<(String, Vec<String>)>,
    /// Phase 5: how to close the dialogue (`Esc: close`).
    pub dialogue_close: String,
    /// Phase 7: the open shop (`None`: none).
    pub shop: Option<ShopModel>,
    /// Phase 9: the player's group (`None`: not in one).
    pub party: Option<PartyModel>,
    /// What the player can do for others from the player menu.
    pub support: Support,
    /// Gestures the player can make, with their names, for the emote list.
    pub emotes: Vec<EmoteEntry>,
    /// The newest invite or trade request, written out (accept/decline).
    pub invite: Option<String>,
    /// The trade window: both offers, piece by piece.
    pub trade: Option<TradeModel>,
    /// Phase 8: recipes the character knows, in content order.
    pub recipes: Vec<RecipeModel>,
    /// Phase 8: the crafting window has nothing to show.
    pub no_recipes: String,
    /// Names, bars and markers over the beings on screen.
    pub overhead: Vec<Overhead>,
    /// Damage numbers rising from the beings.
    pub floats: Vec<FloatText>,
    /// Techniques the character knows (window K), in learning order.
    pub techniques: Vec<TechniqueModel>,
    /// Actions waiting for the server's answer.
    pub pending: Pending,
}

/// What stays open between frames.
#[derive(Clone, Debug, Default)]
pub struct HudState {
    pub inventory: bool,
    pub sheet: bool,
    pub quests: bool,
    /// Phase 8: the crafting window is open.
    pub crafting: bool,
    pub chat_open: bool,
    pub chat_draft: String,
    pub log_tab: LogTab,
    /// The techniques window (K): drag from it to the hotbar.
    pub techniques: bool,
    /// The help window (F1); `help_seen`: opened once, its button stops pulsing.
    pub help: bool,
    help_seen: bool,
    /// The Options window.
    pub options: bool,
    /// Crafting: the open tab (index among the crafts) and "only what I
    /// can make".
    pub craft_tab: usize,
    pub craft_ready: bool,
    /// Log lines scrolled back from the newest (0: following), and how
    /// many lines the tab had last frame (new ones keep the view still).
    pub log_scroll: usize,
    log_seen: usize,
    /// `(error, shown at ms)` over the hotbar.
    errors: Vec<(String, u64)>,
    /// What the mouse is carrying.
    drag: Option<Drag>,
    /// The copper typed in the trade window, and where that window was
    /// last frame (a backpack slot dropped there joins the offer).
    pub trade_coins: String,
    pub(crate) trade_rect: Option<Rect>,
    /// The quantity being typed on one of my trade lines: `(slot, digits)`.
    pub(crate) trade_qty: Option<(u16, String)>,
    /// The menu of another player (right click on them, or a click on a
    /// group member).
    pub player_menu: Option<PlayerMenu>,
    /// The emote list by the log is open.
    pub emotes_open: bool,
    /// What a click on the backpack does at the open shop.
    pub shop_mode: ShopMode,
    /// Backpack slots as drawn last frame, for a drop between them.
    pub(crate) bag_rects: Vec<Rect>,
    /// First visible row of the windows that scroll (`Scroll::*`).
    pub(crate) scroll: [usize; 5],
    /// `(text, shown at ms)`.
    system: Vec<(String, u64)>,
    /// Each window's fade: open or closing, since when (ms). See `anim`.
    anim: std::collections::BTreeMap<&'static str, (bool, u64)>,
    /// Where each window was last frame, and this frame so far: the
    /// newest window closes the ones it would cover (`anim::resolve`).
    rects: std::collections::BTreeMap<&'static str, Rect>,
    next_rects: std::collections::BTreeMap<&'static str, Rect>,
    /// When each of the player's windows was opened (ms).
    opened: std::collections::BTreeMap<&'static str, u64>,
    /// Windows kept open but not drawn: they would cover a shop, a trade,
    /// a dialogue or an invite, which never close on their own.
    hidden: std::collections::BTreeSet<&'static str>,
}

/// A slot being dragged: where from, and how it looks under the mouse.
#[derive(Clone, Debug)]
struct Drag {
    from: DragSource,
    icon: Option<[f32; 4]>,
    label: String,
}

const CHAT_FIELD: u32 = 100;

/// Windows whose rows may not fit: index in `HudState::scroll`.
#[derive(Clone, Copy)]
enum Scroll {
    Sheet = 0,
    Crafting = 1,
    Quests = 2,
    Shop = 3,
    Help = 4,
}
const ATTRS: [&str; 6] = ["str", "vit", "agi", "dex", "int", "spi"];
/// How long a refusal stays over the hotbar.
const ERROR_MS: u64 = 2500;
/// The key label of the menu button that opens `w` (empty: unbound).
fn key_of(m: &HudModel, w: MenuWindow) -> String {
    m.menu
        .iter()
        .find(|b| b.window == w)
        .map_or_else(String::new, |b| b.key.clone())
}

impl HudState {
    pub fn toggle_inventory(&mut self) {
        self.inventory = !self.inventory;
    }

    /// Status and crafting share the left side: one at a time.
    pub fn toggle_sheet(&mut self) {
        self.sheet = !self.sheet;
        if self.sheet {
            self.crafting = false;
        }
    }

    pub fn toggle_quests(&mut self) {
        self.quests = !self.quests;
    }

    pub fn toggle_techniques(&mut self) {
        self.techniques = !self.techniques;
    }

    /// Help and options share the middle of the screen: one at a time.
    pub fn toggle_help(&mut self) {
        self.help = !self.help;
        self.help_seen = true;
        if self.help {
            self.options = false;
        }
    }

    pub fn toggle_options(&mut self) {
        self.options = !self.options;
        if self.options {
            self.help = false;
        }
    }

    /// Something is being dragged (Esc drops it first).
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn toggle_crafting(&mut self) {
        self.crafting = !self.crafting;
        if self.crafting {
            self.sheet = false;
        }
    }

    /// Esc: closes the chat, else the open windows; false if nothing was open.
    pub fn close_top(&mut self) -> bool {
        if self.drag.is_some() {
            self.drag = None;
        } else if self.player_menu.is_some() || self.emotes_open {
            self.player_menu = None;
            self.emotes_open = false;
        } else if self.chat_open {
            self.chat_open = false;
        } else if self.inventory
            || self.sheet
            || self.quests
            || self.crafting
            || self.techniques
            || self.help
            || self.options
        {
            self.inventory = false;
            self.sheet = false;
            self.quests = false;
            self.crafting = false;
            self.techniques = false;
            self.help = false;
            self.options = false;
        } else {
            return false;
        }
        true
    }

    pub fn draw(&mut self, f: &mut Frame<'_>, m: &HudModel) {
        let now = f.input.now_ms;
        for s in &m.system {
            self.system.push((s.clone(), now));
        }
        let keep = u64::from(f.theme.system_window.show_ms);
        self.system.retain(|(_, at)| now.saturating_sub(*at) < keep);
        if self.system.len() > 4 {
            self.system.drain(..self.system.len() - 4);
        }
        for e in &m.errors {
            // The same refusal again only refreshes the one on screen.
            self.errors.retain(|(t, _)| t != e);
            self.errors.push((e.clone(), now));
        }
        self.errors.retain(|(_, at)| now.saturating_sub(*at) < ERROR_MS);
        if self.errors.len() > 2 {
            self.errors.drain(..self.errors.len() - 2);
        }
        // The dialogue sits over the shop and the backpack: a click on one
        // of its options must not also buy or sell what lies under it.
        let covered = m
            .dialogue
            .as_ref()
            .is_some_and(|(text, options)| dialogue_rect(f, text, options).0.contains(f.input.mouse));
        let asked = if covered {
            f.without_mouse(|f| self.draw_below(f, m))
        } else {
            self.draw_below(f, m)
        };
        match &m.dialogue {
            Some((text, options)) => {
                self.appear(f, "dialogue", |_, f| {
                    dialogue(f, text, options, &m.dialogue_close)
                });
            }
            None => self.gone("dialogue"),
        }
        if m.fallen {
            self.appear(f, "fallen", |_, f| fallen(f));
        } else {
            self.gone("fallen");
        }
        // Drawn last, over everything; one asked for this frame shows next.
        if let Some(menu) = self.player_menu.take()
            && social::player_menu(f, &menu, m.party.as_ref(), m.support)
        {
            self.player_menu = Some(menu);
        }
        if asked.is_some() {
            self.player_menu = asked;
        }
        self.drop(f);
        self.end_layout();
    }

    /// Everything the dialogue window may cover; returns the player menu the
    /// party window asked for.
    fn draw_below(&mut self, f: &mut Frame<'_>, m: &HudModel) -> Option<PlayerMenu> {
        low_health(f, m);
        overlay::overheads(f, &m.overhead);
        overlay::floats(f, &m.floats);
        status(f, m);
        target(f, m);
        minimap(f, m);
        self.menu(f, m);
        hotbar(f, m, &mut self.drag);
        self.log(f, m);
        self.system_windows(f);
        self.error_toasts(f);
        if m.shop.is_none() {
            self.shop_mode = ShopMode::Sell;
        }
        // A shop or a trade opens the backpack: selling or offering is a
        // click on an item. Set first: the tracker gives way to it.
        if m.shop.is_some() || m.trade.is_some() {
            self.inventory = true;
        }
        self.resolve(f, m);
        // The open quests stay in sight while nothing else uses the right side.
        let tracking = !self.quests && !self.inventory && m.shop.is_none();
        if self.animated(f, "tracker", tracking, |_, f| tracker(f, m)) {
            self.quests = true;
        }
        let open = self.inventory;
        if self.animated(f, "inventory", open, |s, f| {
            inventory(f, m, &mut s.drag, s.shop_mode, &mut s.bag_rects)
        }) {
            if m.shop.is_some() {
                f.actions.push(UiAction::CloseShop);
            }
            self.inventory = false;
        }
        let open = self.techniques;
        if self.animated(f, "techniques", open, |s, f| techniques(f, m, &mut s.drag)) {
            self.techniques = false;
        }
        let open = self.sheet;
        if self.animated(f, "sheet", open, |s, f| {
            sheet(f, m, &mut s.scroll[Scroll::Sheet as usize])
        }) {
            self.sheet = false;
        }
        // A shop or a trade takes the place beside the backpack that the
        // quest log would use.
        let (open, beside) = (
            self.quests && m.shop.is_none() && m.trade.is_none(),
            self.inventory,
        );
        if self.animated(f, "quests", open, |s, f| {
            quests(f, m, beside, &mut s.scroll[Scroll::Quests as usize])
        }) {
            self.quests = false;
        }
        let open = self.crafting;
        if self.animated(f, "crafting", open, |s, f| s.crafting_window(f, m)) {
            self.crafting = false;
        }
        match &m.shop {
            Some(shop_model) => {
                let closed = self.appear(f, "shop", |s, f| {
                    shop(
                        f,
                        shop_model,
                        m.pending.shop,
                        &mut s.shop_mode,
                        &mut s.scroll[Scroll::Shop as usize],
                    )
                });
                if closed {
                    f.actions.push(UiAction::CloseShop);
                }
            }
            None => self.gone("shop"),
        }
        let open = self.help;
        if self.animated(f, "help", open, |s, f| {
            help(f, m, &mut s.scroll[Scroll::Help as usize])
        }) {
            self.help = false;
        }
        let open = self.options;
        if self.animated(f, "options", open, |_, f| options(f, m)) {
            self.options = false;
        }
        // The sheet and crafting use the same column at the left.
        let asked = match m.party.as_ref().filter(|_| !self.sheet && !self.crafting) {
            Some(p) => self.appear(f, "party", |_, f| social::party(f, p)),
            None => {
                self.gone("party");
                None
            }
        };
        self.trade_rect = None;
        if let Some(t) = &m.trade {
            let r = self.appear(f, "trade", |s, f| {
                trade::draw(f, t, &mut s.trade_coins, &mut s.trade_qty)
            });
            self.trade_rect = Some(r);
        } else {
            self.gone("trade");
            self.trade_coins.clear();
            self.trade_qty = None;
        }
        match &m.invite {
            Some(text) => self.appear(f, "invite", |_, f| trade::invite(f, text)),
            None => self.gone("invite"),
        }
        asked
    }

    /// The window under `name` this frame is drawn: next frame's layout
    /// knows where it is.
    fn end_layout(&mut self) {
        self.rects = std::mem::take(&mut self.next_rects);
    }

    /// Refusals ("no target", "not enough MP") in red over the hotbar,
    /// where the eyes are, and not only in the log.
    fn error_toasts(&self, f: &mut Frame<'_>) {
        if self.errors.is_empty() {
            return;
        }
        let s = f.theme.sizes.clone();
        let pad = f.px(s.padding);
        let lh = f.line_height(Role::Text, s.text);
        let px = f.px(s.text);
        let mut y = hotbar_rect(f, 0).y - pad - lh;
        let widest = f.px(640.0).min(f.input.screen.0 - 2.0 * pad);
        for (text, at) in self.errors.iter().rev() {
            let age = f.input.now_ms.saturating_sub(*at) as f32 / ERROR_MS as f32;
            let mut color = f.theme.colors.error;
            color[3] *= (1.0 - (age - 0.7).max(0.0) / 0.3).clamp(0.0, 1.0);
            // A long refusal takes a second line instead of being cut.
            let lines = f.fonts.wrap(Role::Text, px, text, widest);
            for line in lines.iter().take(2).rev() {
                let w = f.fonts.measure(Role::Text, px, line).min(widest);
                let x = (f.input.screen.0 - w) / 2.0;
                let mut back = f.theme.colors.background;
                back[3] = 0.6 * color[3];
                f.painter.rect(Rect::new(x - pad / 2.0, y, w + pad, lh), back);
                f.label_fit(Role::Text, s.text, (x, y), w, color, line);
                y -= lh;
            }
            y -= pad / 4.0;
        }
    }

    fn is_open(&self, w: MenuWindow) -> bool {
        match w {
            MenuWindow::Inventory => self.inventory,
            MenuWindow::Sheet => self.sheet,
            MenuWindow::Techniques => self.techniques,
            MenuWindow::Quests => self.quests,
            MenuWindow::Crafting => self.crafting,
            MenuWindow::Help => self.help,
            MenuWindow::Options => self.options,
        }
    }

    pub fn toggle(&mut self, w: MenuWindow) {
        match w {
            MenuWindow::Inventory => self.toggle_inventory(),
            MenuWindow::Sheet => self.toggle_sheet(),
            MenuWindow::Techniques => self.toggle_techniques(),
            MenuWindow::Quests => self.toggle_quests(),
            MenuWindow::Crafting => self.toggle_crafting(),
            MenuWindow::Help => self.toggle_help(),
            MenuWindow::Options => self.toggle_options(),
        }
    }

    /// Icon buttons under the minimap: every window is one click away and
    /// the tooltip teaches its key.
    fn menu(&mut self, f: &mut Frame<'_>, m: &HudModel) {
        if m.menu.is_empty() {
            return;
        }
        let s = f.theme.sizes.clone();
        let pad = f.px(s.padding);
        let c = f.theme.colors.clone();
        let n = m.menu.len() as f32;
        // The same size with four buttons or five, right under the minimap.
        let slot = menu_slot(f);
        let w = n * slot + (n - 1.0) * pad / 4.0;
        let top = if m.minimap.side == 0 {
            pad
        } else {
            2.0 * pad + f.px(s.minimap)
        };
        let x0 = f.input.screen.0 - w - pad;
        f.place("menu", Rect::new(x0, top, w, slot));
        let mut tip = None;
        for (i, b) in m.menu.iter().enumerate() {
            let r = Rect::new(x0 + i as f32 * (slot + pad / 4.0), top, slot, slot);
            let hover = r.contains(f.input.mouse);
            let open = self.is_open(b.window);
            f.panel(r, hover || open);
            let unseen = b.window == MenuWindow::Help && !self.help_seen;
            if (b.alert || unseen) && !open {
                pulse(f, r);
            }
            match b.icon {
                Some(uv) => {
                    let k = if hover || open { 1.0 } else { 0.8 };
                    f.painter.image(r.shrink(pad / 4.0), uv, [k, k, k, 1.0]);
                }
                None => {
                    let title = f.tr(b.window.title_key());
                    f.label_center(Role::Text, s.small, r, c.text, &title);
                }
            }
            if !b.key.is_empty() {
                f.label(
                    Role::Numbers,
                    s.small,
                    (r.x + pad / 4.0, r.y),
                    c.highlight,
                    &b.key,
                );
            }
            if hover {
                let title = f.tr(b.window.title_key());
                let head = if b.key.is_empty() {
                    title
                } else {
                    format!("{title} ({})", b.key)
                };
                tip = Some(vec![(head, c.highlight), (f.tr(b.window.hint_key()), c.text)]);
            }
            if f.tapped(r) {
                self.toggle(b.window);
            }
        }
        if let Some(lines) = tip {
            overlay::tooltip(f, &lines);
        }
    }

    /// The carried slot follows the mouse; letting go on a hotbar slot puts
    /// it there, letting a hotbar slot go anywhere else empties it.
    fn drop(&mut self, f: &mut Frame<'_>) {
        let Some(d) = &self.drag else { return };
        if f.input.released || !f.input.mouse_down {
            let on = (0..10u8).find(|i| hotbar_rect(f, *i).contains(f.input.mouse));
            let on_trade = self.trade_rect.is_some_and(|r| r.contains(f.input.mouse));
            let on_bag = self
                .bag_rects
                .iter()
                .position(|r| r.contains(f.input.mouse))
                .filter(|_| self.inventory)
                .map(|i| i as u16);
            match (on, d.from) {
                (Some(slot), from) if from != DragSource::Hotbar(slot) => {
                    f.actions.push(UiAction::HotbarDrop { slot, from });
                }
                (None, DragSource::Bag(b)) if on_trade => f.actions.push(UiAction::TradeAdd(b)),
                // Dropped on another backpack slot: swap or merge.
                (None, DragSource::Bag(from)) if on_bag.is_some_and(|to| to != from) => {
                    if let Some(to) = on_bag {
                        f.actions.push(UiAction::MoveItem { from, to });
                    }
                }
                (None, DragSource::Hotbar(i)) => f.actions.push(UiAction::HotbarClear(i)),
                _ => {}
            }
            self.drag = None;
            return;
        }
        let slot = f.px(f.theme.sizes.hotbar_slot);
        let r = Rect::new(
            f.input.mouse.0 - slot / 2.0,
            f.input.mouse.1 - slot / 2.0,
            slot,
            slot,
        );
        let c = f.theme.colors.clone();
        f.painter.panel(f.theme, r, true);
        match d.icon {
            Some(uv) => f.painter.image(r.shrink(f.px(2.0)), uv, [1.0, 1.0, 1.0, 0.8]),
            None => {
                let label = d.label.clone();
                f.label_center(Role::Text, f.theme.sizes.small, r, c.highlight, &label);
            }
        }
    }

    fn log(&mut self, f: &mut Frame<'_>, m: &HudModel) {
        let s = &f.theme.sizes;
        let (pad, size, lines) = (f.px(s.padding), s.small, usize::from(s.log_lines));
        let lh = f.line_height(Role::Text, size);
        let field_h = if self.chat_open { lh + 2.0 * pad } else { 0.0 };
        // Left of the hotbar, never under it.
        let hotbar_w = 10.0 * f.px(s.hotbar_slot) + 4.5 * pad;
        let w = f
            .px(440.0)
            .min((f.input.screen.0 - hotbar_w) / 2.0 - 2.0 * pad)
            .max(f.px(160.0));
        let h = lh * lines as f32 + 2.0 * pad;
        let r = Rect::new(pad, f.input.screen.1 - h - field_h - pad, w, h);
        f.place("log", r);
        let (end, tab_y, tab_h) = self.log_tabs(f, r);
        self.emotes_open = social::emotes(f, (end, tab_y), tab_h, &m.emotes, self.emotes_open);
        f.panel(r, false);
        // A long line wraps instead of being cut: the view counts the
        // wrapped lines.
        let px = f.px(size);
        let width = r.w - 2.0 * pad;
        let mut shown: Vec<(String, LineKind)> = Vec::new();
        for l in m.log.iter().filter(|l| self.log_tab.shows(l.kind)) {
            for part in f.fonts.wrap(Role::Text, px, &l.text, width) {
                shown.push((part, l.kind));
            }
        }
        // The wheel over the log reads back; new lines do not move the view.
        if self.log_scroll > 0 {
            self.log_scroll += shown.len().saturating_sub(self.log_seen);
        }
        self.log_seen = shown.len();
        if r.contains(f.input.mouse) && f.input.wheel != 0.0 {
            let step = f.input.wheel.abs().ceil().max(1.0) as usize;
            self.log_scroll = if f.input.wheel > 0.0 {
                self.log_scroll + step
            } else {
                self.log_scroll.saturating_sub(step)
            };
        }
        self.log_scroll = self.log_scroll.min(shown.len().saturating_sub(lines));
        let end = shown.len() - self.log_scroll;
        let start = end.saturating_sub(lines);
        for (i, (text, kind)) in shown[start..end].iter().enumerate() {
            let c = &f.theme.colors;
            let color = match kind {
                LineKind::System | LineKind::Quest => c.system,
                LineKind::Chat => c.text,
                LineKind::Combat => c.dim_text,
                LineKind::Error => c.error,
            };
            f.label_fit(
                Role::Text,
                size,
                (r.x + pad, r.y + pad + lh * i as f32),
                r.w - 2.0 * pad,
                color,
                text,
            );
        }
        if self.log_scroll > 0 {
            // How far back the view is; the wheel down returns.
            let c = f.theme.colors.clone();
            let more = f.text.t_args("ui.log.newer", &[Arg::Int(self.log_scroll as i64)]);
            let px = f.px(size);
            let w = f.fonts.measure(Role::Text, px, &more);
            let at = (r.x + r.w - pad - w, r.y + r.h - pad - lh);
            let mut back = c.background;
            back[3] = 0.85;
            f.painter
                .rect(Rect::new(at.0 - pad / 2.0, at.1, w + pad, lh), back);
            f.label(Role::Text, size, at, c.highlight, &more);
        }
        if self.chat_open {
            *f.focus = Some(CHAT_FIELD);
            let fr = Rect::new(r.x, r.y + r.h + pad / 2.0, r.w, field_h - pad / 2.0);
            let mut draft = std::mem::take(&mut self.chat_draft);
            if f.field(CHAT_FIELD, fr, &mut draft, false, 200) {
                let text = draft.trim().to_owned();
                if !text.is_empty() {
                    f.actions.push(UiAction::SendChat(text));
                }
                draft.clear();
                self.chat_open = false;
                *f.focus = None;
            }
            self.chat_draft = draft;
        }
    }

    /// The tab row over the log; returns where the row ends (x), its top
    /// and its height.
    fn log_tabs(&mut self, f: &mut Frame<'_>, log: Rect) -> (f32, f32, f32) {
        let s = f.theme.sizes.clone();
        let pad = f.px(s.padding);
        let lh = f.line_height(Role::Text, s.small);
        let h = lh + pad / 2.0;
        let mut x = log.x;
        for tab in LogTab::ALL {
            let label = f.tr(tab.key());
            let w = f.fonts.measure(Role::Text, f.px(s.small), &label) + pad * 1.5;
            let r = Rect::new(x, log.y - h - pad / 4.0, w, h);
            let hover = r.contains(f.input.mouse);
            f.panel(r, hover || tab == self.log_tab);
            let c = &f.theme.colors;
            let color = if tab == self.log_tab {
                c.highlight
            } else {
                c.dim_text
            };
            f.label_center(Role::Text, s.small, r, color, &label);
            if hover && f.input.clicked {
                self.log_tab = tab;
                self.log_scroll = 0;
            }
            x += w + pad / 4.0;
        }
        (x, log.y - h - pad / 4.0, h)
    }

    fn system_windows(&self, f: &mut Frame<'_>) {
        let prefix = f.tr(&f.theme.system_window.prefix_key.clone());
        let size = f.theme.sizes.text;
        let pad = f.px(f.theme.sizes.padding);
        let lh = f.line_height(Role::Text, size);
        let w = f.px(420.0).min(f.input.screen.0 - 2.0 * pad);
        let mut y = f.input.screen.1 * 0.18;
        for (text, at) in &self.system {
            let age = f.input.now_ms.saturating_sub(*at) as f32;
            let enter = (age / f.theme.system_window.enter_anim_ms.max(1) as f32).min(1.0);
            let px = f.px(size);
            let pw = f.fonts.measure(Role::Title, px, &prefix);
            // A long message takes more lines instead of being cut.
            let lines = f.fonts.wrap(Role::Text, px, text, w - pw - 3.0 * pad);
            let r = Rect::new(
                (f.input.screen.0 - w) / 2.0,
                y - (1.0 - enter) * lh,
                w,
                lh * lines.len().max(1) as f32 + 2.0 * pad,
            );
            f.panel(r, true);
            f.painter.text(
                f.fonts,
                Role::Title,
                px,
                (r.x + pad, r.y + pad),
                f.theme.colors.system,
                &prefix,
            );
            let color = f.theme.colors.text;
            for (i, line) in lines.iter().enumerate() {
                f.label_fit(
                    Role::Text,
                    size,
                    (r.x + pad * 2.0 + pw, r.y + pad + lh * i as f32),
                    r.w - pw - 3.0 * pad,
                    color,
                    line,
                );
            }
            y += r.h + pad / 2.0;
        }
    }
}

/// Side of one button of the menu bar: all of them fit the width of the minimap.
fn menu_slot(f: &Frame<'_>) -> f32 {
    let s = &f.theme.sizes;
    let n = MenuWindow::ALL.len() as f32;
    (f.px(s.minimap) - (n - 1.0) * f.px(s.padding) / 4.0) / n
}

/// Where the right-hand windows start: under the minimap and the menu bar.
fn right_top(f: &Frame<'_>) -> f32 {
    let pad = f.px(f.theme.sizes.padding);
    f.px(f.theme.sizes.minimap) + 3.0 * pad + menu_slot(f)
}

/// Where the left-hand windows start: just under the status strip.
fn under_status(f: &Frame<'_>) -> f32 {
    let s = &f.theme.sizes;
    let pad = f.px(s.padding);
    f.px(s.bar_height) + 4.0 * pad + pad
}

/// One strip at the top left: level and name, then life, vigor, mana and XP side
/// by side.
fn status(f: &mut Frame<'_>, m: &HudModel) {
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let bar = f.px(s.bar_height);
    let c = f.theme.colors.clone();
    let title = f.text.t_args("ui.hud.level", &[Arg::Int(i64::from(m.level))]);
    let tw = f.fonts.measure(Role::Title, f.px(s.small), &title);
    let name_w = f.px(90.0);
    // Never reach the target panel at the top center.
    let total = f.px(600.0).min(f.input.screen.0 / 2.0 - f.px(130.0) - 2.0 * pad);
    let r = Rect::new(pad, pad, total.max(f.px(240.0)), bar + 2.0 * pad);
    f.place("status", r);
    f.panel(r, false);
    let lh = f.line_height(Role::Title, s.small);
    let ty = r.y + (r.h - lh) / 2.0;
    f.label(Role::Title, s.small, (r.x + pad, ty), c.highlight, &title);
    f.label_fit(
        Role::Text,
        s.small,
        (r.x + 2.0 * pad + tw, ty),
        name_w.min(r.w / 4.0),
        c.text,
        &m.name,
    );
    let x0 = r.x + pad + tw + pad + name_w.min(r.w / 4.0) + pad;
    let gap = pad * 1.5;
    // The coins close the strip at the right, always in sight.
    let coins = f.text.t_args("ui.inventory.coins", &[Arg::Str(grouped(m.coins))]);
    let coin_w = coin_tag(f, &coins, (r.x + r.w - pad, ty), lh);
    let each = (r.x + r.w - pad - coin_w - gap - x0 - 3.0 * gap) / 4.0;
    let nlh = f.line_height(Role::Numbers, s.small);
    let mut tip = None;
    for (i, ((v, max), color, key)) in [
        (m.hp, c.life, "ui.hud.hp"),
        (m.vigor, c.stamina, "ui.hud.vigor"),
        (m.mp, c.mana, "ui.hud.mp"),
        (m.xp, c.xp, "ui.hud.xp"),
    ]
    .into_iter()
    .enumerate()
    {
        let br = Rect::new(x0 + i as f32 * (each + gap), r.y + pad, each, bar);
        f.painter.bar(f.theme, br, v, max, color);
        let name = f.tr(key);
        let full = format!("{name} {}/{}", grouped(v), grouped(max));
        let px = f.px(s.small);
        // Numbers that do not fit (XP at high levels) become a percent;
        // the whole of it is on hover.
        let label = if f.fonts.measure(Role::Numbers, px, &full) <= br.w - pad {
            full.clone()
        } else {
            format!("{name} {}%", (v.min(max) * 100).checked_div(max).unwrap_or(0))
        };
        f.label_fit(
            Role::Numbers,
            s.small,
            (br.x + pad / 2.0, br.y + (br.h - nlh) / 2.0),
            br.w - pad,
            c.text,
            &label,
        );
        if br.contains(f.input.mouse) {
            let mut lines = vec![(full, c.highlight)];
            if key == "ui.hud.xp" {
                let left = f
                    .text
                    .t_args("ui.hud.xp_left", &[Arg::Str(grouped(max.saturating_sub(v)))]);
                lines.push((left, c.text));
            }
            tip = Some(lines);
        }
    }
    if let Some(lines) = tip {
        overlay::tooltip(f, &lines);
    }
}

/// A coin and the amount, right-aligned to `right.0` on the line at
/// `right.1`. Returns the width it took.
fn coin_tag(f: &mut Frame<'_>, text: &str, right: (f32, f32), lh: f32) -> f32 {
    let s = f.theme.sizes.clone();
    let c = f.theme.colors.clone();
    let pad = f.px(s.padding);
    let tw = f.fonts.measure(Role::Numbers, f.px(s.small), text);
    let d = (lh * 0.8).max(6.0);
    let w = d + pad / 2.0 + tw;
    let x = right.0 - w;
    // An octagon in the highlight color with a darker core: a coin.
    let (cx, cy) = (x + d / 2.0, right.1 + lh / 2.0);
    let h = c.highlight;
    let core = [h[0] * 0.6, h[1] * 0.6, h[2] * 0.6, h[3]];
    for (rad, color) in [(d / 2.0, h), (d / 3.2, core)] {
        let pts: Vec<(f32, f32)> = (0..8)
            .map(|i| {
                let a = std::f32::consts::FRAC_PI_8 + i as f32 * std::f32::consts::FRAC_PI_4;
                (cx + rad * a.cos(), cy + rad * a.sin())
            })
            .collect();
        for i in 1..7 {
            f.painter.tri(pts[0], pts[i], pts[i + 1], color);
        }
    }
    f.label(
        Role::Numbers,
        s.small,
        (x + d + pad / 2.0, right.1),
        c.highlight,
        text,
    );
    w
}

#[cfg(test)]
pub(crate) fn grouped_for_tests(n: u64) -> String {
    grouped(n)
}

/// `1234567` as `1,234,567`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn target(f: &mut Frame<'_>, m: &HudModel) {
    let Some(t) = &m.target else { return };
    let s = f.theme.sizes.clone();
    let pad = f.px(s.padding);
    let lh = f.line_height(Role::Title, s.text);
    let w = f.px(260.0);
    let r = Rect::new(
        (f.input.screen.0 - w) / 2.0,
        pad,
        w,
        lh + f.px(s.bar_height) + 3.0 * pad,
    );
    f.place("target", r);
    f.panel(r, true);
    let level = f.text.t_args("ui.hud.level", &[Arg::Int(i64::from(t.level))]);
    let c = f.theme.colors.clone();
    // The level's color says how hard the fight is next to the player's own.
    let gap = i16::from(t.level) - i16::from(m.level);
    let (con, con_key) = match gap {
        i16::MIN..=-5 => (c.dim_text, "ui.target.trivial"),
        -4..=-2 => (c.text, "ui.target.easy"),
        -1..=1 => (c.highlight, "ui.target.even"),
        2..=4 => (c.stamina, "ui.target.hard"),
        _ => (c.life, "ui.target.deadly"),
    };
    let lw = f.label(Role::Title, s.text, (r.x + pad, r.y + pad), con, &level);
    if r.contains(f.input.mouse) {
        let tip = f.tr(con_key);
        overlay::tooltip(f, &[(tip, con)]);
    }
    f.label_fit(
        Role::Text,
        s.text,
        (r.x + 2.0 * pad + lw, r.y + pad),
        r.w - lw - 3.0 * pad,
        c.text,
        &t.name,
    );
    let br = Rect::new(
        r.x + pad,
        r.y + 2.0 * pad + lh,
        r.w - 2.0 * pad,
        f.px(s.bar_height),
    );
    f.painter.bar(f.theme, br, u64::from(t.health), 100, c.life);
    let pct = format!("{}%", t.health);
    let nlh = f.line_height(Role::Numbers, s.small);
    let pw = f.fonts.measure(Role::Numbers, f.px(s.small), &pct);
    f.label(
        Role::Numbers,
        s.small,
        (br.x + (br.w - pw) / 2.0, br.y + (br.h - nlh) / 2.0),
        c.text,
        &pct,
    );
}

/// Red edges that beat when life is low, so it is seen in the middle of a fight.
fn low_health(f: &mut Frame<'_>, m: &HudModel) {
    let (hp, max) = m.hp;
    if m.fallen || max == 0 || hp * 4 > max {
        return;
    }
    let phase = (f.input.now_ms % 1000) as f32 / 1000.0;
    let beat = 0.5 + 0.5 * (phase * std::f32::consts::TAU).sin().abs();
    let (w, h) = f.input.screen;
    let band = f.px(10.0);
    for i in 0..6 {
        let mut color = f.theme.colors.life;
        color[3] = 0.10 * beat * (1.0 - i as f32 / 6.0);
        let d = i as f32 * band;
        for r in [
            Rect::new(0.0, d, w, band),
            Rect::new(0.0, h - d - band, w, band),
            Rect::new(d, 0.0, band, h),
            Rect::new(w - d - band, 0.0, band, h),
        ] {
            f.painter.rect(r, color);
        }
    }
}

fn minimap(f: &mut Frame<'_>, m: &HudModel) {
    let mm = &m.minimap;
    if mm.side == 0 || mm.tiles.len() != usize::from(mm.side) * usize::from(mm.side) {
        return;
    }
    let pad = f.px(f.theme.sizes.padding);
    let size = f.px(f.theme.sizes.minimap);
    let r = Rect::new(f.input.screen.0 - size - pad, pad, size, size);
    f.place("minimap", r);
    f.panel(r, false);
    let inner = r.shrink(pad / 2.0);
    let cell = inner.w / f32::from(mm.side);
    let c = f.theme.colors.clone();
    let floor = [c.dim_text[0], c.dim_text[1], c.dim_text[2], c.dim_text[3] * 0.35];
    for (i, t) in mm.tiles.iter().enumerate() {
        let color = match t {
            1 => floor,
            2 => c.border,
            _ => continue,
        };
        let (x, y) = (
            (i % usize::from(mm.side)) as f32,
            (i / usize::from(mm.side)) as f32,
        );
        f.painter.rect(
            Rect::new(inner.x + x * cell, inner.y + y * cell, cell, cell),
            color,
        );
    }
    let half = f32::from(mm.side) / 2.0;
    for (dx, dy, kind) in &mm.dots {
        let color = match kind {
            Dot::Me => c.highlight,
            Dot::Player => c.text,
            Dot::Npc => c.secondary,
            Dot::Monster => c.life,
            Dot::Quest => c.highlight,
            Dot::Goal => c.system,
        };
        let (x, y) = (half + f32::from(*dx), half + f32::from(*dy));
        if x < 0.0 || y < 0.0 || x >= f32::from(mm.side) || y >= f32::from(mm.side) {
            continue;
        }
        let d = match kind {
            Dot::Goal => (cell * 4.0).max(f.px(7.0)),
            Dot::Quest => (cell * 3.0).max(f.px(5.0)),
            _ => (cell * 2.0).max(f.px(3.0)),
        };
        f.painter.rect(
            Rect::new(inner.x + x * cell - d / 2.0, inner.y + y * cell - d / 2.0, d, d),
            color,
        );
    }
    // What the dots mean.
    if r.contains(f.input.mouse) {
        let lines: Vec<(String, Color)> = [
            ("ui.minimap.title", c.highlight),
            ("ui.minimap.me", c.highlight),
            ("ui.minimap.quest", c.highlight),
            ("ui.minimap.goal", c.system),
            ("ui.minimap.npc", c.secondary),
            ("ui.minimap.monster", c.life),
            ("ui.minimap.player", c.text),
        ]
        .into_iter()
        .map(|(k, color)| (f.tr(k), color))
        .collect();
        overlay::tooltip(f, &lines);
    }
}

/// Where hotbar slot `i` sits on the screen.
fn hotbar_rect(f: &Frame<'_>, i: u8) -> Rect {
    let s = &f.theme.sizes;
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let n = 10.0;
    let w = n * slot + (n - 1.0) * pad / 2.0;
    let (x0, y) = ((f.input.screen.0 - w) / 2.0, f.input.screen.1 - slot - pad);
    Rect::new(x0 + f32::from(i) * (slot + pad / 2.0), y, slot, slot)
}

/// A soft pulse over a slot whose action waits for the server.
fn pulse(f: &mut Frame<'_>, r: Rect) {
    let phase = (f.input.now_ms % 600) as f32 / 600.0;
    let a = 0.15 + 0.2 * (phase * std::f32::consts::TAU).sin().abs();
    let mut color = f.theme.colors.highlight;
    color[3] = a;
    f.painter.rect(r, color);
}

fn hotbar(f: &mut Frame<'_>, m: &HudModel, drag: &mut Option<Drag>) {
    let s = f.theme.sizes.clone();
    let (pad, slot) = (f.px(s.padding), f.px(s.hotbar_slot));
    let c = f.theme.colors.clone();
    let mut tip = None;
    let carrying = drag.is_some();
    let (first, last) = (hotbar_rect(f, 0), hotbar_rect(f, 9));
    f.place(
        "hotbar",
        Rect::new(first.x, first.y, last.x + last.w - first.x, first.h),
    );
    for i in 0..10u8 {
        let r = hotbar_rect(f, i);
        let hover = r.contains(f.input.mouse);
        // While carrying something every slot shows it can take it.
        f.panel(r, hover || carrying);
        if carrying && hover {
            let mut glow = c.highlight;
            glow[3] = 0.25;
            f.painter.rect(r, glow);
        }
        if let Some(Some(t)) = m.hotbar.get(usize::from(i)) {
            if t.cooldown > 0.0 {
                // Shade that drains as the cooldown runs out.
                let h = slot * t.cooldown.clamp(0.0, 1.0);
                f.painter
                    .rect(Rect::new(r.x, r.y + slot - h, slot, h), c.cooldown);
            }
            let color = if t.usable { c.text } else { c.dim_text };
            match t.icon {
                Some(uv) => {
                    let k = if t.usable { 1.0 } else { 0.45 };
                    f.painter.image(r.shrink(pad / 4.0), uv, [k, k, k, 1.0]);
                }
                None => f.label_fit(
                    Role::Text,
                    s.small,
                    (r.x + pad / 2.0, r.y + slot / 2.0 - pad / 2.0),
                    slot - pad,
                    color,
                    &t.name,
                ),
            }
            if let Some(count) = &t.count {
                let lh = f.line_height(Role::Numbers, s.small);
                let w = f.fonts.measure(Role::Numbers, f.px(s.small), count);
                f.label(
                    Role::Numbers,
                    s.small,
                    (r.x + slot - w - pad / 4.0, r.y + slot - lh),
                    if t.usable { c.highlight } else { c.error },
                    count,
                );
            }
            if let Some(left) = &t.cooldown_text {
                f.label_center(Role::Numbers, s.text, r, c.highlight, left);
            }
            if hover && !carrying {
                let mut lines = vec![(t.name.clone(), c.highlight)];
                lines.extend(t.tooltip.iter().map(|l| (l.clone(), c.text)));
                lines.push((f.tr("ui.hotbar.drag_hint"), c.dim_text));
                tip = Some(lines);
            }
            if !carrying && f.drag_started(r) {
                *drag = Some(Drag {
                    from: DragSource::Hotbar(i),
                    icon: t.icon,
                    label: t.name.clone(),
                });
            }
        }
        let empty = !matches!(m.hotbar.get(usize::from(i)), Some(Some(_)));
        if empty && hover && !carrying {
            tip = Some(vec![(f.tr("ui.hotbar.empty"), c.dim_text)]);
        }
        if let Some(k) = m.hotbar_keys.get(usize::from(i)) {
            f.label(
                Role::Numbers,
                s.small,
                (r.x + pad / 2.0, r.y + pad / 4.0),
                c.dim_text,
                k,
            );
        }
        if !carrying && f.tapped(r) {
            f.actions.push(UiAction::Hotbar(i));
        }
    }
    if let Some(lines) = tip {
        overlay::tooltip(f, &lines);
    }
}

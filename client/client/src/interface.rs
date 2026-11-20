//! Wiring of `client-ui`: theme and fonts from `assets/` (the theme file
//! is checked every second and reloaded when it changes: no rebuild), the
//! key map from the user folder, and the HUD model built from `WorldState`.

use crate::hotbar::{Bind, Hotbar};
use crate::text::Texts;
use base::ItemKind;
use base::{Arg, EntityId, Fixed};
use base::{Attribute, Locale, Look, TextRef, TilePos};
use client_render::{Camera, RenderConfig, tile_to_screen};
use client_state::TICK_MS;
use client_state::WorldState;
use client_ui::{
    BagSlot, Dot, Figure, FloatKind, FloatText, Fonts, HotbarSlot, HudModel, KeyMap, LineKind, LogLine,
    LookList, MarkerKind, MenuButton, MenuWindow, Minimap, Overhead, QuestModel, SheetModel, TargetModel, Ui,
    UiText, load_theme, read_game_theme,
};
use protocol::{DeviceToken, EntityKind, EntityName, ObjectiveView, QuestMarker, QuestState, TILE_BLOCKS};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tracing::{info, warn};

const CHECK_EVERY: Duration = Duration::from_secs(1);
/// Lines kept in the log.
const LOG_KEEP: usize = 200;
/// Tiles on each side of the minimap.
const MINIMAP_SIDE: u16 = 48;

pub struct Assets {
    dir: PathBuf,
    /// `--theme`: wins over the game's theme.
    forced: Option<String>,
    /// The theme last loaded.
    theme: String,
    locale: Locale,
    stamp: Option<SystemTime>,
    checked: Instant,
}

impl Assets {
    pub fn new(dir: PathBuf, forced: Option<String>, locale: Locale) -> Assets {
        let mut a = Assets {
            dir,
            forced,
            theme: client_ui::FALLBACK_THEME.into(),
            locale,
            stamp: None,
            checked: Instant::now(),
        };
        a.theme = a.wanted_theme();
        a
    }

    /// `--theme`, else the theme `assets/theme.ron` names (set in the
    /// editor), else the fallback.
    fn wanted_theme(&self) -> String {
        if let Some(t) = &self.forced {
            return t.clone();
        }
        let path = self.dir.join(client_ui::GAME_THEME_FILE);
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| read_game_theme(&t).map_err(|e| e.to_string()))
        {
            Ok(id) => id,
            Err(e) => {
                warn!(path = %path.display(), "game theme not read, using {}: {e}", client_ui::FALLBACK_THEME);
                client_ui::FALLBACK_THEME.into()
            }
        }
    }

    fn theme_path(&self) -> PathBuf {
        self.dir.join("themes").join(format!("{}.ron", self.theme))
    }

    /// Theme and fonts; `None` if the theme file is missing or broken.
    fn load(&mut self) -> Option<(client_ui::UiTheme, Fonts)> {
        let path = self.theme_path();
        self.stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warn!(path = %path.display(), "theme not read: {e}");
                return None;
            }
        };
        let theme = match load_theme(&text) {
            Ok(t) => t,
            Err(e) => {
                warn!(path = %path.display(), "{e}");
                return None;
            }
        };
        let dir = self.dir.clone();
        let (fonts, problems) = Fonts::load(&theme, self.locale.code(), &mut |p: &str| {
            std::fs::read(dir.join(p)).ok()
        });
        for p in problems {
            warn!("{p}");
        }
        info!(theme = %theme.id, "theme loaded");
        Some((theme, fonts))
    }

    /// The interface with the game's theme, or the built-in one when the
    /// game has none that loads.
    pub fn ui(&mut self) -> Result<Ui, String> {
        let (theme, fonts) = match self.load() {
            Some(loaded) => loaded,
            None => {
                let theme = load_theme(include_str!(
                    "../../../templates/new-game/assets/themes/isekai.ron"
                ))
                .map_err(|e| format!("built-in theme: {e}"))?;
                let (fonts, _) = Fonts::load(&theme, "en", &mut |_| None);
                (theme, fonts)
            }
        };
        Ok(Ui::new(theme, fonts, UiText::new(self.locale)))
    }

    /// Another language: its fonts (CJK faces differ) and interface texts.
    pub fn set_locale(&mut self, locale: Locale, ui: &mut Ui) {
        self.locale = locale;
        if let Some((theme, fonts)) = self.load() {
            ui.set_theme(theme, fonts);
        }
        ui.set_text(UiText::new(locale));
    }

    /// Reloads the theme when its file changed on disk or the game's theme
    /// (`assets/theme.ron`) changed.
    pub fn reload_if_changed(&mut self, ui: &mut Ui) {
        if self.checked.elapsed() < CHECK_EVERY {
            return;
        }
        self.checked = Instant::now();
        // The editor may switch the game to another theme.
        let wanted = self.wanted_theme();
        let switched = wanted != self.theme;
        if switched {
            self.theme = wanted;
        }
        let stamp = std::fs::metadata(self.theme_path())
            .and_then(|m| m.modified())
            .ok();
        if stamp.is_some()
            && (switched || stamp != self.stamp)
            && let Some((theme, fonts)) = self.load()
        {
            ui.set_theme(theme, fonts);
        }
    }
}

/// The user folder: `~/.config/<game>/` on Linux, `%APPDATA%\<game>\`
/// on Windows, `<game>` being [`client_ui::game_id`].
pub fn config_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
    };
    base.map(|b| b.join(client_ui::game_id()))
}

/// `login.ron` in the user folder: the server list (the official one
/// first), the language, the account name if "remember account" was on,
/// and this device's token per server and account. Never the
/// password. Readable only by its owner on Unix.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LoginConfig {
    pub servers: Vec<String>,
    pub account: Option<String>,
    pub locale: Option<String>,
    /// `"server|account"` → token in hex.
    pub devices: BTreeMap<String, String>,
}

impl LoginConfig {
    pub fn load() -> LoginConfig {
        let Some(path) = config_dir().map(|d| d.join("login.ron")) else {
            return LoginConfig::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => ron::from_str(&text).unwrap_or_else(|e| {
                warn!(path = %path.display(), "{e}");
                LoginConfig::default()
            }),
            Err(_) => LoginConfig::default(),
        }
    }

    pub fn save(&self) {
        let Some(dir) = config_dir() else { return };
        let text = match ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()) {
            Ok(t) => t,
            Err(e) => return warn!("login.ron: {e}"),
        };
        let written =
            std::fs::create_dir_all(&dir).and_then(|()| write_private(&dir.join("login.ron"), &text));
        if let Err(e) = written {
            warn!(dir = %dir.display(), "cannot save login.ron: {e}");
        }
    }

    fn device_key(server: &str, account: &str) -> String {
        format!("{}|{}", server.trim(), account.trim().to_ascii_lowercase())
    }

    pub fn device(&self, server: &str, account: &str) -> Option<DeviceToken> {
        let hex = self.devices.get(&Self::device_key(server, account))?;
        let bytes = from_hex(hex)?;
        Some(DeviceToken(bytes.try_into().ok()?))
    }

    pub fn set_device(&mut self, server: &str, account: &str, token: &DeviceToken) {
        self.devices
            .insert(Self::device_key(server, account), to_hex(&token.0));
    }
}

/// Owner-only file on Unix (it holds device tokens).
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        // An older file keeps its mode on open: set it again.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `keybinds.ron` in the user folder; missing file: the defaults.
pub fn key_map() -> KeyMap {
    let Some(path) = config_dir().map(|d| d.join("keybinds.ron")) else {
        return KeyMap::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let (map, problems) = KeyMap::load(&text);
            for p in problems {
                warn!(path = %path.display(), "{p}");
            }
            map
        }
        Err(_) => KeyMap::default(),
    }
}

/// The log of the HUD: System messages, combat, chat, errors.
#[derive(Default)]
pub struct Log {
    pub lines: Vec<LogLine>,
    /// System messages that arrived since the last frame (System window).
    pub system_new: Vec<String>,
    /// Errors that arrived since the last frame (shown over the hotbar).
    pub error_new: Vec<String>,
}

impl Log {
    pub fn push(&mut self, kind: LineKind, text: String) {
        if kind == LineKind::System {
            self.system_new.push(text.clone());
        }
        if kind == LineKind::Error {
            self.error_new.push(text.clone());
        }
        self.lines.push(LogLine { kind, text });
        if self.lines.len() > LOG_KEEP {
            self.lines.drain(..self.lines.len() - LOG_KEEP);
        }
    }
}

/// `0xRRGGBB` as a color.
fn rgb(c: u32) -> client_ui::Color {
    let ch = |shift: u32| ((c >> shift) & 0xff) as f32 / 255.0;
    [ch(16), ch(8), ch(0), 1.0]
}

/// Creation lists as the screen shows them (names from the text pack).
pub fn look_lists(state: &WorldState, texts: &Texts) -> [LookList; 4] {
    std::array::from_fn(|i| {
        let list = state.appearances().lists.get(i).cloned().unwrap_or_default();
        LookList {
            names: list.iter().map(|o| texts.show(&TextRef::new(o.name))).collect(),
            colors: list.iter().map(|o| rgb(o.color)).collect(),
        }
    })
}

/// Drawn for a player whose body has no sprite of its own, in the skin tone.
pub const PLAYER_SPRITE: &str = "player";

/// The sprites of a look, bottom first, each with the tone that tints it
/// (`None`: as drawn): the body in the skin tone (the game's `player`
/// sprite when the body has none), the body's clothes, then the hairstyle
/// in the hair color. The world view, the creation preview and the
/// character list all draw these, so the preview is the character.
pub fn look_layers(state: &WorldState, look: Look) -> Vec<(&str, Option<u32>)> {
    let lists = &state.appearances().lists;
    let pick = |i: usize, at: u8| {
        lists
            .get(i)
            .and_then(|l| l.get(usize::from(at)).or_else(|| l.first()))
    };
    let skin = pick(1, look.skin).map(|o| o.color);
    let hair = pick(3, look.hair_color).map(|o| o.color);
    let body = pick(0, look.body);
    fn named(s: &str) -> Option<&str> {
        (!s.is_empty()).then_some(s)
    }
    let mut out = vec![(body.and_then(|b| named(&b.sprite)).unwrap_or(PLAYER_SPRITE), skin)];
    if let Some(clothes) = body.and_then(|b| named(&b.clothes)) {
        out.push((clothes, None));
    }
    if let Some(style) = pick(2, look.hair_style).and_then(|o| named(&o.sprite)) {
        out.push((style, hair));
    }
    out
}

/// Creation lists that change nothing drawn, so the creation screen does
/// not offer them: bodies when none has a sprite, hairstyles (and so hair
/// colors) when none has one. Skins always tint the body.
pub fn hidden_lists(state: &WorldState) -> [bool; 4] {
    let lists = &state.appearances().lists;
    let drawn = |i: usize| {
        lists
            .get(i)
            .is_some_and(|l| l.iter().any(|o| !o.sprite.is_empty() || !o.clothes.is_empty()))
    };
    let hair = drawn(2);
    [!drawn(0), false, !hair, !hair]
}

/// A look as the game draws it; `None` before the lists arrive.
pub fn figure_of(state: &WorldState, gfx: &crate::graphics::Graphics, look: Look) -> Option<Figure> {
    let layers = look_layers(state, look)
        .into_iter()
        .filter_map(|(name, tone)| {
            Some(client_ui::FigureLayer {
                sprite: gfx.sheet_sprite(name)?,
                tint: tone.map_or([1.0; 4], rgb),
            })
        })
        .collect();
    let lists = &state.appearances().lists;
    let color = |i: usize, at: u8| {
        lists
            .get(i)
            .and_then(|l| l.get(usize::from(at)).or_else(|| l.first()))
            .map(|o| rgb(o.color))
    };
    Some(Figure {
        layers,
        body: color(0, look.body)?,
        skin: color(1, look.skin)?,
        hair: color(3, look.hair_color),
        broad: look.body % 2 == 1,
    })
}

/// Skin tone of a look in the world view (`None`: lists not here yet).
pub fn skin_of(state: &WorldState, look: Look) -> Option<u32> {
    let list = state.appearances().lists.get(1)?;
    list.get(usize::from(look.skin))
        .or_else(|| list.first())
        .map(|o| o.color)
}

fn name_of(state: &WorldState, texts: &Texts, name: &EntityName) -> String {
    let _ = state;
    match name {
        EntityName::Player(n) => n.as_str().to_owned(),
        EntityName::Npc(t) | EntityName::Monster(t) | EntityName::Node(t) => texts.show(&TextRef::new(*t)),
    }
}

pub fn entity_name(state: &WorldState, texts: &Texts, id: base::EntityId) -> String {
    state
        .entity(id)
        .map_or_else(|| format!("#{}", id.get()), |e| name_of(state, texts, &e.name))
}

/// Label of a key for the hotbar (`Digit1` → `1`).
pub fn key_label(code: &str) -> String {
    code.strip_prefix("Digit")
        .or_else(|| code.strip_prefix("Key"))
        .unwrap_or(code)
        .to_owned()
}

/// Seconds of `ticks`, with one decimal (`7.5`).
fn seconds(ticks: f64) -> Arg {
    Arg::Fixed(Fixed::from_millis(
        (ticks * TICK_MS).round().min(f64::from(i32::MAX)) as i32,
    ))
}

/// The hotbar slot of a technique: name, tooltip, cooldown and whether it
/// can be used now.
/// What a hotbar slot holds: the techniques of the sheet, in order, then
/// the usable consumables of the backpack (phase 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotbarEntry {
    Technique(base::TechniqueId),
    /// `(backpack slot holding it, item)`; `None`: none left, the slot
    /// stays in the bar, dimmed, until more arrive.
    Item(Option<u16>, base::ItemId),
}

/// The entry of hotbar slot `i`, the same way the model and the keys see it:
/// the player's layout, with techniques the character knows and
/// consumables found in the backpack.
pub fn hotbar_entry(state: &WorldState, bar: &Hotbar, i: usize) -> Option<HotbarEntry> {
    match bar.get(i)? {
        Bind::Technique(t) => {
            let t = base::TechniqueId::new(t);
            let known = state.stats()?.techniques.iter().any(|(x, _)| *x == t);
            known.then_some(HotbarEntry::Technique(t))
        }
        Bind::Item(item) => {
            let item = base::ItemId::new(item);
            state.item(item)?.use_effect?;
            let slot = state.inventory().and_then(|inv| {
                inv.slots
                    .iter()
                    .position(|s| s.as_ref().is_some_and(|s| s.item == item))
                    .and_then(|p| u16::try_from(p).ok())
            });
            Some(HotbarEntry::Item(slot, item))
        }
    }
}

/// What a backpack slot holds, as a hotbar binding, if it can go there.
pub fn bag_bind(state: &WorldState, slot: u16) -> Option<Bind> {
    let held = state.inventory()?.slots.get(usize::from(slot))?.as_ref()?;
    state.item(held.item)?.use_effect?;
    Some(Bind::Item(held.item.get()))
}

/// A consumable in the hotbar (phase 7).
fn hotbar_item(
    state: &WorldState,
    texts: &Texts,
    ui: &UiText,
    slot: u16,
    item: base::ItemId,
    now_ms: f64,
    gfx: &crate::graphics::Graphics,
) -> Option<HotbarSlot> {
    let _ = (now_ms, slot);
    let def = state.item(item)?;
    let use_effect = def.use_effect?;
    let count = state
        .inventory()
        .and_then(|i| {
            i.slots
                .iter()
                .flatten()
                .filter(|s| s.item == item)
                .map(|s| s.qty)
                .reduce(u32::saturating_add)
        })
        .unwrap_or(0);
    // Nothing left: the slot shows as empty (the bar forgets it on its next sync).
    if count == 0 {
        return None;
    }
    let mut tooltip = vec![texts.show(&TextRef::new(def.description))];
    if use_effect.hp > 0 {
        tooltip.push(ui.t_args("ui.item.use_hp", &[Arg::Int(i64::from(use_effect.hp))]));
    }
    if use_effect.vigor > 0 {
        tooltip.push(ui.t_args("ui.item.use_vigor", &[Arg::Int(i64::from(use_effect.vigor))]));
    }
    if use_effect.revive_permille > 0 {
        tooltip.push(ui.t_args(
            "ui.item.use_revive",
            &[Arg::Int(i64::from(use_effect.revive_permille / 10))],
        ));
    }
    tooltip.push(ui.t_args("ui.hotbar.count", &[Arg::Int(i64::from(count))]));
    Some(HotbarSlot {
        name: texts.show(&TextRef::new(def.name)),
        tooltip,
        cooldown: 0.0,
        cooldown_text: None,
        usable: count > 0,
        icon: gfx.icon_uv(state.icon_name(def.icon), "generic_item"),
        count: Some(count.to_string()),
    })
}

fn hotbar_slot(
    state: &WorldState,
    texts: &Texts,
    ui: &UiText,
    (t, level): (base::TechniqueId, u8),
    has_target: bool,
    now_ms: f64,
    gfx: &crate::graphics::Graphics,
) -> Option<HotbarSlot> {
    let info = state.technique_info(t)?;
    let s = state.stats()?;
    let mut tooltip = vec![
        ui.t_args("ui.technique.level", &[Arg::Int(i64::from(level))]),
        texts.show(&TextRef::new(info.description)),
    ];
    let grown = |base: u32| {
        base.saturating_add(
            info.growth_permille
                .saturating_mul(u32::from(level.saturating_sub(1))),
        )
    };
    // Support needs no enemy; a heal falls back on oneself.
    let support = info.heal_permille > 0 || info.revive_permille > 0 || info.buff_ticks > 0;
    let has_target = has_target || support;
    if info.damage_permille > 0 {
        let damage = grown(info.damage_permille);
        tooltip.push(ui.t_args("ui.technique.damage", &[Arg::Int(i64::from(damage / 10))]));
    }
    if info.heal_permille > 0 {
        let heal = grown(info.heal_permille).min(1000);
        tooltip.push(ui.t_args("ui.technique.heal", &[Arg::Int(i64::from(heal / 10))]));
    }
    if info.buff_ticks > 0 {
        // The vow; faith (through light) adds to it on the server.
        tooltip.push(ui.t_args(
            "ui.technique.buff_defense",
            &[
                Arg::Int(i64::from(grown(info.buff_permille) / 10)),
                seconds(f64::from(info.buff_ticks)),
            ],
        ));
    }
    if info.revive_permille > 0 {
        tooltip.push(ui.t_args(
            "ui.technique.revive",
            &[Arg::Int(i64::from(info.revive_permille / 10))],
        ));
    }
    if info.vigor > 0 {
        tooltip.push(ui.t_args("ui.technique.vigor", &[Arg::Int(i64::from(info.vigor))]));
    }
    if info.mp > 0 {
        tooltip.push(ui.t_args("ui.technique.mp", &[Arg::Int(i64::from(info.mp))]));
    }
    tooltip.push(ui.t_args("ui.tip.cooldown", &[seconds(f64::from(info.cooldown_ticks))]));
    tooltip.push(ui.t_args("ui.technique.range", &[Arg::Int(i64::from(info.range))]));
    let fits = state.weapon_fits(t);
    if !fits {
        tooltip.push(match info.proficiency.and_then(|p| state.proficiency_info(p)) {
            Some(p) => ui.t_args(
                "ui.technique.needs_weapon",
                &[Arg::Str(texts.show(&TextRef::new(p.name)))],
            ),
            None => ui.t("ui.technique.needs_hands"),
        });
    }
    if !has_target {
        tooltip.push(ui.t("ui.technique.no_target"));
    }
    let left = state.cooldown_left_ms(t, now_ms);
    let total = (f64::from(info.cooldown_ticks) * TICK_MS).max(left).max(1.0);
    Some(HotbarSlot {
        name: texts.show(&TextRef::new(info.name)),
        tooltip,
        cooldown: (left / total) as f32,
        cooldown_text: (left > 0.0).then(|| format!("{:.0}", (left / 1000.0).ceil())),
        usable: left <= 0.0 && s.vigor >= info.vigor && s.mp >= info.mp && has_target && fits,
        icon: gfx.icon_uv(state.icon_name(info.icon), "generic_technique"),
        count: None,
    })
}

/// The hidden bonus of a piece once appraised (`+3 Strength +1 Agility`).
/// A title's description and what it gives, for its tooltip.
fn title_tip(state: &WorldState, texts: &Texts, ui: &UiText, t: base::TitleId) -> Vec<String> {
    use protocol::TitleBonusInfo as B;
    let mut lines: Vec<String> = state
        .title_info(t)
        .and_then(|i| i.description)
        .map(|d| texts.show(&TextRef::new(d)))
        .into_iter()
        .collect();
    let int = |v: u16| Arg::Int(i64::from(v));
    let until = |key: &str, v: u16, level: Option<u8>| match level {
        Some(l) => ui.t_args(&format!("{key}_until"), &[int(v), Arg::Int(i64::from(l))]),
        None => ui.t_args(key, &[int(v)]),
    };
    for b in state.title_bonuses(t) {
        lines.push(match b {
            B::Attribute(a, v) => ui.t_args(
                "ui.bonus.attribute",
                &[int(*v), Arg::Str(attribute_label(ui, *a))],
            ),
            B::MaxHp(v) => ui.t_args("ui.bonus.max_hp", &[Arg::Int(i64::from(*v))]),
            B::MaxVigor(v) => ui.t_args("ui.bonus.max_vigor", &[Arg::Int(i64::from(*v))]),
            B::DamagePercent(v, l) => until("ui.bonus.damage", *v, *l),
            B::XpPercent(v, l) => until("ui.bonus.xp", *v, *l),
            B::MaxMpPercent(v, l) => until("ui.bonus.max_mp", *v, *l),
            B::Special(name, v) => ui.t_args(
                "ui.bonus.special",
                &[int(u16::from(*v)), Arg::Str(texts.show(&TextRef::new(*name)))],
            ),
        });
    }
    lines
}

fn appraised_line(ui: &UiText, p: &protocol::InstanceInfo) -> Option<String> {
    p.hidden.as_ref().map(|bonus| {
        bonus
            .iter()
            .map(|(a, v)| format!("+{v} {}", attribute_label(ui, *a)))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Phase 11: the light an item gives and how fast it burns.
fn light_lines(d: &protocol::ItemInfo, ui: &UiText) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(l) = d.light {
        out.push(ui.t_args("ui.item.light", &[Arg::Int(i64::from(l.radius))]));
    }
    if d.burn > 0 {
        out.push(ui.t_args("ui.item.burn", &[Arg::Int(i64::from(d.burn))]));
    }
    out
}

/// Description and what using an item does, for its tooltip.
fn item_details(
    state: &WorldState,
    texts: &Texts,
    ui: &UiText,
    item: base::ItemId,
    unique: bool,
) -> (String, Vec<String>, bool) {
    let Some(d) = state.item(item) else {
        return (String::new(), Vec::new(), false);
    };
    let mut details = Vec::new();
    if d.kind == ItemKind::Quest {
        details.push(ui.t("ui.item.quest"));
    }
    if let Some(u) = d.use_effect {
        if u.hp > 0 {
            details.push(ui.t_args("ui.item.use_hp", &[Arg::Int(i64::from(u.hp))]));
        }
        if u.vigor > 0 {
            details.push(ui.t_args("ui.item.use_vigor", &[Arg::Int(i64::from(u.vigor))]));
        }
        if u.mp > 0 {
            details.push(ui.t_args("ui.item.use_mp", &[Arg::Int(i64::from(u.mp))]));
        }
        if u.revive_permille > 0 {
            details.push(ui.t_args(
                "ui.item.use_revive",
                &[Arg::Int(i64::from(u.revive_permille / 10))],
            ));
        }
        if u.cooldown_ticks > 0 {
            details.push(ui.t_args("ui.tip.cooldown", &[seconds(f64::from(u.cooldown_ticks))]));
        }
        details.push(ui.t("ui.item.click_use"));
    }
    details.extend(light_lines(d, ui));
    if unique {
        details.push(ui.t("ui.item.click_equip"));
    }
    (
        texts.show(&TextRef::new(d.description)),
        details,
        d.use_effect.is_some(),
    )
}

/// Names, health bars and quest markers over the beings on screen.
pub fn overhead(
    state: &WorldState,
    texts: &Texts,
    target: Option<EntityId>,
    (cfg, cam): (&RenderConfig, &Camera),
    now_ms: f64,
) -> Vec<Overhead> {
    let me = state.my_id();
    state
        .entities()
        .filter(|e| Some(e.id) != me)
        .filter_map(|e| {
            let (x, y) = e.visual_pos(now_ms);
            let (sx, sy) = tile_to_screen(cfg, cam, x as f32, y as f32);
            if sx < 0.0 || sy < 0.0 || sx > cam.screen.0 || sy > cam.screen.1 {
                return None;
            }
            let is_target = Some(e.id) == target;
            let hurt = e.health < 100;
            // A quest comes first; a shop shows when there is no quest.
            let marker = state
                .marker(e.id)
                .map(|m| match m {
                    QuestMarker::Available => MarkerKind::Available,
                    QuestMarker::Talk => MarkerKind::Talk,
                    QuestMarker::Ready => MarkerKind::Ready,
                })
                .or_else(|| state.keeps_shop(e.id).then_some(MarkerKind::Shop));
            let (name, health) = match e.kind {
                // A node wears its name so the player knows what it is.
                EntityKind::Npc | EntityKind::Node => (true, None),
                EntityKind::Player => (true, (hurt || is_target).then_some(e.health)),
                EntityKind::Monster => (is_target || hurt, (hurt || is_target).then_some(e.health)),
            };
            // Over the head of the figure (about two tile heights up).
            Some(Overhead {
                at: (sx, sy - cfg.tile_h * cam.zoom * 1.1),
                name: name.then(|| name_of(state, texts, &e.name)),
                health,
                marker,
                target: is_target,
            })
        })
        .collect()
}

/// A blow worth a rising number, waiting to fade.
#[derive(Clone, Debug)]
pub struct Float {
    pub on: EntityId,
    pub text: String,
    pub kind: FloatKind,
    pub crit: bool,
    pub born_ms: f64,
    /// An emote with a picture (protocol 20).
    pub icon: Option<[f32; 4]>,
}

/// How long a damage number stays.
pub const FLOAT_MS: f64 = 1200.0;

/// How long an emote bubble stays.
pub const EMOTE_MS: f64 = 2600.0;

impl Float {
    /// How long this float stays on screen.
    pub fn lasts_ms(&self) -> f64 {
        if self.kind == FloatKind::Emote {
            EMOTE_MS
        } else {
            FLOAT_MS
        }
    }
}

/// Damage numbers at the beings they belong to.
pub fn floats(
    state: &WorldState,
    list: &[Float],
    (cfg, cam): (&RenderConfig, &Camera),
    now_ms: f64,
) -> Vec<FloatText> {
    list.iter()
        .filter_map(|f| {
            let e = state.entity(f.on)?;
            let (x, y) = e.visual_pos(now_ms);
            let (sx, sy) = tile_to_screen(cfg, cam, x as f32, y as f32);
            Some(FloatText {
                at: (sx, sy - cfg.tile_h * cam.zoom * 1.6),
                text: f.text.clone(),
                kind: f.kind,
                crit: f.crit,
                age: ((now_ms - f.born_ms) / f.lasts_ms()) as f32,
                icon: f.icon,
            })
        })
        .collect()
}

pub struct ModelInput<'a> {
    pub keys: &'a KeyMap,
    /// Icons (phase 6b).
    pub gfx: &'a crate::graphics::Graphics,
    pub target: Option<EntityId>,
    pub now_ms: f64,
    /// The player's hotbar layout.
    pub bar: &'a Hotbar,
    pub pending: client_ui::Pending,
}

pub fn model(
    state: &WorldState,
    texts: &Texts,
    ui_text: &UiText,
    log: &mut Log,
    input: &ModelInput<'_>,
) -> HudModel {
    let (keys, target, now_ms) = (input.keys, input.target, input.now_ms);
    let has_target = target.is_some_and(|t| state.entity(t).is_some_and(|e| e.health > 0));
    let show = |id: base::TextId| texts.show(&TextRef::new(id));
    let mut m = HudModel {
        log: log.lines.clone(),
        system: std::mem::take(&mut log.system_new),
        errors: std::mem::take(&mut log.error_new),
        help: help(keys, ui_text),
        hotbar_keys: (0..10)
            .map(|i| {
                keys.chord_of(client_ui::Action::Hotbar(i))
                    .map_or_else(String::new, |c| key_label(&c.key))
            })
            .collect(),
        ..HudModel::default()
    };
    if let Some(me) = state.me() {
        m.name = name_of(state, texts, &me.name);
    }
    if let Some(s) = state.stats() {
        m.level = s.level;
        m.hp = (u64::from(s.hp), u64::from(s.max_hp));
        m.vigor = (u64::from(s.vigor), u64::from(s.max_vigor));
        m.mp = (u64::from(s.mp), u64::from(s.max_mp));
        m.xp = (s.xp, s.xp_next);
        m.fallen = s.fallen;
        // Techniques first, then the consumables of the backpack: the bar
        // holds both.
        m.hotbar = (0..10)
            .map(|i| match hotbar_entry(state, input.bar, i) {
                Some(HotbarEntry::Technique(t)) => {
                    let level = s.techniques.iter().find(|(x, _)| *x == t).map_or(1, |(_, l)| *l);
                    hotbar_slot(state, texts, ui_text, (t, level), has_target, now_ms, input.gfx)
                }
                Some(HotbarEntry::Item(slot, item)) => hotbar_item(
                    state,
                    texts,
                    ui_text,
                    slot.unwrap_or(u16::MAX),
                    item,
                    now_ms,
                    input.gfx,
                ),
                None => None,
            })
            .collect();
        // Window K: every technique known, with the same tooltip as the bar.
        m.techniques = s
            .techniques
            .iter()
            .filter_map(|&(t, level)| {
                let slot = hotbar_slot(state, texts, ui_text, (t, level), true, now_ms, input.gfx)?;
                Some(client_ui::TechniqueModel {
                    id: t,
                    name: slot.name,
                    level,
                    icon: slot.icon,
                    tooltip: slot.tooltip,
                    in_bar: input.bar.contains(Bind::Technique(t.get())),
                })
            })
            .collect();
        m.pending = input.pending;
        m.sheet = SheetModel {
            attributes: s.attributes,
            base_attributes: s.base_attributes,
            title_tips: s
                .titles
                .iter()
                .map(|t| title_tip(state, texts, ui_text, *t))
                .collect(),
            free_points: s.free_points,
            class: s.class.and_then(|c| state.class_info(c)).map(|i| show(i.name)),
            classes_available: s
                .classes_available
                .iter()
                .filter_map(|c| state.class_info(*c))
                .map(|i| show(i.name))
                .collect(),
            titles: s
                .titles
                .iter()
                .filter_map(|t| state.title_info(*t))
                .map(|i| show(i.name))
                .collect(),
            proficiencies: s
                .proficiencies
                .iter()
                .filter_map(|(p, v)| state.proficiency_info(*p).map(|i| (show(i.name), *v)))
                .collect(),
            defense: s.defense,
            specials: s.specials.iter().map(|(n, v)| (show(*n), *v)).collect(),
        };
        m.equipped = s
            .equipment
            .iter()
            .map(|(slot, item)| {
                let def = state.item(*item);
                let slot = match slot {
                    protocol::EquipSlot::Weapon => 0,
                    protocol::EquipSlot::Armor => 1,
                    protocol::EquipSlot::Offhand => 2,
                };
                // The server lists the worn pieces weapon first, then armor.
                let piece = state.worn().iter().find(|p| p.slot == u16::from(slot));
                client_ui::EquippedModel {
                    slot,
                    name: def.map_or_else(|| format!("#{}", item.get()), |d| show(d.name)),
                    icon: input
                        .gfx
                        .icon_uv(def.and_then(|d| state.icon_name(d.icon)), "generic_item"),
                    details: def
                        .map(|d| show(d.description))
                        .into_iter()
                        .chain(def.map(|d| light_lines(d, ui_text)).into_iter().flatten())
                        .chain(
                            piece
                                .map(|p| crate::social::piece_lines(state, texts, ui_text, p, *item))
                                .into_iter()
                                .flatten(),
                        )
                        // The appraised bonus stays in sight once worn.
                        .chain(piece.and_then(|p| appraised_line(ui_text, p)))
                        .collect(),
                    rarity: piece
                        .map_or_else(|| def.map_or(base::Rarity::Common, |d| d.rarity), |p| p.rarity),
                    durability: piece.map(|p| p.durability).filter(|(_, max)| *max > 0),
                }
            })
            .collect();
    }
    if let Some(inv) = state.inventory() {
        m.coins = inv.coins;
        m.bag = inv
            .slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                slot.as_ref().map(|s| {
                    let inst = state.instances().iter().find(|inst| usize::from(inst.slot) == i);
                    let (description, mut details, usable) =
                        item_details(state, texts, ui_text, s.item, inst.is_some());
                    // Phase 7 and 8: what is left of this very piece, who made
                    // it, and whether the open shop deals in it. The prices
                    // stay on the server: a shop varies them by city and the
                    // System line says what was paid.
                    let def = state.item(s.item);
                    let shop = state.shop();
                    let worn = inst.map(|i| i.durability).filter(|(_, max)| *max > 0);
                    if let Some(p) = inst {
                        details.extend(crate::social::piece_lines(state, texts, ui_text, p, s.item));
                    }
                    let can_sell =
                        shop.is_some() && def.is_some_and(|d| d.value > 0 && d.kind != ItemKind::Quest);
                    let can_repair = shop.is_some_and(|sh| sh.repair_permille > 0)
                        && worn.is_some_and(|(cur, max)| cur < max);
                    // Only a piece not appraised yet; a production class
                    // appraises it alone, the others at a shop.
                    let unappraised = inst.is_some_and(|i| i.hidden.is_none());
                    BagSlot {
                        durability: worn,
                        can_sell,
                        can_repair,
                        can_appraise: unappraised && state.stats().is_some_and(|s| s.appraises),
                        can_shop_appraise: unappraised && shop.is_some_and(|sh| sh.appraise_permille > 0),
                        can_self_repair: worn.is_some_and(|(cur, max)| cur < max)
                            && state.stats().is_some_and(|s| s.self_repairs),
                        icon: input.gfx.icon_uv(
                            state.item(s.item).and_then(|d| state.icon_name(d.icon)),
                            "generic_item",
                        ),
                        description,
                        details,
                        usable,
                        name: state
                            .item(s.item)
                            .map_or_else(|| format!("#{}", s.item.get()), |d| show(d.name)),
                        count: s.qty,
                        // A crafted piece carries its own rolled rarity.
                        rarity: inst
                            .map_or_else(|| def.map_or(base::Rarity::Common, |d| d.rarity), |i| i.rarity),
                        unique: inst.is_some(),
                        appraised: inst.and_then(|i| appraised_line(ui_text, i)),
                    }
                })
            })
            .collect();
    }
    if let Some(t) = target.and_then(|id| state.entity(id)) {
        m.target = Some(TargetModel {
            name: name_of(state, texts, &t.name),
            level: t.level,
            health: t.health,
        });
    }
    if let Some(d) = state.dialogue() {
        m.dialogue = Some((
            texts.show(&d.text),
            d.options.iter().map(|o| texts.show(o)).collect(),
        ));
        let esc = keys
            .chord_of(client_ui::Action::Menu)
            .map_or_else(|| "Esc".to_owned(), |c| key_label(&c.key));
        m.dialogue_close = ui_text.t_args("ui.dialogue.close", &[Arg::Str(esc)]);
    }
    m.quests = quests(state, texts, ui_text);
    m.minimap = minimap(state);
    m.shop = shop_model(state, texts, ui_text, input.gfx);
    m.party = crate::social::party(state, texts);
    m.emotes = crate::social::emotes(state, texts, input.gfx);
    m.support = client_ui::Support {
        heal: state.stats().is_some_and(|s| {
            s.techniques
                .iter()
                .any(|(t, _)| state.technique_info(*t).is_some_and(|i| i.heal_permille > 0))
        }),
    };
    m.invite = crate::social::invite(state, texts, ui_text);
    m.trade = crate::social::trade(state, texts, ui_text, input.gfx);
    m.recipes = recipe_models(state, texts, ui_text, input.gfx);
    m.no_recipes = ui_text.t("ui.crafting.none");
    m.menu = menu(&m, keys, input.gfx);
    m
}

/// The help window: what each key does (with the player's own keys) and tips.
fn help(keys: &client_ui::KeyMap, ui: &UiText) -> client_ui::HelpModel {
    use client_ui::Action as A;
    let key = |a: A| keys.chord_of(a).map_or_else(String::new, |c| key_label(&c.key));
    let mut rows = Vec::new();
    let walk: Vec<String> = [A::MoveUp, A::MoveLeft, A::MoveDown, A::MoveRight]
        .into_iter()
        .map(key)
        .filter(|k| !k.is_empty())
        .collect();
    if !walk.is_empty() {
        rows.push((ui.t("ui.help.move"), walk.join(" ")));
    }
    rows.push((
        ui.t("ui.help.hotbar"),
        format!("{}-{}", key(A::Hotbar(0)), key(A::Hotbar(9))),
    ));
    for (a, k) in [
        (A::TargetNext, "ui.help.target_next"),
        (A::TargetClear, "ui.help.target_clear"),
        (A::Interact, "ui.help.interact"),
        (A::Stop, "ui.help.stop"),
        (A::ChatOpen, "ui.help.chat_open"),
        (A::OpenInventory, "ui.help.open_inventory"),
        (A::OpenSheet, "ui.help.open_sheet"),
        (A::OpenTechniques, "ui.help.open_techniques"),
        (A::OpenQuests, "ui.help.open_quests"),
        (A::OpenCrafting, "ui.help.open_crafting"),
        (A::OpenHelp, "ui.help.open_help"),
        (A::OpenOptions, "ui.help.open_options"),
        (A::ToggleMute, "ui.help.toggle_mute"),
    ] {
        let label = key(a);
        if !label.is_empty() {
            rows.push((ui.t(k), label));
        }
    }
    client_ui::HelpModel {
        keys: rows,
        tips: [
            "ui.help.tip_left",
            "ui.help.tip_right",
            "ui.help.tip_wheel",
            "ui.help.tip_drag",
            "ui.help.tip_markers",
            "ui.help.tip_chat",
        ]
        .into_iter()
        .map(|k| ui.t(k))
        .collect(),
    }
}

/// The menu bar: one button per window, with its icon and key. The
/// crafting button shows up once the character knows a recipe.
fn menu(m: &HudModel, keys: &client_ui::KeyMap, gfx: &crate::graphics::Graphics) -> Vec<MenuButton> {
    use client_ui::Action as A;
    let alert_sheet = m.sheet.free_points > 0 || !m.sheet.classes_available.is_empty();
    [
        (MenuWindow::Inventory, A::OpenInventory, "ui_inventory", false),
        (MenuWindow::Sheet, A::OpenSheet, "ui_sheet", alert_sheet),
        (MenuWindow::Techniques, A::OpenTechniques, "ui_techniques", false),
        (MenuWindow::Quests, A::OpenQuests, "ui_quests", false),
        (MenuWindow::Crafting, A::OpenCrafting, "ui_crafting", false),
        (MenuWindow::Help, A::OpenHelp, "ui_help", false),
        (MenuWindow::Options, A::OpenOptions, "ui_options", false),
    ]
    .into_iter()
    .filter(|(w, ..)| *w != MenuWindow::Crafting || !m.recipes.is_empty())
    .map(|(window, action, icon, alert)| MenuButton {
        window,
        icon: gfx.icon_uv(Some(icon), "generic_item"),
        key: keys
            .chord_of(action)
            .map_or_else(String::new, |c| key_label(&c.key)),
        alert,
    })
    .collect()
}

/// The open shop as the window shows it (phase 7).
fn shop_model(
    state: &WorldState,
    texts: &Texts,
    ui: &UiText,
    gfx: &crate::graphics::Graphics,
) -> Option<client_ui::ShopModel> {
    let view = state.shop()?;
    let coins = state.inventory().map_or(0, |i| i.coins);
    let lines = view
        .stock
        .iter()
        .map(|l| {
            let def = state.item(l.item);
            let mut tooltip = Vec::new();
            if let Some(d) = def {
                tooltip.push(texts.show(&TextRef::new(d.description)));
            }
            tooltip.push(ui.t("ui.shop.click_buy"));
            client_ui::ShopLineModel {
                name: def.map_or_else(
                    || format!("#{}", l.item.get()),
                    |d| texts.show(&TextRef::new(d.name)),
                ),
                icon: gfx.icon_uv(def.and_then(|d| state.icon_name(d.icon)), "generic_item"),
                price: l.price,
                qty: l.qty,
                affordable: coins >= l.price,
                tooltip,
                rarity: def.map_or(base::Rarity::Common, |d| d.rarity),
            }
        })
        .collect();
    Some(client_ui::ShopModel {
        title: entity_name(state, texts, view.npc),
        lines,
        repairs: view.repair_permille > 0,
        appraises: view.appraise_permille > 0,
        // The purse counts down from the moment it was sent.
        purse: view.purse.map(|(coins, secs)| {
            ui.t_args(
                "ui.shop.purse",
                &[
                    Arg::Int(i64::try_from(coins).unwrap_or(i64::MAX)),
                    Arg::Int(i64::from(secs.div_ceil(60).max(1))),
                ],
            )
        }),
        coins: ui.t_args(
            "ui.shop.coins",
            &[Arg::Int(i64::try_from(coins).unwrap_or(i64::MAX))],
        ),
        close: ui.t("ui.shop.close"),
    })
}

/// The recipes this character knows (phase 8).
fn recipe_models(
    state: &WorldState,
    texts: &Texts,
    ui: &UiText,
    gfx: &crate::graphics::Graphics,
) -> Vec<client_ui::RecipeModel> {
    let Some(stats) = state.stats() else {
        return Vec::new();
    };
    let count = |item: base::ItemId| {
        state.inventory().map_or(0, |inv| {
            inv.slots
                .iter()
                .flatten()
                .filter(|s| s.item == item)
                .map(|s| s.qty)
                .fold(0u32, u32::saturating_add)
        })
    };
    stats
        .recipes
        .iter()
        .filter_map(|id| {
            let r = state.recipe(*id)?;
            let points = stats
                .proficiencies
                .iter()
                .find(|(p, _)| *p == r.proficiency)
                .map_or(0, |(_, v)| *v);
            let materials: Vec<(String, u32, u32)> = r
                .materials
                .iter()
                .map(|(item, need)| {
                    (
                        state.item(*item).map_or_else(
                            || format!("#{}", item.get()),
                            |d| texts.show(&TextRef::new(d.name)),
                        ),
                        count(*item),
                        *need,
                    )
                })
                .collect();
            let enough = materials.iter().all(|(_, have, need)| have >= need);
            let skilled = points >= r.min_points;
            let result = state.item(r.result);
            let craft = state
                .proficiency_info(r.proficiency)
                .map_or_else(String::new, |i| texts.show(&TextRef::new(i.name)));
            Some(client_ui::RecipeModel {
                id: *id,
                name: texts.show(&TextRef::new(r.name)),
                description: texts.show(&TextRef::new(r.description)),
                icon: gfx.icon_uv(result.and_then(|d| state.icon_name(d.icon)), "generic_item"),
                group: craft,
                kind: result.map_or(ItemKind::Material, |d| d.kind),
                qty: r.qty,
                rarity: result.map_or(base::Rarity::Common, |d| d.rarity),
                materials,
                can: enough && skilled,
                blocked: if skilled {
                    ui.t("ui.crafting.missing")
                } else {
                    ui.t_args(
                        "ui.crafting.proficiency",
                        &[
                            Arg::Str(
                                state
                                    .proficiency_info(r.proficiency)
                                    .map_or_else(String::new, |i| texts.show(&TextRef::new(i.name))),
                            ),
                            Arg::Int(i64::from(r.min_points)),
                        ],
                    )
                },
            })
        })
        .collect()
}

/// One objective written out (`Glowing herb: 1/3`, `Talk to Mira`).
pub fn objective_text(state: &WorldState, texts: &Texts, ui: &UiText, o: &ObjectiveView) -> String {
    match o {
        ObjectiveView::Item { item, have, need } => {
            let name = state.item(*item).map_or_else(
                || format!("#{}", item.get()),
                |d| texts.show(&TextRef::new(d.name)),
            );
            ui.t_args(
                "ui.quests.item",
                &[
                    base::Arg::Str(name),
                    base::Arg::Int(i64::from((*have).min(*need))),
                    base::Arg::Int(i64::from(*need)),
                ],
            )
        }
        ObjectiveView::Talk { npc, .. } => ui.t_args(
            "ui.quests.talk",
            &[base::Arg::Str(texts.show(&TextRef::new(*npc)))],
        ),
    }
}

/// The quest log: active quests first, in the order taken.
fn quests(state: &WorldState, texts: &Texts, ui: &UiText) -> Vec<QuestModel> {
    let mut list: Vec<QuestModel> = state
        .quests()
        .iter()
        .map(|q| QuestModel {
            name: texts.show(&TextRef::new(q.name)),
            description: texts.show(&TextRef::new(q.description)),
            completed: q.state == QuestState::Completed,
            objectives: q
                .objectives
                .iter()
                .map(|o| {
                    let done = match o {
                        ObjectiveView::Item { have, need, .. } => have >= need,
                        ObjectiveView::Talk { done, .. } => *done,
                    };
                    (objective_text(state, texts, ui, o), done)
                })
                .collect(),
            goal: (q.state != QuestState::Completed)
                .then_some(q.goal)
                .flatten()
                .and_then(|g| goal_text(state, ui, g)),
        })
        .collect();
    list.sort_by_key(|q| q.completed);
    list
}

/// "up-right, 40 tiles" toward a quest's next place, as the
/// screen shows it (the map is drawn turned 45 degrees).
fn goal_text(state: &WorldState, ui: &UiText, goal: TilePos) -> Option<String> {
    let me = state.me()?.pos;
    let (dx, dy) = (goal.x - me.x, goal.y - me.y);
    let tiles = me.distance(goal);
    if tiles <= 2 {
        return Some(ui.t("ui.quests.goal_here"));
    }
    let dir = screen_direction(dx, dy);
    Some(ui.t_args(
        "ui.quests.goal",
        &[
            base::Arg::Str(ui.t(&format!("ui.dir.{dir}"))),
            base::Arg::Int(i64::from(tiles)),
        ],
    ))
}

/// One of eight screen directions (`n` is up) for a tile offset: on screen
/// x grows with `dx - dy` and y with `dx + dy` (half as much).
fn screen_direction(dx: i32, dy: i32) -> &'static str {
    let sx = 2.0 * (dx - dy) as f32;
    let sy = (dx + dy) as f32;
    // Clockwise from up, in eighths of a turn.
    let angle = sx.atan2(-sy).to_degrees().rem_euclid(360.0);
    const DIRS: [&str; 8] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
    DIRS.get(((angle + 22.5) / 45.0) as usize % 8)
        .copied()
        .unwrap_or("n")
}

/// Goals of the active quests on the minimap: where they are, or on the
/// edge toward them.
fn goal_dots(state: &WorldState, me: TilePos, half: i32) -> Vec<(i16, i16, Dot)> {
    state
        .quests()
        .iter()
        .filter(|q| q.state != QuestState::Completed)
        .filter_map(|q| q.goal)
        .map(|g| {
            let (dx, dy) = (g.x - me.x, g.y - me.y);
            let far = dx.abs().max(dy.abs());
            let edge = half - 1;
            let (x, y) = if far > edge {
                // Same direction, pulled in to the edge.
                (dx * edge / far, dy * edge / far)
            } else {
                (dx, dy)
            };
            (x as i16, y as i16, Dot::Goal)
        })
        .collect()
}

fn attribute_label(ui: &UiText, a: Attribute) -> String {
    ui.t(&format!("ui.attr.{}", format!("{a:?}").to_lowercase()))
}

fn minimap(state: &WorldState) -> Minimap {
    let Some(me) = state.me() else {
        return Minimap::default();
    };
    let side = MINIMAP_SIDE;
    let half = i32::from(side) / 2;
    let mut tiles = Vec::with_capacity(usize::from(side) * usize::from(side));
    for dy in -half..half {
        for dx in -half..half {
            tiles.push(match state.tile(TilePos::new(me.pos.x + dx, me.pos.y + dy)) {
                None => 0,
                Some(w) if w & TILE_BLOCKS != 0 => 2,
                Some(_) => 1,
            });
        }
    }
    let dots = state
        .entities()
        .filter_map(|e| {
            let (dx, dy) = (e.pos.x - me.pos.x, e.pos.y - me.pos.y);
            if dx.abs() >= half || dy.abs() >= half {
                return None;
            }
            let kind = if e.id == me.id {
                Dot::Me
            } else {
                match e.kind {
                    EntityKind::Player => Dot::Player,
                    EntityKind::Npc if state.marker(e.id).is_some() => Dot::Quest,
                    EntityKind::Npc | EntityKind::Node => Dot::Npc,
                    EntityKind::Monster => Dot::Monster,
                }
            };
            Some((dx as i16, dy as i16, kind))
        })
        .chain(goal_dots(state, me.pos, half))
        .collect();
    Minimap { side, tiles, dots }
}

#[cfg(test)]
mod layer_tests {
    use super::*;
    use base::TextId;
    use protocol::{AppearanceInfo, LookOptionInfo, ServerMsg};

    fn option(color: u32, sprite: &str, clothes: &str) -> LookOptionInfo {
        LookOptionInfo {
            name: TextId::new(1),
            color,
            sprite: sprite.into(),
            clothes: clothes.into(),
        }
    }

    /// The state on the creation screen: the look lists have come with the
    /// session, `Definitions` (sprite numbers) not yet.
    fn world(lists: [Vec<LookOptionInfo>; 4]) -> WorldState {
        let mut w = WorldState::new();
        w.apply(ServerMsg::Appearances(AppearanceInfo { lists }), 0.0);
        w
    }

    #[test]
    fn a_look_is_body_clothes_and_hair_in_their_tones() {
        let w = world([
            vec![option(0, "body_a", "clothes_a")],
            vec![option(0x112233, "", "")],
            vec![option(0, "hair_short", ""), option(0, "", "")],
            vec![option(0x445566, "", "")],
        ]);
        let look = Look::default();
        assert_eq!(
            look_layers(&w, look),
            vec![
                ("body_a", Some(0x112233)),
                ("clothes_a", None),
                ("hair_short", Some(0x445566)),
            ]
        );
        // The second hairstyle has no sprite: bald.
        let bald = Look {
            hair_style: 1,
            ..look
        };
        assert_eq!(look_layers(&w, bald).len(), 2);
        assert_eq!(hidden_lists(&w), [false, false, false, false]);
    }

    #[test]
    fn without_layer_art_the_player_sprite_and_only_skins() {
        let w = world([
            vec![option(0, "", ""), option(0, "", "")],
            vec![option(0x112233, "", "")],
            vec![option(0, "", "")],
            vec![option(0x445566, "", "")],
        ]);
        assert_eq!(
            look_layers(&w, Look::default()),
            vec![(PLAYER_SPRITE, Some(0x112233))]
        );
        assert_eq!(hidden_lists(&w), [true, false, true, true]);
    }
}

//! `Theme`: colors, fonts and sizes of the interface, read from
//! `assets/themes/<id>.ron`. No color, font
//! or size lives in the widget code; new fields come with a default.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// RGBA, components in 0..=1.
pub type Color = [f32; 4];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FontSet {
    pub title: String,
    pub text: String,
    pub numbers: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panel {
    /// Size of the diagonal cut on two corners, in pixels.
    pub diagonal_slash: f32,
    pub border_thickness: f32,
    /// 0 turns the glow off.
    pub border_glow: f32,
    pub background_blur: bool,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            diagonal_slash: 8.0,
            border_thickness: 1.0,
            border_glow: 0.6,
            background_blur: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sizes {
    pub text: f32,
    pub small: f32,
    pub title: f32,
    pub padding: f32,
    pub hotbar_slot: f32,
    pub bar_height: f32,
    pub minimap: f32,
    pub log_lines: u16,
}

impl Default for Sizes {
    fn default() -> Self {
        Sizes {
            text: 16.0,
            small: 13.0,
            title: 22.0,
            padding: 8.0,
            hotbar_slot: 48.0,
            bar_height: 14.0,
            minimap: 160.0,
            log_lines: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemWindow {
    pub prefix_key: String,
    pub enter_anim_ms: u32,
    /// How long a System message stays on screen.
    pub show_ms: u32,
}

impl Default for SystemWindow {
    fn default() -> Self {
        SystemWindow {
            prefix_key: "ui.system.prefix".into(),
            enter_anim_ms: 300,
            show_ms: 4000,
        }
    }
}

/// How the windows open and close: a short fade with a small rise, so a
/// window never pops in or out in one frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowAnim {
    pub open_ms: u32,
    pub close_ms: u32,
    /// How far (in px at scale 1) a window rises while it fades in.
    pub rise: f32,
}

impl Default for WindowAnim {
    fn default() -> Self {
        WindowAnim {
            open_ms: 140,
            close_ms: 110,
            rise: 8.0,
        }
    }
}

/// The theme as written in the file: colors as `#rrggbb` or `#rrggbbaa`.
/// The editor's Theme screen reads and writes this shape; `load_theme`
/// checks it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "Theme")]
pub struct ThemeFile {
    pub id: String,
    pub fonts: FontSet,
    #[serde(default)]
    pub fonts_by_locale: BTreeMap<String, FontSet>,
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub panel: Panel,
    #[serde(default)]
    pub sizes: Sizes,
    #[serde(default)]
    pub system_window: SystemWindow,
    #[serde(default)]
    pub windows: WindowAnim,
}

/// A color the theme names: `fallback` is the color an optional one takes
/// when the file leaves it out (another name, or `#rrggbbaa`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorSpec {
    pub name: &'static str,
    pub fallback: Option<&'static str>,
}

const fn required(name: &'static str) -> ColorSpec {
    ColorSpec { name, fallback: None }
}

const fn optional(name: &'static str, fallback: &'static str) -> ColorSpec {
    ColorSpec {
        name,
        fallback: Some(fallback),
    }
}

/// Every color of the theme, in the order the editor shows them.
pub const COLORS: &[ColorSpec] = &[
    required("background"),
    required("panel"),
    required("border"),
    required("strong_border"),
    required("text"),
    required("dim_text"),
    required("highlight"),
    required("secondary"),
    required("life"),
    required("mana"),
    required("stamina"),
    required("system"),
    optional("error", "life"),
    optional("xp", "secondary"),
    optional("cooldown", "#0000008c"),
    optional("rarity_rare", "mana"),
    optional("rarity_epic", "secondary"),
    optional("rarity_unique", "highlight"),
    optional("rarity_legendary", "stamina"),
    optional("rarity_mythic", "life"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Colors {
    pub background: Color,
    pub panel: Color,
    pub border: Color,
    pub strong_border: Color,
    pub text: Color,
    pub dim_text: Color,
    pub highlight: Color,
    pub secondary: Color,
    pub life: Color,
    pub mana: Color,
    pub stamina: Color,
    pub system: Color,
    /// Optional in the file: `error` (default: `life`), `xp` (default: `secondary`).
    pub error: Color,
    pub xp: Color,
    /// Optional in the file (default: black at 55%): the shade over a
    /// hotbar slot that is cooling down.
    pub cooldown: Color,
    /// Item names by `base::Rarity`, lowest first. Optional in the file:
    /// `rarity_rare`, `rarity_epic`, `rarity_unique`, `rarity_legendary`,
    /// `rarity_mythic` (defaults: mana, secondary, highlight, stamina, life);
    /// common is always `text`.
    pub rarity: [Color; 6],
}

impl Colors {
    /// The color of an item name of this rarity.
    pub fn of_rarity(&self, r: base::Rarity) -> Color {
        self.rarity[r as usize]
    }
}

/// A loaded theme.
#[derive(Clone, Debug, PartialEq)]
pub struct UiTheme {
    pub id: String,
    pub fonts: FontSet,
    pub fonts_by_locale: BTreeMap<String, FontSet>,
    pub colors: Colors,
    pub panel: Panel,
    pub sizes: Sizes,
    pub system_window: SystemWindow,
    pub windows: WindowAnim,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ThemeError(pub String);

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "theme: {}", self.0)
    }
}

impl std::error::Error for ThemeError {}

pub fn parse_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let a = if hex.len() == 8 { byte(6)? } else { 255 };
    Some([
        f32::from(byte(0)?) / 255.0,
        f32::from(byte(2)?) / 255.0,
        f32::from(byte(4)?) / 255.0,
        f32::from(a) / 255.0,
    ])
}

pub fn load_theme(text: &str) -> Result<UiTheme, ThemeError> {
    let t: ThemeFile = ron::from_str(text).map_err(|e| ThemeError(e.to_string()))?;
    check_theme(&t)?;
    let color = |name: &str| -> Result<Color, ThemeError> {
        let v = t
            .colors
            .get(name)
            .ok_or_else(|| ThemeError(format!("missing color `{name}`")))?;
        parse_color(v).ok_or_else(|| ThemeError(format!("color `{name}`: `{v}` is not #rrggbb or #rrggbbaa")))
    };
    let optional = |name: &str, fallback: Color| -> Result<Color, ThemeError> {
        match t.colors.get(name) {
            None => Ok(fallback),
            Some(_) => color(name),
        }
    };
    let black_55 = [0.0, 0.0, 0.0, 140.0 / 255.0];
    let life = color("life")?;
    let secondary = color("secondary")?;
    let colors = Colors {
        background: color("background")?,
        panel: color("panel")?,
        border: color("border")?,
        strong_border: color("strong_border")?,
        text: color("text")?,
        dim_text: color("dim_text")?,
        highlight: color("highlight")?,
        secondary,
        life,
        mana: color("mana")?,
        stamina: color("stamina")?,
        system: color("system")?,
        error: optional("error", life)?,
        xp: optional("xp", secondary)?,
        cooldown: optional("cooldown", black_55)?,
        rarity: [
            color("text")?,
            optional("rarity_rare", color("mana")?)?,
            optional("rarity_epic", secondary)?,
            optional("rarity_unique", color("highlight")?)?,
            optional("rarity_legendary", color("stamina")?)?,
            optional("rarity_mythic", life)?,
        ],
    };
    Ok(UiTheme {
        id: t.id,
        fonts: t.fonts,
        fonts_by_locale: t.fonts_by_locale,
        colors,
        panel: t.panel,
        sizes: t.sizes,
        system_window: t.system_window,
        windows: t.windows,
    })
}

/// The rules a theme file must follow beyond its shape: no unknown color
/// (a typo would be silently ignored), sizes and times that draw something.
/// The game's theme: `assets/theme.ron` names the theme every player sees
/// (the player does not choose; the editor's Theme screen sets it).
pub const GAME_THEME_FILE: &str = "theme.ron";
/// The theme when `theme.ron` is missing or broken.
pub const FALLBACK_THEME: &str = "isekai";

/// `assets/theme.ron`: `(theme: "isekai")`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "GameTheme")]
pub struct GameTheme {
    pub theme: String,
}

/// The theme `theme.ron` names.
pub fn read_game_theme(text: &str) -> Result<String, ThemeError> {
    let g: GameTheme = ron::from_str(text).map_err(|e| ThemeError(e.to_string()))?;
    let ok = !g.theme.is_empty()
        && g.theme
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(g.theme)
    } else {
        Err(ThemeError(format!("`{}` is not a theme name", g.theme)))
    }
}

pub fn check_theme(t: &ThemeFile) -> Result<(), ThemeError> {
    if let Some(n) = t
        .colors
        .keys()
        .find(|n| !COLORS.iter().any(|c| c.name == n.as_str()))
    {
        return Err(ThemeError(format!("unknown color `{n}`")));
    }
    let s = &t.sizes;
    let sizes = [
        ("text", s.text),
        ("small", s.small),
        ("title", s.title),
        ("hotbar_slot", s.hotbar_slot),
        ("bar_height", s.bar_height),
        ("minimap", s.minimap),
    ];
    if let Some((n, _)) = sizes.iter().find(|(_, v)| *v <= 0.0 || *v > 512.0) {
        return Err(ThemeError(format!("size `{n}` must be above 0 and at most 512")));
    }
    if !(0.0..=64.0).contains(&s.padding) {
        return Err(ThemeError("size `padding` must be 0 to 64".into()));
    }
    if s.log_lines == 0 || s.log_lines > 50 {
        return Err(ThemeError("size `log_lines` must be 1 to 50".into()));
    }
    let p = &t.panel;
    if !(0.0..=64.0).contains(&p.diagonal_slash)
        || !(0.0..=16.0).contains(&p.border_thickness)
        || !(0.0..=4.0).contains(&p.border_glow)
    {
        return Err(ThemeError(
            "panel: diagonal_slash 0 to 64, border_thickness 0 to 16, border_glow 0 to 4".into(),
        ));
    }
    let w = &t.windows;
    if w.open_ms > 2000 || w.close_ms > 2000 || !(0.0..=64.0).contains(&w.rise) {
        return Err(ThemeError(
            "windows: open_ms and close_ms at most 2000, rise 0 to 64".into(),
        ));
    }
    let sw = &t.system_window;
    if sw.enter_anim_ms > 5000 || sw.show_ms == 0 || sw.show_ms > 60_000 {
        return Err(ThemeError(
            "system_window: enter_anim_ms at most 5000, show_ms 1 to 60000".into(),
        ));
    }
    Ok(())
}

impl UiTheme {
    /// The font set for a locale code (`"ja"`), else the base one.
    pub fn fonts_for(&self, locale: &str) -> &FontSet {
        self.fonts_by_locale.get(locale).unwrap_or(&self.fonts)
    }

    /// The text fonts of every locale with its own set (CJK).
    pub fn locale_text_fonts(&self) -> impl Iterator<Item = &str> {
        self.fonts_by_locale.values().map(|s| s.text.as_str())
    }
}

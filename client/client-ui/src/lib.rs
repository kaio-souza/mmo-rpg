//! Interface of the client: theme from a file, TTF fonts per locale, the
//! glyph atlas, widgets, HUD and screens. Knows nothing of `WorldState`,
//! `protocol` nor the window: the binary fills `Input` and a model, gets
//! back triangles (`Vertex`) and `UiAction`s.
//!
//!

mod fonts;
mod hud;
mod keys;
mod overlay;
mod paint;
mod screens;
mod texts;
mod theme;
mod ui;

pub use fonts::{ATLAS_SIZE, Fonts, Role};
pub use hud::{
    BagSlot, Dot, EmoteEntry, EquippedModel, HelpModel, HotbarSlot, HudModel, HudState, LineKind, LogLine,
    LogTab, MenuButton, MenuWindow, Minimap, PartyMember, PartyModel, Pending, PlayerMenu, QuestModel,
    RecipeModel, SheetModel, ShopLineModel, ShopMode, ShopModel, SoundModel, Support, TargetModel,
    TechniqueModel, TradeLine, TradeModel, VolumeSlot,
};
pub use keys::{Action, Chord, KeyMap};
pub use overlay::{FloatKind, FloatText, MarkerKind, Overhead};
pub use paint::{Painter, Rect, Vertex};
pub use screens::{
    CharacterCard, CreateModel, Figure, LoginModel, LookList, PasswordModel, Screen, ScreenState,
    SelectModel, figure,
};
pub use texts::{FILES as TEXT_FILES, GAME_ID, UiText, language_names};
pub use theme::{
    COLORS, Color, ColorSpec, Colors, FALLBACK_THEME, FontSet, GAME_THEME_FILE, GameTheme, Panel, Sizes,
    SystemWindow, ThemeError, ThemeFile, UiTheme, WindowAnim, check_theme, load_theme, parse_color,
    read_game_theme,
};
pub use ui::{DragSource, Frame, Input, PlayerAct, UiAction};

/// Theme, fonts, texts and what persists between frames.
pub struct Ui {
    pub theme: UiTheme,
    pub fonts: Fonts,
    pub text: UiText,
    pub hud: HudState,
    /// What was typed on the screens before the game.
    pub screens: ScreenState,
    painter: Painter,
    focus: Option<u32>,
    /// Where each window and HUD part sat last frame (`Frame::place`).
    pub(crate) placed: Vec<(&'static str, Rect)>,
}

/// Result of a frame besides the triangles.
#[derive(Debug, Default)]
pub struct FrameOut {
    pub over_ui: bool,
    pub actions: Vec<UiAction>,
    /// A text field has the keyboard: keys are text, not shortcuts.
    pub typing: bool,
    /// Caret of the focused field, for the IME candidate window.
    pub ime_area: Option<Rect>,
}

impl Ui {
    pub fn new(theme: UiTheme, fonts: Fonts, text: UiText) -> Ui {
        Ui {
            theme,
            fonts,
            text,
            hud: HudState::default(),
            screens: ScreenState::default(),
            painter: Painter::default(),
            focus: None,
            placed: Vec::new(),
        }
    }

    /// Swaps the theme and its fonts (reloaded from disk: no rebuild).
    pub fn set_theme(&mut self, theme: UiTheme, fonts: Fonts) {
        self.theme = theme;
        self.fonts = fonts;
    }

    pub fn set_text(&mut self, text: UiText) {
        self.text = text;
    }

    pub fn clear_focus(&mut self) {
        self.focus = None;
    }

    /// Draws one frame with `draw`; the triangles stay in `vertices()`.
    pub fn frame(&mut self, input: &Input, draw: impl FnOnce(&mut Frame<'_>, &mut HudState)) -> FrameOut {
        self.run(input, |f, hud, _| draw(f, hud))
    }

    /// Draws a screen before the game (login, selection, creation...).
    pub fn screen(&mut self, input: &Input, screen: &Screen) -> FrameOut {
        self.run(input, |f, _, s| s.draw(f, screen))
    }

    fn run(
        &mut self,
        input: &Input,
        draw: impl FnOnce(&mut Frame<'_>, &mut HudState, &mut ScreenState),
    ) -> FrameOut {
        self.painter.clear();
        let mut hud = std::mem::take(&mut self.hud);
        let mut screens = std::mem::take(&mut self.screens);
        let mut f = Frame {
            theme: &self.theme,
            fonts: &mut self.fonts,
            text: &self.text,
            painter: &mut self.painter,
            input,
            focus: &mut self.focus,
            over_ui: false,
            actions: Vec::new(),
            tab_done: false,
            ime_area: None,
            tip: None,
            placed: std::mem::take(&mut self.placed),
        };
        f.placed.clear();
        draw(&mut f, &mut hud, &mut screens);
        if let Some(lines) = f.tip.take() {
            overlay::draw_tooltip(&mut f, &lines);
        }
        let (over_ui, actions, ime_area) = (f.over_ui, f.actions, f.ime_area);
        self.placed = f.placed;
        let out = FrameOut {
            over_ui,
            actions,
            typing: self.focus.is_some(),
            ime_area,
        };
        self.hud = hud;
        self.screens = screens;
        out
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.painter.verts
    }

    pub fn take_atlas(&mut self) -> Option<(u32, &[u8])> {
        self.fonts.take_atlas()
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_layout;

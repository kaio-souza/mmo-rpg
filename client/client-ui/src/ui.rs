//! Immediate-mode drawing context: the binary fills `Input` each frame,
//! screens and HUD draw through `Frame` and return `UiAction`s.

use crate::fonts::{Fonts, Role};
use crate::paint::{Painter, Rect};
use crate::texts::UiText;
use crate::theme::{Color, UiTheme};

/// What happened since the last frame.
#[derive(Clone, Debug, Default)]
pub struct Input {
    /// Window size in physical pixels.
    pub screen: (f32, f32),
    /// Physical pixels per logical pixel (theme sizes are logical).
    pub scale: f32,
    pub mouse: (f32, f32),
    pub clicked: bool,
    pub right_clicked: bool,
    /// The left button is held now.
    pub mouse_down: bool,
    /// The left button went up this frame.
    pub released: bool,
    /// Where the last left press started (drag and tap test against it).
    pub press_at: Option<(f32, f32)>,
    /// Shift is held: bigger amounts (whole stack, five pieces).
    pub shift: bool,
    /// Mouse wheel since the last frame, in lines (positive: away from the
    /// player, up). A window under the mouse scrolls; elsewhere it zooms.
    pub wheel: f32,
    /// Text typed (or committed by the IME) this frame.
    pub typed: String,
    /// IME composition still open (CJK), shown in the focused field.
    pub preedit: String,
    pub backspace: u32,
    pub enter: bool,
    pub escape: bool,
    pub tab: bool,
    pub now_ms: u64,
}

/// Everything the player can do on the interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    Login {
        server: String,
        account: String,
        password: String,
        remember: bool,
    },
    Register {
        server: String,
        account: String,
        password: String,
        remember: bool,
    },
    /// The language picked in the login screen list.
    SetLocale(base::Locale),
    Enter(usize),
    OpenCreate,
    Create {
        name: String,
        look: [u8; 4],
    },
    BackToSelect,
    /// Deletes the character at `index`; `name` typed to confirm.
    DeleteCharacter {
        index: usize,
        name: String,
    },
    CancelQueue,
    OpenChangePassword,
    ChangePassword {
        new: String,
    },
    SendChat(String),
    Hotbar(u8),
    /// Something dropped on hotbar slot `slot`.
    HotbarDrop {
        slot: u8,
        from: DragSource,
    },
    /// A hotbar slot dragged out of the bar: emptied.
    HotbarClear(u8),
    /// A technique clicked in its window: first empty hotbar slot.
    HotbarAdd(base::TechniqueId),
    ChooseClass(usize),
    AddPoint(usize),
    Equip(u16),
    Unequip(u8),
    Appraise(u16),
    /// A production class mends a backpack piece with materials.
    SelfRepair(u16),
    /// Phase 5: uses the consumable of a backpack slot.
    UseItem(u16),
    /// Phase 7: pieces of the open shop's line, by position.
    Buy {
        index: u16,
        qty: u32,
    },
    /// Sells pieces of a backpack slot at the open shop.
    Sell {
        slot: u16,
        qty: u32,
    },
    /// Repairs the equipment of a backpack slot at the open shop.
    Repair(u16),
    /// Alpha: the open shop appraises the piece of a backpack slot.
    ShopAppraise(u16),
    /// Alpha: a backpack slot dropped on another (swap or merge).
    MoveItem {
        from: u16,
        to: u16,
    },
    CloseShop,
    /// Phase 8: crafts one batch of a recipe.
    Craft(base::RecipeId),
    /// Gathers a resource node by entity id.
    Gather(base::EntityId),
    GiveUp,
    /// The trade window: a backpack slot joins my offer (the
    /// whole stack); the offer as it should be now; confirm; leave.
    TradeAdd(u16),
    TradeOffer {
        items: Vec<(u16, u32)>,
        coins: u64,
    },
    TradeConfirm,
    TradeCancel,
    /// The trade window answers the request it shows: yes.
    TradeAccept,
    /// Answers the newest invite or trade request.
    AnswerInvite(bool),
    /// Something done to another player, from their menu (right click on
    /// them, or a click on a group member).
    Player {
        target: base::EntityId,
        name: String,
        act: PlayerAct,
    },
    LeaveParty,
    /// Makes a gesture (the emote button by the log).
    Emote(base::EmoteId),
    /// Dialogue option, by index in the list shown.
    ChooseOption(u16),
    Quit,
    /// Options window: one 10% step of a volume.
    SoundStep {
        slot: crate::VolumeSlot,
        up: bool,
    },
    /// Mute on or off.
    SoundMute,
    /// Plays a sample at the volume set.
    SoundTest,
}

/// What the player menu offers. The server checks every one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAct {
    Trade,
    Invite,
    /// Opens the chat with the whisper to them started.
    Whisper,
    /// Group leader only: passes the lead, or removes them.
    Lead,
    Kick,
    Report,
    /// They are fallen: the revival technique when ready, else
    /// a revival item from the backpack.
    Revive,
    /// A heal technique on them.
    Heal,
}

/// Where a dragged thing came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragSource {
    Hotbar(u8),
    Bag(u16),
    Technique(base::TechniqueId),
}

/// Mouse travel, in physical pixels, that turns a press into a drag.
const DRAG_START: f32 = 6.0;

pub struct Frame<'a> {
    pub theme: &'a UiTheme,
    pub fonts: &'a mut Fonts,
    pub text: &'a UiText,
    pub painter: &'a mut Painter,
    pub input: &'a Input,
    /// Focused text field.
    pub focus: &'a mut Option<u32>,
    /// The mouse is over a panel: the click is not for the world.
    pub over_ui: bool,
    pub actions: Vec<UiAction>,
    /// Tab already moved the focus this frame.
    pub tab_done: bool,
    /// Caret of the focused field: where the IME shows its candidates.
    pub ime_area: Option<Rect>,
    /// The tooltip of this frame, drawn last so no window covers it.
    pub(crate) tip: Option<Vec<(String, Color)>>,
    /// Windows and HUD parts drawn this frame, by name: what the layout
    /// tests check for overlaps.
    pub(crate) placed: Vec<(&'static str, Rect)>,
}

impl Frame<'_> {
    /// Runs `body` as if the mouse were nowhere: no hover, click, wheel or
    /// drag start. For what lies under a window drawn later, so one click
    /// is not taken by both (the dialogue over the shop).
    pub fn without_mouse<R>(&mut self, body: impl FnOnce(&mut Frame<'_>) -> R) -> R {
        let masked = Input {
            mouse: (f32::MIN, f32::MIN),
            clicked: false,
            right_clicked: false,
            mouse_down: false,
            released: false,
            press_at: None,
            wheel: 0.0,
            ..self.input.clone()
        };
        let mut sub = Frame {
            theme: self.theme,
            fonts: &mut *self.fonts,
            text: self.text,
            painter: &mut *self.painter,
            input: &masked,
            focus: &mut *self.focus,
            over_ui: self.over_ui,
            actions: std::mem::take(&mut self.actions),
            tab_done: self.tab_done,
            ime_area: self.ime_area,
            tip: self.tip.take(),
            placed: std::mem::take(&mut self.placed),
        };
        let out = body(&mut sub);
        self.over_ui = sub.over_ui;
        self.actions = sub.actions;
        self.tab_done = sub.tab_done;
        self.ime_area = sub.ime_area;
        self.tip = sub.tip;
        self.placed = sub.placed;
        out
    }

    /// A theme size in physical pixels.
    pub fn px(&self, logical: f32) -> f32 {
        logical * self.input.scale.max(0.5)
    }

    /// A click that went down and up inside `r` without becoming a drag.
    /// Slots that can be dragged act on this instead of on the press.
    pub fn tapped(&self, r: Rect) -> bool {
        self.input.released
            && r.contains(self.input.mouse)
            && self
                .input
                .press_at
                .is_some_and(|p| r.contains(p) && !self.moved(p))
    }

    /// The press that started in `r` is being dragged away now.
    pub fn drag_started(&self, r: Rect) -> bool {
        self.input.mouse_down
            && self
                .input
                .press_at
                .is_some_and(|p| r.contains(p) && self.moved(p))
    }

    fn moved(&self, p: (f32, f32)) -> bool {
        let (dx, dy) = (self.input.mouse.0 - p.0, self.input.mouse.1 - p.1);
        dx * dx + dy * dy > self.px(DRAG_START).powi(2)
    }

    pub fn tr(&self, key: &str) -> String {
        self.text.t(key)
    }

    pub fn tr_args(&self, key: &str, args: &[base::Arg]) -> String {
        self.text.t_args(key, args)
    }

    /// Records where a window or HUD part sits this frame.
    pub(crate) fn place(&mut self, name: &'static str, r: Rect) {
        self.placed.push((name, r));
    }

    pub fn panel(&mut self, r: Rect, strong: bool) {
        if r.contains(self.input.mouse) {
            self.over_ui = true;
        }
        self.painter.panel(self.theme, r, strong);
    }

    pub fn label(&mut self, role: Role, size: f32, at: (f32, f32), color: Color, text: &str) -> f32 {
        let px = self.px(size);
        self.painter.text(self.fonts, role, px, at, color, text)
    }

    pub fn label_fit(&mut self, role: Role, size: f32, at: (f32, f32), width: f32, color: Color, text: &str) {
        let px = self.px(size);
        self.painter.text_fit(
            self.fonts,
            (role, px),
            Rect::new(at.0, at.1, width, 0.0),
            color,
            text,
        );
    }

    /// Text centered in `r`.
    pub fn label_center(&mut self, role: Role, size: f32, r: Rect, color: Color, text: &str) {
        let px = self.px(size);
        let w = self.fonts.measure(role, px, text).min(r.w);
        let h = self.fonts.line_height(role, px);
        let at = (r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0);
        self.painter.text_fit(
            self.fonts,
            (role, px),
            Rect::new(at.0, at.1, r.w, 0.0),
            color,
            text,
        );
    }

    pub fn line_height(&self, role: Role, size: f32) -> f32 {
        self.fonts.line_height(role, self.px(size))
    }

    pub fn button(&mut self, r: Rect, label: &str) -> bool {
        let hover = r.contains(self.input.mouse);
        self.panel(r, hover);
        let color = if hover {
            self.theme.colors.highlight
        } else {
            self.theme.colors.text
        };
        self.label_center(Role::Title, self.theme.sizes.text, r, color, label);
        hover && self.input.clicked
    }

    /// The main button of a window: always lit, so it is found first.
    pub fn button_primary(&mut self, r: Rect, label: &str) -> bool {
        let hover = r.contains(self.input.mouse);
        self.panel(r, true);
        let mut glow = self.theme.colors.highlight;
        glow[3] *= if hover { 0.32 } else { 0.18 };
        let inset = self.px(2.0);
        self.painter.rect(r.shrink(inset), glow);
        let color = self.theme.colors.highlight;
        self.label_center(Role::Title, self.theme.sizes.text, r, color, label);
        hover && self.input.clicked
    }

    /// A small triangle pointing down (a list that opens) or up (open).
    pub fn chevron(&mut self, r: Rect, open: bool, color: Color) {
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let s = r.w.min(r.h) * 0.18;
        if open {
            self.painter.tri(
                (cx - s, cy + s / 2.0),
                (cx + s, cy + s / 2.0),
                (cx, cy - s / 2.0),
                color,
            );
        } else {
            self.painter.tri(
                (cx - s, cy - s / 2.0),
                (cx + s, cy - s / 2.0),
                (cx, cy + s / 2.0),
                color,
            );
        }
    }

    /// A one-line text field; returns true when Enter is pressed in it.
    /// `secret` shows dots. Tab moves the focus to `id + 1`.
    pub fn field(&mut self, id: u32, r: Rect, value: &mut String, secret: bool, max_chars: usize) -> bool {
        if self.input.clicked && r.contains(self.input.mouse) {
            *self.focus = Some(id);
        }
        let focused = *self.focus == Some(id);
        let mut entered = false;
        if focused {
            for c in self.input.typed.chars().filter(|c| !c.is_control()) {
                if value.chars().count() < max_chars {
                    value.push(c);
                }
            }
            for _ in 0..self.input.backspace {
                value.pop();
            }
            entered = self.input.enter;
        }
        self.panel(r, focused);
        let shown: String = if secret {
            "•".repeat(value.chars().count())
        } else {
            value.clone()
        };
        let pad = self.px(self.theme.sizes.padding);
        let size = self.theme.sizes.text;
        let h = self.line_height(Role::Text, size);
        let at = (r.x + pad, r.y + (r.h - h) / 2.0);
        let w = self.label(Role::Text, size, at, self.theme.colors.text, &shown);
        if focused {
            let mut x = at.0 + w;
            if !self.input.preedit.is_empty() {
                x += self.label(
                    Role::Text,
                    size,
                    (x, at.1),
                    self.theme.colors.highlight,
                    &self.input.preedit.clone(),
                );
            }
            let caret = Rect::new(x + 1.0, at.1, self.px(1.5), h);
            self.ime_area = Some(caret);
            if (self.input.now_ms / 500).is_multiple_of(2) {
                self.painter.rect(caret, self.theme.colors.highlight);
            }
        }
        if focused && self.input.tab && !self.tab_done {
            *self.focus = Some(id + 1);
            self.tab_done = true;
        }
        entered
    }
}

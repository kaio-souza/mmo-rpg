//! Screens before the game: login,
//! character selection, creation, entry queue, password change, and a
//! waiting message. Same rules as the HUD: the binary builds a model, the
//! screen draws it and returns `UiAction`s; typed values stay here between
//! frames.

use crate::fonts::Role;
use crate::paint::Rect;
use crate::theme::Color;
use crate::ui::{Frame, UiAction};
use base::{Arg, CharacterName};

/// Text field ids (focus and Tab order).
const LOGIN_SERVER: u32 = 1;
const LOGIN_ACCOUNT: u32 = 2;
const LOGIN_PASSWORD: u32 = 3;
const CREATE_NAME: u32 = 10;
const DELETE_NAME: u32 = 11;
const PASSWORD_NEW: u32 = 20;
const PASSWORD_REPEAT: u32 = 21;
/// Id of the language list (the server list uses `LOGIN_SERVER`).
const LIST_LOCALE: u32 = 30;

/// Where the login lists open from, filled while the form is drawn.
#[derive(Default)]
struct Anchors {
    locale: Rect,
    server: Rect,
}

/// Colors of the placeholder figure until the sprites of phase 6: clothes
/// (body type), skin and hair; `hair: None` is bald.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Figure {
    pub body: Color,
    pub skin: Color,
    pub hair: Option<Color>,
    /// Second body type: a broader figure.
    pub broad: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoginModel {
    /// Servers from the configuration, the official one first.
    pub servers: Vec<String>,
    /// The language in use, lit in the language list.
    pub locale: base::Locale,
    pub error: Option<String>,
    /// Waiting for the server: buttons are off.
    pub busy: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterCard {
    pub name: String,
    pub level: u8,
    pub figure: Option<Figure>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectModel {
    pub characters: Vec<CharacterCard>,
    /// Characters an account may have.
    pub max: usize,
    pub error: Option<String>,
    pub busy: bool,
}

/// One creation list: option names and their figure color.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LookList {
    pub names: Vec<String>,
    pub colors: Vec<Color>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateModel {
    /// Body, skin, hairstyle, hair color.
    pub lists: [LookList; 4],
    pub error: Option<String>,
    pub busy: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PasswordModel {
    /// Came from a temporary password: no way back.
    pub required: bool,
    pub error: Option<String>,
    pub busy: bool,
}

/// What the player typed, kept between frames.
#[derive(Clone, Debug, Default)]
pub struct ScreenState {
    pub server: String,
    pub account: String,
    pub password: String,
    pub remember: bool,
    /// Selected character.
    pub selected: usize,
    pub name: String,
    pub look: [u8; 4],
    pub new_password: String,
    pub repeat_password: String,
    /// The selection asks for the name again before deleting.
    pub confirm_delete: bool,
    pub delete_name: String,
    /// The login list open now (language or server), if any.
    pub open_list: Option<u32>,
    /// The picture behind the screens (sheet coordinates, width / height);
    /// without it, the summoning circle.
    pub backdrop: Option<([f32; 4], f32)>,
}

/// Which screen to draw.
#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Login(LoginModel),
    Select(SelectModel),
    Create(CreateModel),
    Queue {
        position: u32,
        estimate_secs: u32,
    },
    Password(PasswordModel),
    /// A title and a line, no buttons (connecting, reconnecting).
    Wait {
        title: String,
        text: String,
    },
}

impl ScreenState {
    pub fn draw(&mut self, f: &mut Frame<'_>, screen: &Screen) {
        match self.backdrop {
            Some((uv, aspect)) => picture(f, uv, aspect),
            None => background(f),
        }
        match screen {
            Screen::Login(m) => self.login(f, m),
            Screen::Select(m) => self.select(f, m),
            Screen::Create(m) => self.create(f, m),
            Screen::Queue {
                position,
                estimate_secs,
            } => queue(f, *position, *estimate_secs),
            Screen::Password(m) => self.password_change(f, m),
            Screen::Wait { title, text } => {
                let r = window(f, 420.0, 2);
                let (pad, lh) = metrics(f);
                header(f, r, title);
                let c = f.theme.colors.text;
                f.label_fit(
                    Role::Text,
                    f.theme.sizes.text,
                    (r.x + pad, r.y + 2.0 * pad + lh),
                    r.w - 2.0 * pad,
                    c,
                    text,
                );
            }
        }
    }

    /// A random look (the "Random" button). `seed` comes from the clock:
    /// the choice is the player's to keep or change, nothing depends on it.
    pub fn randomize(&mut self, lists: &[LookList; 4], seed: u64) {
        let mut x = seed | 1;
        for (slot, list) in self.look.iter_mut().zip(lists) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let n = list.names.len().max(1) as u64;
            *slot = u8::try_from(x % n).unwrap_or(0);
        }
    }

    fn login(&mut self, f: &mut Frame<'_>, m: &LoginModel) {
        if f.focus.is_none() {
            *f.focus = Some(if self.account.is_empty() {
                LOGIN_ACCOUNT
            } else {
                LOGIN_PASSWORD
            });
        }
        if m.busy {
            self.open_list = None;
        }
        // An open list sits over the form: the form gets no mouse, and a
        // click outside the list only closes it.
        let mut anchors = Anchors::default();
        let was_open = self.open_list.is_some();
        if was_open {
            f.without_mouse(|f| self.login_form(f, m, &mut anchors));
        } else {
            self.login_form(f, m, &mut anchors);
        }
        // The click that opened a list is not also a click on it.
        if !was_open && self.open_list.is_some() {
            let open = self.open_list;
            f.without_mouse(|f| self.login_lists(f, m, &anchors));
            self.open_list = open;
            return;
        }
        self.login_lists(f, m, &anchors);
    }

    fn login_lists(&mut self, f: &mut Frame<'_>, m: &LoginModel, anchors: &Anchors) {
        match self.open_list {
            Some(LIST_LOCALE) => {
                let names = crate::texts::language_names();
                let chosen = names.iter().position(|(l, _)| *l == m.locale);
                let items: Vec<&str> = names.iter().map(|(_, n)| n.as_str()).collect();
                if let Some(i) = self.drop_list(f, anchors.locale, &items, chosen) {
                    f.actions.push(UiAction::SetLocale(names[i].0));
                }
            }
            Some(LOGIN_SERVER) => {
                let items: Vec<&str> = m.servers.iter().map(String::as_str).collect();
                let chosen = m.servers.iter().position(|s| *s == self.server);
                if let Some(i) = self.drop_list(f, anchors.server, &items, chosen) {
                    self.server = m.servers[i].clone();
                }
            }
            _ => {}
        }
    }

    /// The login form under any open list.
    fn login_form(&mut self, f: &mut Frame<'_>, m: &LoginModel, anchors: &mut Anchors) {
        let (pad, lh) = metrics(f);
        let field_h = lh + 2.0 * pad;
        let small = f.line_height(Role::Text, f.theme.sizes.small);
        // Language row, three labeled fields, remember, buttons, status.
        let h = 2.0 * pad + field_h + 3.0 * (small + field_h + pad) + 2.0 * (field_h + pad) + lh + pad;
        let r = window_h(f, 460.0, h);
        let title = f.tr("ui.game.title");
        header(f, r, &title);
        let inner = r.w - 2.0 * pad;
        // Language: a list at the top right, the current one in its own name.
        let names = crate::texts::language_names();
        let current = names
            .iter()
            .find(|(l, _)| *l == m.locale)
            .map_or_else(|| f.tr("ui.locale.name"), |(_, n)| n.clone());
        let lang_w = (inner * 0.45).max(f.px(150.0));
        anchors.locale = Rect::new(r.x + r.w - pad - lang_w, r.y + pad, lang_w, field_h);
        if self.select_box(f, anchors.locale, &current, self.open_list == Some(LIST_LOCALE)) && !m.busy {
            self.open_list = Some(LIST_LOCALE);
        }
        let mut y = r.y + 2.0 * pad + field_h;
        // Server: typed, or picked from the configured list.
        y = labeled(f, r, y, "ui.login.server");
        let list_w = if m.servers.is_empty() { 0.0 } else { field_h };
        f.field(
            LOGIN_SERVER,
            Rect::new(r.x + pad, y, inner - list_w, field_h),
            &mut self.server,
            false,
            128,
        );
        if list_w > 0.0 {
            anchors.server = Rect::new(r.x + pad, y, inner, field_h);
            let b = Rect::new(r.x + r.w - pad - list_w, y, list_w, field_h);
            let open = self.open_list == Some(LOGIN_SERVER);
            if f.button(b, "") {
                self.open_list = Some(LOGIN_SERVER);
            }
            let c = f.theme.colors.text;
            f.chevron(b, open, c);
        }
        y += field_h + pad;
        y = labeled(f, r, y, "ui.login.account");
        let enter_a = f.field(
            LOGIN_ACCOUNT,
            Rect::new(r.x + pad, y, inner, field_h),
            &mut self.account,
            false,
            24,
        );
        y += field_h + pad;
        y = labeled(f, r, y, "ui.login.password");
        let enter_p = f.field(
            LOGIN_PASSWORD,
            Rect::new(r.x + pad, y, inner, field_h),
            &mut self.password,
            true,
            128,
        );
        y += field_h + pad;
        // "Remember account": the box and its label toggle it.
        let box_r = Rect::new(r.x + pad, y + (field_h - lh) / 2.0, lh, lh);
        f.panel(box_r, self.remember);
        if self.remember {
            f.painter.rect(box_r.shrink(lh / 4.0), f.theme.colors.highlight);
        }
        let label = f.tr("ui.login.remember");
        let w = f.label(
            Role::Text,
            f.theme.sizes.text,
            (box_r.x + lh + pad, box_r.y),
            f.theme.colors.text,
            &label,
        );
        let toggle = Rect::new(r.x + pad, y, lh + pad + w, field_h);
        if f.input.clicked && toggle.contains(f.input.mouse) {
            self.remember = !self.remember;
        }
        y += field_h + pad;
        // Enter is the main button; a new account is the other way in.
        let half = (inner - pad) / 2.0;
        let enter = f.tr("ui.login.enter");
        let register = f.tr("ui.login.register");
        let login = f.button_primary(Rect::new(r.x + pad, y, half, field_h), &enter) || enter_a || enter_p;
        let create = f.button(Rect::new(r.x + 2.0 * pad + half, y, half, field_h), &register);
        y += field_h + pad;
        if m.busy {
            let t = f.tr("ui.login.connecting");
            f.label(
                Role::Text,
                f.theme.sizes.text,
                (r.x + pad, y),
                f.theme.colors.dim_text,
                &t,
            );
        } else if let Some(e) = &m.error {
            f.label_fit(
                Role::Text,
                f.theme.sizes.text,
                (r.x + pad, y),
                inner,
                f.theme.colors.error,
                e,
            );
        }
        if (login || create) && !m.busy {
            let (server, account, password, remember) = (
                self.server.trim().to_owned(),
                self.account.trim().to_owned(),
                self.password.clone(),
                self.remember,
            );
            f.actions.push(if create {
                UiAction::Register {
                    server,
                    account,
                    password,
                    remember,
                }
            } else {
                UiAction::Login {
                    server,
                    account,
                    password,
                    remember,
                }
            });
        }
    }

    /// A closed list: the current value and an arrow. True when clicked.
    fn select_box(&mut self, f: &mut Frame<'_>, r: Rect, value: &str, open: bool) -> bool {
        let hover = r.contains(f.input.mouse);
        f.panel(r, hover || open);
        let (pad, lh) = metrics(f);
        let arrow = Rect::new(r.x + r.w - r.h, r.y, r.h, r.h);
        let c = if hover {
            f.theme.colors.highlight
        } else {
            f.theme.colors.text
        };
        f.label_fit(
            Role::Text,
            f.theme.sizes.text,
            (r.x + pad, r.y + (r.h - lh) / 2.0),
            r.w - r.h - pad,
            c,
            value,
        );
        f.chevron(arrow, open, c);
        hover && f.input.clicked
    }

    /// The open list under `anchor`: one row per item, the chosen one lit.
    /// Returns the clicked row; a click elsewhere or Escape closes it.
    fn drop_list(
        &mut self,
        f: &mut Frame<'_>,
        anchor: Rect,
        items: &[&str],
        chosen: Option<usize>,
    ) -> Option<usize> {
        let (pad, lh) = metrics(f);
        let row_h = lh + pad;
        let h = row_h * items.len() as f32 + pad;
        // Below the anchor, or above it when the screen ends first.
        let below = anchor.y + anchor.h;
        let y = if below + h > f.input.screen.1 && anchor.y - h >= 0.0 {
            anchor.y - h
        } else {
            below
        };
        let list = Rect::new(anchor.x, y, anchor.w, h);
        // Opaque: the form under it must not show through.
        f.painter.rect(list, f.theme.colors.background);
        f.panel(list, true);
        let mut picked = None;
        for (i, name) in items.iter().enumerate() {
            let row = Rect::new(
                list.x + pad / 2.0,
                list.y + pad / 2.0 + row_h * i as f32,
                list.w - pad,
                row_h,
            );
            let hover = row.contains(f.input.mouse);
            if Some(i) == chosen || hover {
                let mut back = f.theme.colors.highlight;
                back[3] *= if hover { 0.22 } else { 0.12 };
                f.painter.rect(row, back);
            }
            let c = if Some(i) == chosen {
                f.theme.colors.highlight
            } else {
                f.theme.colors.text
            };
            f.label_fit(
                Role::Text,
                f.theme.sizes.text,
                (row.x + pad / 2.0, row.y + pad / 2.0),
                row.w - pad,
                c,
                name,
            );
            if hover && f.input.clicked {
                picked = Some(i);
            }
        }
        if picked.is_some() || f.input.escape || (f.input.clicked && !list.contains(f.input.mouse)) {
            self.open_list = None;
        }
        picked
    }

    fn select(&mut self, f: &mut Frame<'_>, m: &SelectModel) {
        let rows = m.max.max(m.characters.len()).max(1);
        if m.characters.is_empty() {
            self.confirm_delete = false;
        }
        let confirm = self.confirm_delete;
        let r = window(f, 560.0, rows as u32 + if confirm { 5 } else { 3 });
        let (pad, lh) = metrics(f);
        let title = f.tr("ui.select.title");
        header(f, r, &title);
        let row_h = lh * 2.0 + pad;
        let inner = r.w - 2.0 * pad;
        let mut y = r.y + 2.0 * pad + lh;
        if m.characters.is_empty() {
            let t = f.tr("ui.select.empty");
            f.label(
                Role::Text,
                f.theme.sizes.text,
                (r.x + pad, y),
                f.theme.colors.dim_text,
                &t,
            );
        }
        self.selected = self.selected.min(m.characters.len().saturating_sub(1));
        let mut play = false;
        for (i, card) in m.characters.iter().enumerate() {
            let row = Rect::new(r.x + pad, y, inner, row_h);
            let chosen = i == self.selected;
            f.panel(row, chosen);
            if f.input.clicked && row.contains(f.input.mouse) {
                if chosen {
                    play = true; // second click plays
                }
                self.selected = i;
            }
            if let Some(fig) = &card.figure {
                figure(
                    f,
                    Rect::new(
                        row.x + pad / 2.0,
                        row.y + pad / 4.0,
                        row_h * 0.6,
                        row_h - pad / 2.0,
                    ),
                    fig,
                    0,
                );
            }
            let x = row.x + row_h * 0.6 + pad;
            let c = f.theme.colors.clone();
            f.label(
                Role::Title,
                f.theme.sizes.text,
                (x, row.y + pad / 2.0),
                if chosen { c.highlight } else { c.text },
                &card.name,
            );
            let level = f.text.t_args("ui.hud.level", &[Arg::Int(i64::from(card.level))]);
            f.label(
                Role::Numbers,
                f.theme.sizes.small,
                (x, row.y + pad / 2.0 + lh),
                c.dim_text,
                &level,
            );
            y += row_h + pad / 2.0;
        }
        y = r.y + r.h - 2.0 * (lh + 2.0 * pad) - pad;
        let field_h = lh + 2.0 * pad;
        if confirm {
            // "Type Kai to delete it": the field, then Delete and Cancel.
            let chosen = m
                .characters
                .get(self.selected)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let mut top = y - 2.0 * (field_h + pad) - lh;
            let ask = f.text.t_args("ui.select.delete_confirm", &[Arg::Str(chosen)]);
            f.label_fit(
                Role::Text,
                f.theme.sizes.small,
                (r.x + pad, top),
                inner,
                f.theme.colors.dim_text,
                &ask,
            );
            top += lh;
            if f.focus.is_none() {
                *f.focus = Some(DELETE_NAME);
            }
            let entered = f.field(
                DELETE_NAME,
                Rect::new(r.x + pad, top, inner, field_h),
                &mut self.delete_name,
                false,
                16,
            );
            top += field_h + pad;
            let half = (inner - pad) / 2.0;
            let (forever, cancel) = (f.tr("ui.select.delete_forever"), f.tr("ui.select.delete_cancel"));
            let yes = f.button(Rect::new(r.x + pad, top, half, field_h), &forever);
            let no = f.button(Rect::new(r.x + 2.0 * pad + half, top, half, field_h), &cancel);
            if !m.busy && (yes || entered) && !self.delete_name.trim().is_empty() {
                f.actions.push(UiAction::DeleteCharacter {
                    index: self.selected,
                    name: self.delete_name.trim().to_owned(),
                });
                self.confirm_delete = false;
                *f.focus = None;
            }
            if no {
                self.confirm_delete = false;
                *f.focus = None;
            }
        }
        let quarter = (inner - 3.0 * pad) / 4.0;
        let labels = [
            f.tr("ui.select.enter"),
            f.tr("ui.select.create"),
            f.tr("ui.select.delete"),
            f.tr("ui.select.change_password"),
        ];
        let mut clicked = [false; 4];
        for (i, label) in labels.iter().enumerate() {
            clicked[i] = f.button(
                Rect::new(r.x + pad + (quarter + pad) * i as f32, y, quarter, field_h),
                label,
            );
        }
        if f.input.enter && f.focus.is_none() && !confirm {
            play = true;
        }
        if !m.busy && !confirm {
            if (clicked[0] || play) && !m.characters.is_empty() {
                f.actions.push(UiAction::Enter(self.selected));
            }
            if clicked[1] && m.characters.len() < m.max {
                self.name.clear();
                f.actions.push(UiAction::OpenCreate);
            }
            if clicked[2] && !m.characters.is_empty() {
                self.delete_name.clear();
                self.confirm_delete = true;
                *f.focus = None;
            }
            if clicked[3] {
                self.new_password.clear();
                self.repeat_password.clear();
                f.actions.push(UiAction::OpenChangePassword);
            }
        }
        y += field_h + pad;
        status_line(f, r, y, m.busy, m.error.as_deref());
    }

    fn create(&mut self, f: &mut Frame<'_>, m: &CreateModel) {
        if f.focus.is_none() {
            *f.focus = Some(CREATE_NAME);
        }
        let r = window(f, 560.0, 10);
        let (pad, lh) = metrics(f);
        let title = f.tr("ui.create.title");
        header(f, r, &title);
        let field_h = lh + 2.0 * pad;
        // Left: the choices; right: the preview turning through 8 directions.
        let left_w = r.w * 0.6 - 1.5 * pad;
        let mut y = r.y + 2.0 * pad + lh;
        y = labeled(f, r, y, "ui.create.name");
        let entered = f.field(
            CREATE_NAME,
            Rect::new(r.x + pad, y, left_w, field_h),
            &mut self.name,
            false,
            16,
        );
        let valid = CharacterName::try_from(self.name.as_str()).is_ok();
        y += field_h + pad / 2.0;
        if !self.name.is_empty() && !valid {
            let t = f.tr("ui.create.invalid_name");
            f.label_fit(
                Role::Text,
                f.theme.sizes.small,
                (r.x + pad, y),
                left_w,
                f.theme.colors.error,
                &t,
            );
        }
        y += lh + pad / 2.0;
        let keys = [
            "ui.create.body",
            "ui.create.skin",
            "ui.create.hair",
            "ui.create.hair_color",
        ];
        for (i, key) in keys.iter().enumerate() {
            let list = &m.lists[i];
            let n = list.names.len();
            let label = f.tr(key);
            let c = f.theme.colors.clone();
            f.label(
                Role::Text,
                f.theme.sizes.text,
                (r.x + pad, y + pad),
                c.dim_text,
                &label,
            );
            let bx = r.x + pad + left_w * 0.38;
            let bw = field_h;
            let value_w = left_w * 0.62 - 2.0 * bw - pad;
            if f.button(Rect::new(bx, y, bw, field_h), "<") && n > 0 {
                self.look[i] = ((usize::from(self.look[i]) + n - 1) % n) as u8;
            }
            if f.button(Rect::new(bx + bw + value_w + pad, y, bw, field_h), ">") && n > 0 {
                self.look[i] = ((usize::from(self.look[i]) + 1) % n) as u8;
            }
            let value = Rect::new(bx + bw + pad / 2.0, y, value_w, field_h);
            if let Some(color) = list.colors.get(usize::from(self.look[i])) {
                f.painter.rect(
                    Rect::new(value.x, value.y + value.h - f.px(3.0), value.w, f.px(3.0)),
                    *color,
                );
            }
            let name = list
                .names
                .get(usize::from(self.look[i]))
                .cloned()
                .unwrap_or_default();
            f.label_center(Role::Text, f.theme.sizes.text, value, c.text, &name);
            y += field_h + pad / 2.0;
        }
        let preview = Rect::new(
            r.x + r.w * 0.6 + pad / 2.0,
            r.y + 2.0 * pad + lh,
            r.w * 0.4 - 1.5 * pad,
            y - r.y - 3.0 * pad - lh,
        );
        f.panel(preview, false);
        let fig = self.figure(m);
        let dir = ((f.input.now_ms / 600) % 8) as u8;
        let w = preview.h.min(preview.w) * 0.45;
        figure(
            f,
            Rect::new(
                preview.x + (preview.w - w) / 2.0,
                preview.y + preview.h * 0.12,
                w,
                preview.h * 0.76,
            ),
            &fig,
            dir,
        );
        y += pad / 2.0;
        let inner = r.w - 2.0 * pad;
        let third = (inner - 2.0 * pad) / 3.0;
        let random = f.tr("ui.create.random");
        let confirm = f.tr("ui.create.confirm");
        let back = f.tr("ui.create.back");
        if f.button(Rect::new(r.x + pad, y, third, field_h), &random) {
            self.randomize(&m.lists, f.input.now_ms.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        }
        let create = f.button(Rect::new(r.x + 2.0 * pad + third, y, third, field_h), &confirm) || entered;
        if f.button(Rect::new(r.x + 3.0 * pad + 2.0 * third, y, third, field_h), &back) && !m.busy {
            f.actions.push(UiAction::BackToSelect);
        }
        if create && valid && !m.busy {
            f.actions.push(UiAction::Create {
                name: self.name.clone(),
                look: self.look,
            });
        }
        y += field_h + pad;
        status_line(f, r, y, m.busy, m.error.as_deref());
    }

    /// The figure of the look being chosen.
    pub fn figure(&self, m: &CreateModel) -> Figure {
        let color = |list: &LookList, i: u8| {
            list.colors
                .get(usize::from(i))
                .copied()
                .unwrap_or([0.5, 0.5, 0.5, 1.0])
        };
        Figure {
            body: color(&m.lists[0], self.look[0]),
            skin: color(&m.lists[1], self.look[1]),
            hair: Some(color(&m.lists[3], self.look[3])),
            broad: self.look[0] % 2 == 1,
        }
    }

    fn password_change(&mut self, f: &mut Frame<'_>, m: &PasswordModel) {
        if f.focus.is_none() {
            *f.focus = Some(PASSWORD_NEW);
        }
        let r = window(f, 460.0, 7);
        let (pad, lh) = metrics(f);
        let title = f.tr("ui.password.title");
        header(f, r, &title);
        let field_h = lh + 2.0 * pad;
        let inner = r.w - 2.0 * pad;
        let mut y = r.y + 2.0 * pad + lh;
        if m.required {
            let t = f.tr("ui.password.temporary");
            f.label_fit(
                Role::Text,
                f.theme.sizes.small,
                (r.x + pad, y),
                inner,
                f.theme.colors.system,
                &t,
            );
            y += lh + pad / 2.0;
        }
        y = labeled(f, r, y, "ui.password.new");
        let a = f.field(
            PASSWORD_NEW,
            Rect::new(r.x + pad, y, inner, field_h),
            &mut self.new_password,
            true,
            128,
        );
        y += field_h + pad;
        y = labeled(f, r, y, "ui.password.repeat");
        let b = f.field(
            PASSWORD_REPEAT,
            Rect::new(r.x + pad, y, inner, field_h),
            &mut self.repeat_password,
            true,
            128,
        );
        y += field_h + pad;
        let half = (inner - pad) / 2.0;
        let confirm = f.tr("ui.password.confirm");
        let back = f.tr("ui.create.back");
        let send = f.button(Rect::new(r.x + pad, y, half, field_h), &confirm) || a || b;
        if !m.required && f.button(Rect::new(r.x + 2.0 * pad + half, y, half, field_h), &back) && !m.busy {
            f.actions.push(UiAction::BackToSelect);
        }
        y += field_h + pad;
        let local_error = if self.new_password.chars().count() < 8 && !self.new_password.is_empty() {
            Some(f.tr("ui.password.too_short"))
        } else if !self.repeat_password.is_empty() && self.repeat_password != self.new_password {
            Some(f.tr("ui.password.mismatch"))
        } else {
            None
        };
        let error = local_error.clone().or_else(|| m.error.clone());
        status_line(f, r, y, m.busy, error.as_deref());
        if send && local_error.is_none() && !self.new_password.is_empty() && !m.busy {
            f.actions.push(UiAction::ChangePassword {
                new: self.new_password.clone(),
            });
        }
    }
}

fn queue(f: &mut Frame<'_>, position: u32, estimate_secs: u32) {
    let r = window(f, 420.0, 4);
    let (pad, lh) = metrics(f);
    let title = f.tr("ui.queue.title");
    header(f, r, &title);
    let text = f.text.t_args(
        "ui.queue.position",
        &[Arg::Int(i64::from(position)), Arg::Int(i64::from(estimate_secs))],
    );
    let y = r.y + 2.0 * pad + lh;
    f.label_fit(
        Role::Numbers,
        f.theme.sizes.text,
        (r.x + pad, y),
        r.w - 2.0 * pad,
        f.theme.colors.text,
        &text,
    );
    let field_h = lh + 2.0 * pad;
    let cancel = f.tr("ui.queue.cancel");
    if f.button(
        Rect::new(r.x + pad, y + lh + 2.0 * pad, r.w - 2.0 * pad, field_h),
        &cancel,
    ) {
        f.actions.push(UiAction::CancelQueue);
    }
}

fn metrics(f: &Frame<'_>) -> (f32, f32) {
    let pad = f.px(f.theme.sizes.padding);
    (pad, f.line_height(Role::Text, f.theme.sizes.text))
}

/// A centered window `width` logical pixels wide with room for `rows` rows.
fn window(f: &mut Frame<'_>, width: f32, rows: u32) -> Rect {
    let (pad, lh) = metrics(f);
    let h = (lh + 2.0 * pad + pad) * (rows as f32 + 1.0) + lh;
    window_h(f, width, h)
}

/// A centered window `width` logical pixels wide and `h` physical high.
fn window_h(f: &mut Frame<'_>, width: f32, h: f32) -> Rect {
    let (pad, _) = metrics(f);
    let w = f.px(width).min(f.input.screen.0 - 2.0 * pad);
    let r = Rect::new(
        (f.input.screen.0 - w) / 2.0,
        ((f.input.screen.1 - h) / 2.0).max(pad),
        w,
        h,
    );
    f.panel(r, true);
    r
}

fn header(f: &mut Frame<'_>, r: Rect, title: &str) {
    let (pad, _) = metrics(f);
    f.label(
        Role::Title,
        f.theme.sizes.title,
        (r.x + pad, r.y + pad),
        f.theme.colors.highlight,
        title,
    );
}

/// A small label above a field; returns where the field goes.
fn labeled(f: &mut Frame<'_>, r: Rect, y: f32, key: &str) -> f32 {
    let label = f.tr(key);
    f.label(
        Role::Text,
        f.theme.sizes.small,
        (r.x + f.px(f.theme.sizes.padding), y),
        f.theme.colors.dim_text,
        &label,
    );
    y + f.line_height(Role::Text, f.theme.sizes.small)
}

fn status_line(f: &mut Frame<'_>, r: Rect, y: f32, busy: bool, error: Option<&str>) {
    let (pad, _) = metrics(f);
    if busy {
        let t = f.tr("ui.login.connecting");
        f.label(
            Role::Text,
            f.theme.sizes.text,
            (r.x + pad, y),
            f.theme.colors.dim_text,
            &t,
        );
    } else if let Some(e) = error {
        f.label_fit(
            Role::Text,
            f.theme.sizes.text,
            (r.x + pad, y),
            r.w - 2.0 * pad,
            f.theme.colors.error,
            e,
        );
    }
}

/// The summoning circle behind the screens: rings and a hexagram in the
/// theme's accent colors, turning slowly.
/// The login background covering the screen (cropped, never stretched),
/// darkened so the boxes in front stay readable.
fn picture(f: &mut Frame<'_>, uv: [f32; 4], aspect: f32) {
    let (w, h) = f.input.screen;
    f.painter
        .rect(Rect::new(0.0, 0.0, w, h), f.theme.colors.background);
    let [u0, v0, u1, v1] = uv;
    let screen = w / h.max(1.0);
    // Crop the side that is too long for this screen's shape.
    let uv = if aspect > screen {
        let keep = screen / aspect;
        let cut = (u1 - u0) * (1.0 - keep) / 2.0;
        [u0 + cut, v0, u1 - cut, v1]
    } else {
        let keep = aspect / screen.max(f32::EPSILON);
        let cut = (v1 - v0) * (1.0 - keep) / 2.0;
        [u0, v0 + cut, u1, v1 - cut]
    };
    f.painter
        .image(Rect::new(0.0, 0.0, w, h), uv, [1.0, 1.0, 1.0, 1.0]);
    let mut dim = f.theme.colors.background;
    dim[3] = 0.35;
    f.painter.rect(Rect::new(0.0, 0.0, w, h), dim);
}

fn background(f: &mut Frame<'_>) {
    let (w, h) = f.input.screen;
    f.painter
        .rect(Rect::new(0.0, 0.0, w, h), f.theme.colors.background);
    let c = (w / 2.0, h / 2.0);
    let radius = w.min(h) * 0.46;
    let turn = (f.input.now_ms % 120_000) as f32 / 120_000.0 * std::f32::consts::TAU;
    let (thin, thick, mid, line) = (f.px(1.0), f.px(2.0), f.px(1.5), f.px(1.0));
    let mut glow = f.theme.colors.strong_border;
    glow[3] *= 0.25;
    let mut faint = f.theme.colors.secondary;
    faint[3] *= 0.18;
    let painter = &mut *f.painter;
    let mut ring = |r: f32, thick: f32, color: Color| {
        let n = 96;
        for i in 0..n {
            let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let p = |a: f32, r: f32| (c.0 + a.cos() * r, c.1 + a.sin() * r);
            let (o0, o1, i0, i1) = (p(a0, r), p(a1, r), p(a0, r - thick), p(a1, r - thick));
            painter.tri(o0, o1, i0, color);
            painter.tri(i0, o1, i1, color);
        }
    };
    ring(radius, thick, glow);
    ring(radius * 0.93, thin, faint);
    ring(radius * 0.55, mid, glow);
    // Hexagram: two triangles, thin edges.
    for k in 0..2 {
        let pts: Vec<(f32, f32)> = (0..3)
            .map(|i| {
                let a = turn + k as f32 * std::f32::consts::PI / 3.0 + i as f32 * std::f32::consts::TAU / 3.0;
                (c.0 + a.cos() * radius * 0.9, c.1 + a.sin() * radius * 0.9)
            })
            .collect();
        for i in 0..3 {
            let (a, b) = (pts[i], pts[(i + 1) % 3]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (nx, ny) = (-dy / len * line, dx / len * line);
            painter.tri(
                (a.0 + nx, a.1 + ny),
                (b.0 + nx, b.1 + ny),
                (a.0 - nx, a.1 - ny),
                faint,
            );
            painter.tri(
                (a.0 - nx, a.1 - ny),
                (b.0 + nx, b.1 + ny),
                (b.0 - nx, b.1 - ny),
                faint,
            );
        }
    }
}

/// The placeholder character: clothes, head, hair and a mark on the side
/// it faces (`dir` 0 = south, then clockwise in eighths), until the
/// sprites of phase 6.
pub fn figure(f: &mut Frame<'_>, r: Rect, fig: &Figure, dir: u8) {
    let body_w = if fig.broad { r.w * 0.8 } else { r.w * 0.62 };
    let head = r.w * 0.42;
    let cx = r.x + r.w / 2.0;
    let head_r = Rect::new(cx - head / 2.0, r.y + r.h * 0.04, head, head);
    let body_r = Rect::new(
        cx - body_w / 2.0,
        head_r.y + head * 1.05,
        body_w,
        r.h * 0.9 - head * 1.05,
    );
    f.painter.rect(body_r, fig.body);
    f.painter.rect(head_r, fig.skin);
    if let Some(hair) = fig.hair {
        f.painter.rect(
            Rect::new(
                head_r.x - head * 0.06,
                head_r.y - head * 0.1,
                head * 1.12,
                head * 0.38,
            ),
            hair,
        );
    }
    // Facing: a mark on the head toward `dir` (screen directions).
    let a = std::f32::consts::FRAC_PI_2 + f32::from(dir % 8) * std::f32::consts::FRAC_PI_4;
    let (mx, my) = (
        cx + a.cos() * head * 0.28,
        head_r.y + head * 0.6 + a.sin() * head * 0.2,
    );
    let dot = head * 0.16;
    f.painter.rect(
        Rect::new(mx - dot / 2.0, my - dot / 2.0, dot, dot),
        f.theme.colors.background,
    );
}

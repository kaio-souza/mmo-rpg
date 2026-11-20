//! Client binary: main loop, input and wiring of the client crates.
//!
//!
//! Phase 5: the window opens on the login screen; selection, creation and
//! the entry queue are screens too (`flow`). The command line can fill the
//! login and skip the screens (scripts, tests).
//!
//! ```text
//! client [--server HOST:PORT] [--account NAME [--register] [--character NAME]] [--trust-cert FILE]
//! client --dev --server 127.0.0.1:7000 --name alice   # DevLogin, no screens
//! client --dev --server 127.0.0.1:7000                # screens over plain TCP
//! ```

mod chat;
mod flow;
mod game;
mod graphics;
mod hotbar;
mod interface;
mod scene;
mod screenshot;
mod social;
mod sound;
mod text;

use base::{CharacterName, EntityId, Locale, Look, TilePos};
use client_net::TlsConfig;
use client_render::{Camera, ItemDesign, Render, RenderConfig};
use client_state::WorldState;
use client_ui::{CharacterCard, CreateModel, LoginModel, PasswordModel, Screen, SelectModel, UiAction};
use flow::{Flow, Stage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::Ime;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// A game frame later than this after the last is logged.
const SLOW_FRAME_MS: f64 = 120.0;

/// Time spent by one game frame (see `slow frame` in the log).
#[derive(Clone, Copy, Debug, Default)]
struct FrameCost {
    msgs: usize,
    receive_ms: f64,
    frame_ms: f64,
}

/// The password comes from here (scripts) or from the login screen, never
/// from an argument (visible in the process list).
const PASSWORD_ENV: &str = "GAME_PASSWORD";
/// Server shown when neither the command line nor `login.ron` name one.
const DEFAULT_SERVER: &str = "127.0.0.1:7000";

const USAGE: &str = "usage: client [--server HOST:PORT] [--account NAME] [--register] [--character NAME]
                 [--trust-cert FILE] [--change-password] [--locale CODE] [--content DIR] [--assets DIR]
                 [--theme ID] [--screenshot-after SECS]
       client --dev --server HOST:PORT [--name NAME] [--locale CODE] [--content DIR] [--assets DIR]
                 [--theme ID] [--screenshot-after SECS]
                 [--screenshot-after SECS]
With --account and GAME_PASSWORD set, the login screen is filled and sent.
--dev uses plain TCP (server in --dev); with --name it skips the screens (DevLogin).
F12 (or --screenshot-after) saves a PNG in the user folder's screenshots/.
CODE: en, pt-BR, zh, it, id, de, ru, es, ja, ko";

/// Closest zoom in (the wheel).
pub(crate) const MAX_ZOOM: f32 = 3.0;
/// How fast the drawn zoom catches the wheel's (ms to cover ~63%): a
/// notch glides instead of jumping 10% in one frame.
pub(crate) const ZOOM_EASE_MS: f64 = 60.0;
/// Farthest zoom out on a small window.
const MIN_ZOOM: f32 = 0.6;
/// The zoom-out limit never asks for more than this.
const MIN_ZOOM_CAP: f32 = 1.5;
/// View radius before `Welcome` says the real one.
const DEFAULT_VIEW_RADIUS: i32 = 20;

#[derive(Default)]
struct Args {
    server: Option<String>,
    dev: bool,
    name: String,
    account: Option<String>,
    register: bool,
    character: Option<String>,
    trust_cert: Option<String>,
    change_password: bool,
    locale: Option<Locale>,
    /// The game's `content/` (its interface texts are in `content/text`).
    content: String,
    /// Folder with `themes/`, `fonts/` and `text/` (default `assets`).
    assets: String,
    /// Theme to try instead of the game's (`assets/theme.ron`, set in the
    /// editor): for a designer testing a theme, not a player option.
    theme: Option<String>,
    /// Take one screenshot this many seconds after start (for docs and bug
    /// reports), as F12 would.
    screenshot_after: Option<f64>,
}

impl Args {
    fn parse() -> Result<Args, String> {
        let mut args = Args {
            content: "content".into(),
            assets: "assets".into(),
            theme: None,
            ..Args::default()
        };
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let mut value = || it.next().ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--server" => args.server = Some(value()?),
                "--dev" => args.dev = true,
                "--name" => args.name = value()?,
                "--account" => args.account = Some(value()?),
                "--register" => args.register = true,
                "--character" => args.character = Some(value()?),
                "--trust-cert" => args.trust_cert = Some(value()?),
                "--change-password" => args.change_password = true,
                "--content" => args.content = value()?,
                "--assets" => args.assets = value()?,
                "--theme" => args.theme = Some(value()?),
                "--screenshot-after" => {
                    let v = value()?;
                    args.screenshot_after = Some(
                        v.parse::<f64>()
                            .ok()
                            .filter(|s| s.is_finite() && *s >= 0.0)
                            .ok_or(format!("--screenshot-after needs seconds, got {v}\n{USAGE}"))?,
                    );
                }
                "--locale" => {
                    let code = value()?;
                    args.locale = Some(
                        code.parse()
                            .map_err(|_| format!("unknown locale {code}\n{USAGE}"))?,
                    );
                }
                "--help" | "-h" => return Err(USAGE.into()),
                other => return Err(format!("unknown argument {other}\n{USAGE}")),
            }
        }
        Ok(args)
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
    if let Err(e) = run() {
        error!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse()?;
    // The game's interface texts, read once; without them nothing is shown.
    client_ui::set_game(
        std::path::Path::new(&args.content),
        std::path::Path::new(&args.assets),
    )?;
    let config = interface::LoginConfig::load();
    let locale = args
        .locale
        .or_else(|| config.locale.as_deref().and_then(|c| c.parse().ok()))
        // Only a language of the login list (a required one).
        .filter(|l| client_ui::language_names().iter().any(|(n, _)| n == l))
        .unwrap_or(Locale::EN);
    let server = args
        .server
        .clone()
        .or_else(|| config.servers.first().cloned())
        .unwrap_or_else(|| DEFAULT_SERVER.into());
    let name_rule = |_| "invalid name (3-16 chars of [A-Za-z0-9_])".to_string();
    let tls = if args.dev {
        None
    } else {
        let extra = match &args.trust_cert {
            Some(path) => Some(std::fs::read(path).map_err(|e| {
                format!("--trust-cert {path}: {e}; the server writes it the first time it runs without --dev")
            })?),
            None => None,
        };
        Some(TlsConfig::new(extra.as_deref()).map_err(|e| e.to_string())?)
    };
    let auto = args
        .character
        .as_deref()
        .map(CharacterName::try_from)
        .transpose()
        .map_err(name_rule)?;
    let flow = Flow::new(server.clone(), tls, locale).with_options(auto, args.change_password);

    let event_loop = EventLoop::new().map_err(|e| format!("cannot open the event loop: {e}"))?;
    let mut assets = interface::Assets::new(args.assets.clone().into(), args.theme.clone(), locale);
    let mut ui = assets.ui()?;
    ui.screens.server = server;
    // A server given on the command line, or the only configured one, is
    // not asked on the login screen.
    ui.screens.server_fixed = args.server.is_some() || config.servers.len() == 1;
    ui.screens.account = args
        .account
        .clone()
        .or(config.account.clone())
        .unwrap_or_default();
    ui.screens.remember = config.account.is_some();
    let graphics = graphics::Graphics::new(args.assets.clone().into());
    let mut app = App::new(flow, text::Texts::new(locale), ui, assets, config);
    app.graphics = graphics;
    app.sound = sound::Sounds::open(std::path::Path::new(&args.assets));
    app.shot_at_ms = args.screenshot_after.map(|s| s * 1000.0);
    if args.dev && !args.name.is_empty() {
        let name = CharacterName::try_from(args.name.as_str()).map_err(name_rule)?;
        app.flow.dev(name);
    } else if let (Some(account), Ok(password)) = (&args.account, std::env::var(PASSWORD_ENV)) {
        let server = app.ui.screens.server.clone();
        let device = app.config.device(&server, account);
        app.flow
            .log_in(&app.ui.text, &server, account, &password, args.register, device);
    }
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop: {e}"))?;
    app.failure.map_or(Ok(()), Err)
}

struct App {
    flow: Flow,
    state: WorldState,
    window: Option<Arc<Window>>,
    render: Option<Render>,
    items: Vec<ItemDesign>,
    /// What the player sees, for the shaded ground.
    fog: scene::Fog,
    /// Phase 10: lights of the frame (scene), reused.
    lights: Vec<client_render::PointLight>,
    started: Instant,
    /// When the last game frame began (ms since `started`), to log hitches.
    last_frame_ms: f64,
    /// Where the previous frame spent its time: logged with a slow frame,
    /// so a hitch says whether it came from the network messages or the
    /// drawing (alpha, seam crossing).
    last_cost: FrameCost,
    cursor: (f32, f32),
    /// Zoom as drawn; it eases toward `zoom_goal`, set by the wheel.
    zoom: f32,
    zoom_goal: f32,
    /// When the zoom last eased (ms since `started`).
    zoom_ms: f64,
    center: (f32, f32),
    destination: Option<TilePos>,
    failure: Option<String>,
    texts: text::Texts,
    /// NPC clicked from afar: `Talk` once the player is close enough.
    talk_to: Option<EntityId>,
    /// Phase 8: a node clicked from afar; gathered on arrival.
    gather_at: Option<EntityId>,
    /// Phase 11: a tile that switches (a brazier) clicked from
    /// afar; used on arrival.
    use_at: Option<TilePos>,
    /// Phase 7: an NPC right-clicked from afar; its shop opens on arrival.
    shop_at: Option<EntityId>,
    /// Monster or player selected with a click: techniques go to it.
    target: Option<EntityId>,
    /// Damage numbers rising over the beings (phase 5).
    floats: Vec<interface::Float>,
    ui: client_ui::Ui,
    assets: interface::Assets,
    /// Sprites, icons and floor looks (phase 6b).
    graphics: graphics::Graphics,
    /// What plays for each event, and the player's volumes.
    sound: sound::Sounds,
    /// `--screenshot-after`: when to take it (ms since `started`), once.
    shot_at_ms: Option<f64>,
    config: interface::LoginConfig,
    keys: client_ui::KeyMap,
    log: interface::Log,
    /// Input gathered between frames for the interface.
    input: client_ui::Input,
    /// Text from the IME (`Commit`) and from plain key presses this frame;
    /// with an IME both may carry the same characters, the IME wins.
    ime_text: String,
    key_text: String,
    modifiers: winit::keyboard::ModifiersState,
    /// Movement keys held: up, down, left, right.
    held: [bool; 4],
    last_step: Instant,
    /// The movement keys are what is walking now (released: `Stop`).
    key_walking: bool,
    /// A text field had the keyboard in the last frame.
    typing: bool,
    pending_click: bool,
    /// Left button held, and where the press began (drag and drop).
    mouse_down: bool,
    press_at: Option<(f32, f32)>,
    /// The player's hotbar layout (saved per character on this computer).
    bar: hotbar::Hotbar,
    /// Last bag, shop or recipe action still waiting for its answer, and
    /// when it was sent.
    pending: Option<(client_ui::Pending, f64)>,
}

impl App {
    fn new(
        flow: Flow,
        texts: text::Texts,
        ui: client_ui::Ui,
        assets: interface::Assets,
        config: interface::LoginConfig,
    ) -> App {
        App {
            flow,
            state: WorldState::new(),
            window: None,
            render: None,
            items: Vec::new(),
            fog: scene::Fog::default(),
            lights: Vec::new(),
            started: Instant::now(),
            last_frame_ms: 0.0,
            last_cost: FrameCost::default(),
            cursor: (0.0, 0.0),
            zoom: 1.0,
            zoom_goal: 1.0,
            zoom_ms: 0.0,
            center: (0.0, 0.0),
            destination: None,
            failure: None,
            texts,
            talk_to: None,
            gather_at: None,
            use_at: None,
            shop_at: None,
            floats: Vec::new(),
            target: None,
            ui,
            assets,
            graphics: graphics::Graphics::empty(),
            sound: sound::Sounds::silent(),
            shot_at_ms: None,
            config,
            keys: interface::key_map(),
            log: interface::Log::default(),
            input: client_ui::Input::default(),
            ime_text: String::new(),
            key_text: String::new(),
            modifiers: Default::default(),
            held: [false; 4],
            last_step: Instant::now(),
            key_walking: false,
            typing: false,
            pending_click: false,
            mouse_down: false,
            press_at: None,
            bar: hotbar::Hotbar::default(),
            pending: None,
        }
    }

    fn send(&self, msg: &protocol::ClientMsg) {
        if let Some(c) = &self.flow.conn {
            c.send(msg);
        }
    }

    /// The farthest zoom out whose screen corners stay on terrain
    /// the server sends (view radius plus `CHUNK_MARGIN`), so the unloaded
    /// black never shows. Wider screens get a closer limit.
    fn min_zoom(&self) -> f32 {
        let (size, scale) = self
            .window
            .as_ref()
            .map_or(((1, 1).into(), 1.0), |w| (w.inner_size(), w.scale_factor()));
        let (tw, th) = self
            .render
            .as_ref()
            .map_or((64.0, 32.0), |r| (r.config().tile_w, r.config().tile_h));
        let (w, h) = (
            size.width as f32 / scale as f32,
            size.height as f32 / scale as f32,
        );
        let radius = match self.state.view_radius() {
            0 => DEFAULT_VIEW_RADIUS,
            r => i32::from(r),
        };
        // Two tiles of slack: the player is anywhere in its tile, and
        // standing sprites reach above theirs.
        let reach = (radius + protocol::CHUNK_MARGIN - 2).max(1) as f32;
        // A screen corner is (w / tw + h / th) / 2 tiles away at zoom 1.
        ((w / tw + h / th) / (2.0 * reach)).clamp(MIN_ZOOM, MIN_ZOOM_CAP)
    }

    fn camera(&self) -> Camera {
        let (size, scale) = self
            .window
            .as_ref()
            .map_or(((1, 1).into(), 1.0), |w| (w.inner_size(), w.scale_factor()));
        Camera {
            center: self.center,
            // Same look on high-DPI screens: zoom 1 means 64 logical pixels
            // per tile. A window made larger may push the limit closer.
            zoom: self.zoom.max(self.min_zoom()) * scale as f32,
            // Never 0: a minimized window reports no size.
            screen: (size.width.max(1) as f32, size.height.max(1) as f32),
        }
    }

    /// Nothing to draw (minimized or suspended): keep the connection
    /// served so messages do not pile up.
    fn pump_idle(&mut self) {
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.flow.poll(&self.ui.text);
        self.receive(now_ms);
    }

    /// The window has pixels to draw on (not minimized).
    fn drawable(&self) -> bool {
        self.window
            .as_ref()
            .is_some_and(|w| w.inner_size().width > 0 && w.inner_size().height > 0)
    }

    /// Builds the renderer on the window, at start, after the device was
    /// lost or after a suspend. The GPU copies of the atlas and the sprite
    /// sheet start empty, so both are sent again.
    fn build_render(&mut self) -> Result<(), String> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        let size = window.inner_size();
        let render = pollster::block_on(Render::new(
            window,
            (size.width, size.height),
            RenderConfig::default(),
        ))
        .map_err(|e| e.to_string())?;
        self.render = Some(render);
        self.ui.fonts.invalidate_atlas();
        self.graphics.request_upload();
        Ok(())
    }

    /// Messages of the connection: the login answers go to `flow`, the
    /// rest to `WorldState`.
    /// Returns how many messages arrived.
    fn receive(&mut self, now_ms: f64) -> usize {
        let was_in_game = self.flow.in_game();
        let mut msgs = Vec::new();
        if let Some(c) = &self.flow.conn {
            while let Some(m) = c.receive() {
                msgs.push(m);
            }
        }
        let count = msgs.len();
        for m in msgs {
            let session = matches!(m, protocol::ServerMsg::Session { .. });
            if session {
                // A new session starts from a clean mirror (also after a reconnect).
                self.state = WorldState::new();
            }
            if let Some(m) = self.flow.on_msg(&self.ui.text, m) {
                self.state.apply(m, now_ms);
            }
            if session {
                self.on_session();
            }
        }
        if self.flow.in_game() && !was_in_game {
            self.destination = None;
            self.ui.clear_focus();
        }
        if self.flow.created {
            self.flow.created = false;
            self.ui.screens.selected = self.flow.characters.len().saturating_sub(1);
        }
        count
    }

    /// Logged in: keep the account name if asked to, never the password.
    fn on_session(&mut self) {
        if self.flow.account().is_none() {
            return; // --dev: nothing to keep.
        }
        let s = &self.ui.screens;
        let mut cfg = self.config.clone();
        cfg.account = s.remember.then(|| s.account.trim().to_owned());
        // The device token for this server and account.
        if std::mem::take(&mut self.flow.device_changed)
            && let (Some(device), Some(account)) = (&self.flow.device, self.flow.account())
        {
            cfg.set_device(&self.flow.server, account.as_str(), device);
        }
        cfg.locale = Some(self.flow.locale.code().to_owned());
        if !s.server.is_empty() && !cfg.servers.contains(&s.server) {
            cfg.servers.push(s.server.clone());
        }
        if cfg != self.config {
            cfg.save();
            self.config = cfg;
        }
    }

    /// The screen for the current stage (`None` in the game).
    fn screen(&mut self) -> Option<Screen> {
        let t = &self.ui.text;
        let error = self.flow.error.clone();
        Some(match &self.flow.stage {
            Stage::Game => return None,
            Stage::Login => Screen::Login(LoginModel {
                servers: self.config.servers.clone(),
                locale: self.flow.locale,
                error,
                busy: false,
            }),
            Stage::Connecting(_) | Stage::AwaitSession => Screen::Login(LoginModel {
                servers: self.config.servers.clone(),
                locale: self.flow.locale,
                error: None,
                busy: true,
            }),
            Stage::Reconnecting { .. } => Screen::Wait {
                title: t.t("ui.game.title"),
                // An announced restart waits longer and says so.
                text: t.t(if self.flow.waiting_restart() {
                    "ui.maintenance.waiting"
                } else {
                    "ui.login.reconnecting"
                }),
            },
            Stage::Select | Stage::Entering | Stage::Deleting => Screen::Select(SelectModel {
                characters: self
                    .flow
                    .characters
                    .iter()
                    .map(|c| CharacterCard {
                        name: c.name.to_string(),
                        level: c.level,
                        figure: interface::figure_of(&self.state, &self.graphics, c.look),
                    })
                    .collect(),
                max: flow::MAX_CHARACTERS,
                error,
                busy: matches!(self.flow.stage, Stage::Entering | Stage::Deleting),
            }),
            Stage::Create | Stage::Creating => Screen::Create(CreateModel {
                lists: interface::look_lists(&self.state, &self.texts),
                preview: {
                    let [body, skin, hair_style, hair_color] = self.ui.screens.look;
                    let look = base::Look {
                        body,
                        skin,
                        hair_style,
                        hair_color,
                    };
                    interface::figure_of(&self.state, &self.graphics, look).map(Box::new)
                },
                hidden: interface::hidden_lists(&self.state),
                error,
                busy: self.flow.stage == Stage::Creating,
            }),
            Stage::Queue {
                position,
                estimate_secs,
            } => Screen::Queue {
                position: *position,
                estimate_secs: *estimate_secs,
            },
            Stage::Password { required } | Stage::ChangingPassword { required } => {
                Screen::Password(PasswordModel {
                    required: *required,
                    error,
                    busy: matches!(self.flow.stage, Stage::ChangingPassword { .. }),
                })
            }
        })
    }

    fn screen_action(&mut self, a: UiAction) {
        let t = &self.ui.text;
        match a {
            UiAction::Login {
                server,
                account,
                password,
                ..
            } => {
                let device = self.config.device(&server, &account);
                self.flow.log_in(t, &server, &account, &password, false, device)
            }
            UiAction::Register {
                server,
                account,
                password,
                ..
            } => self.flow.log_in(t, &server, &account, &password, true, None),
            UiAction::SetLocale(locale) => {
                if locale == self.flow.locale {
                    return;
                }
                self.flow.locale = locale;
                self.assets.set_locale(locale, &mut self.ui);
                self.texts = text::Texts::new(locale);
                if let Some(w) = &self.window {
                    w.set_title(&self.ui.text.t("ui.game.title"));
                }
            }
            UiAction::Enter(i) => self.flow.enter(i),
            UiAction::DeleteCharacter { index, name } => {
                self.flow.delete(t, index, &name);
                self.ui.screens.selected = 0;
            }
            UiAction::OpenCreate => {
                self.ui.screens.look = [0; 4];
                self.flow.open(Stage::Create);
            }
            UiAction::Create { name, look } => {
                let [body, skin, hair_style, hair_color] = look;
                let look = Look {
                    body,
                    skin,
                    hair_style,
                    hair_color,
                };
                self.flow.create(t, &name, look);
            }
            UiAction::BackToSelect => self.flow.open(Stage::Select),
            UiAction::OpenChangePassword => self.flow.open(Stage::Password { required: false }),
            UiAction::ChangePassword { new } => self.flow.change_password(t, &new),
            UiAction::CancelQueue => self.flow.cancel_queue(),
            UiAction::Quit => {}
            _ => {}
        }
    }

    /// Input of this frame for the interface.
    fn take_input(&mut self, cam: &Camera, now_ms: f64) -> client_ui::Input {
        let mut input = std::mem::take(&mut self.input);
        input.screen = cam.screen;
        input.scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32);
        input.mouse = self.cursor;
        input.now_ms = now_ms as u64;
        input.typed = if self.ime_text.is_empty() {
            std::mem::take(&mut self.key_text)
        } else {
            self.key_text.clear();
            std::mem::take(&mut self.ime_text)
        };
        input.clicked = self.pending_click;
        input.mouse_down = self.mouse_down;
        input.press_at = self.press_at;
        input.shift = self.modifiers.shift_key();
        input
    }

    /// After the interface drew: the IME follows the focused field.
    fn after_ui(&mut self, input: &client_ui::Input, out: &client_ui::FrameOut) {
        self.input.preedit = input.preedit.clone();
        if let Some(w) = self.window.as_ref() {
            if self.typing != out.typing {
                w.set_ime_allowed(out.typing);
            }
            if let Some(r) = out.ime_area {
                w.set_ime_cursor_area(
                    PhysicalPosition::new(r.x as f64, r.y as f64),
                    PhysicalSize::new(r.w.max(1.0) as f64, r.h as f64),
                );
            }
        }
        self.typing = out.typing;
    }

    fn draw(&mut self, cam: &Camera, world: bool) {
        let ui: Vec<client_render::UiVertex> = self
            .ui
            .vertices()
            .iter()
            .map(|v| client_render::UiVertex {
                pos: v.pos,
                uv: v.uv,
                color: v.color,
            })
            .collect();
        let atlas = self.ui.take_atlas().map(|(size, pixels)| (size, pixels.to_vec()));
        if !world {
            self.items.clear();
        }
        if let Some(render) = self.render.as_mut() {
            render.draw(
                cam,
                &mut self.items,
                &[],
                &ui,
                atlas
                    .as_ref()
                    .map(|(size, pixels)| client_render::Atlas { size: *size, pixels }),
                self.graphics.take_upload(),
            );
            if let Some(shot) = render.take_screenshot() {
                match screenshot::save(&shot) {
                    Ok(path) => info!(path = %path.display(), "screenshot saved"),
                    Err(e) => tracing::warn!("screenshot not saved: {e}"),
                }
            }
        }
    }

    /// Asks the renderer for a copy of the next frame (F12).
    pub(crate) fn screenshot(&mut self) {
        if let Some(render) = self.render.as_mut()
            && !render.request_screenshot()
        {
            tracing::warn!("screenshot: this window's surface cannot be copied");
        }
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        if self.render.as_ref().is_some_and(Render::is_lost) {
            tracing::warn!("rebuilding the renderer after a device loss");
            if let Err(e) = self.build_render() {
                self.failure = Some(e);
                return event_loop.exit();
            }
        }
        if self.render.is_none() || !self.drawable() {
            return self.pump_idle();
        }
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        if self.shot_at_ms.is_some_and(|at| now_ms >= at) {
            self.shot_at_ms = None;
            self.screenshot();
        }
        self.flow.poll(&self.ui.text);
        let received = self.receive(now_ms);
        let receive_ms = self.started.elapsed().as_secs_f64() * 1000.0 - now_ms;
        self.assets.reload_if_changed(&mut self.ui);
        self.graphics.reload_if_changed();
        if self.flow.in_game() {
            // A frame far later than the last one is a hitch the player
            // felt: logged with what the client held and where the previous
            // frame and this one spent their time (alpha, seam crossing).
            let last = std::mem::replace(&mut self.last_frame_ms, now_ms);
            let gap = now_ms - last;
            if last > 0.0 && gap > SLOW_FRAME_MS && gap < 10_000.0 {
                let p = self.last_cost;
                tracing::warn!(
                    ms = gap as u64,
                    chunks = self.state.chunk_count(),
                    entities = self.state.entities().count(),
                    prev_msgs = p.msgs,
                    prev_receive_ms = p.receive_ms as u64,
                    prev_frame_ms = p.frame_ms as u64,
                    msgs = received,
                    receive_ms = receive_ms as u64,
                    "slow frame"
                );
            }
            let frame_start = self.started.elapsed().as_secs_f64() * 1000.0;
            self.game_frame(event_loop, now_ms);
            self.last_cost = FrameCost {
                msgs: received,
                receive_ms,
                frame_ms: self.started.elapsed().as_secs_f64() * 1000.0 - frame_start,
            };
            return;
        }
        // Screens before the game, with their music (P05). The text pack
        // names the look options.
        self.sound.poll();
        self.sound.screen_music("login");
        if matches!(self.flow.stage, Stage::Select | Stage::Create)
            && let text::Need::Request(locale) = self.texts.need_before_game()
        {
            self.send(&protocol::ClientMsg::RequestTextPack { locale });
        }
        if let Some(pack) = self.state.take_text_pack() {
            let hash = pack.content_hash;
            self.texts.received(pack, hash);
        }
        let Some(screen) = self.screen() else { return };
        let cam = self.camera();
        let input = self.take_input(&cam, now_ms);
        self.pending_click = false;
        self.ui.screens.backdrop = self.graphics.backdrop();
        let out = self.ui.screen(&input, &screen);
        self.after_ui(&input, &out);
        self.draw(&cam, false);
        for a in out.actions {
            self.screen_action(a);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            // Back from a suspend: only the surface needs to return.
            if self.render.is_none()
                && let Err(e) = self.build_render()
            {
                self.failure = Some(e);
                event_loop.exit();
            }
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.ui.text.t("ui.game.title"))
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.failure = Some(format!("cannot open the window: {e}"));
                return event_loop.exit();
            }
        };
        self.window = Some(window);
        if let Err(e) = self.build_render() {
            self.failure = Some(e);
            return event_loop.exit();
        }
        info!("window open");
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // The platform takes the surface away (mobile, some Wayland setups).
        self.render = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = self.render.as_mut() {
                    r.resize((size.width, size.height));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => match button {
                // Decided in the next frame: the interface may take it.
                MouseButton::Left => {
                    self.pending_click = true;
                    self.mouse_down = true;
                    self.press_at = Some(self.cursor);
                }
                MouseButton::Right => self.input.right_clicked = true,
                _ => {}
            },
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.mouse_down = false;
                self.input.released = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let up = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                // A touchpad gives pixels: about 40 to a line. The frame
                // decides: a window under the mouse scrolls, the world zooms.
                let lines = match delta {
                    MouseScrollDelta::LineDelta(..) => up,
                    MouseScrollDelta::PixelDelta(_) => up / 40.0,
                };
                if self.flow.in_game() {
                    self.input.wheel += lines;
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Ime(ime) => match ime {
                Ime::Commit(text) => {
                    self.ime_text.push_str(&text);
                    self.input.preedit.clear();
                }
                Ime::Preedit(text, _) => self.input.preedit = text,
                Ime::Disabled => self.input.preedit.clear(),
                Ime::Enabled => {}
            },
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                if self.typing {
                    if pressed {
                        match code {
                            KeyCode::Enter | KeyCode::NumpadEnter => self.input.enter = true,
                            KeyCode::Escape => {
                                self.input.escape = true;
                                self.ui.hud.close_top();
                                self.ui.clear_focus();
                            }
                            KeyCode::Backspace => self.input.backspace += 1,
                            KeyCode::Tab => self.input.tab = true,
                            _ => {
                                // While composing, keys belong to the IME.
                                if self.input.preedit.is_empty()
                                    && let Some(t) = &event.text
                                {
                                    self.key_text.push_str(t);
                                }
                            }
                        }
                    }
                    return;
                }
                if !self.flow.in_game() {
                    if pressed && matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) {
                        self.input.enter = true;
                    }
                    return;
                }
                if event.repeat {
                    return;
                }
                let chord = client_ui::Chord {
                    ctrl: self.modifiers.control_key(),
                    alt: self.modifiers.alt_key(),
                    shift: self.modifiers.shift_key(),
                    key: format!("{code:?}"),
                };
                let mut actions = self.keys.actions(&chord).to_vec();
                if !pressed {
                    // Releasing a movement key counts whatever the modifiers.
                    let plain = client_ui::Chord {
                        key: chord.key.clone(),
                        ..Default::default()
                    };
                    actions.extend_from_slice(self.keys.actions(&plain));
                }
                for a in actions {
                    self.action(event_loop, a, pressed);
                }
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Redraw continuously; presentation waits for vsync.
        if self.render.is_some() && self.drawable() {
            event_loop.set_control_flow(ControlFlow::Wait);
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        } else {
            // Minimized or suspended: no drawing, a slow tick serves the
            // connection; a resize or resume event wakes the loop at once.
            self.pump_idle();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(100),
            ));
        }
    }
}

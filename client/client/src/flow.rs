//! Before the game: connect, log in,
//! choose or create a character, wait in the queue, change a temporary
//! password, and reconnect with `Resume` after a drop. Nothing here blocks
//! the window: connecting runs in a thread and answers arrive as messages.

use base::{AccountName, Arg, CharacterId, CharacterName, Locale, Look};
use client_net::{Connection, ConnectionError, ConnectionState, TlsConfig};
use client_ui::UiText;
use protocol::{
    CharacterSummary, ClientMsg, DeviceToken, ErrorCode, PROTOCOL_VERSION, Password, ServerMsg, SessionToken,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Reconnection tries after the connection drops, one every `RETRY_EVERY`.
const RECONNECT_TRIES: u32 = 5;
const RETRY_EVERY: Duration = Duration::from_secs(2);
/// After an announced restart: one try every 10 s for 10 min.
const MAINTENANCE_TRIES: u32 = 60;
const MAINTENANCE_RETRY: Duration = Duration::from_secs(10);
/// Characters per account (the server's `CHARACTERS_PER_ACCOUNT`).
pub const MAX_CHARACTERS: usize = 5;

/// What to send once the connection is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    Dev(CharacterName),
    Login,
    Register,
    /// The saved token; the password if it expired.
    Resume,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Login,
    Connecting(Auth),
    AwaitSession,
    Select,
    Create,
    Creating,
    /// `DeleteCharacter` sent; the answer is the new list.
    Deleting,
    Password {
        required: bool,
    },
    ChangingPassword {
        required: bool,
    },
    Entering,
    Queue {
        position: u32,
        estimate_secs: u32,
    },
    Game,
    /// Dropped in the game: tries left and when the next one is due.
    Reconnecting {
        left: u32,
        due: Instant,
    },
}

pub struct Flow {
    pub stage: Stage,
    pub server: String,
    /// `None`: plain TCP (`--dev`).
    tls: Option<TlsConfig>,
    pub locale: Locale,
    account: Option<AccountName>,
    password: Option<Password>,
    token: Option<SessionToken>,
    /// This device's token for the account (kept in `login.ron`);
    /// `device_changed` asks the caller to save a new one.
    pub device: Option<DeviceToken>,
    pub device_changed: bool,
    /// The connection's last `Challenge` (seed, bits).
    challenge: Option<([u8; 16], u8)>,
    /// A password message waits for the challenge: `true` to register.
    want_password: Option<bool>,
    /// The proof of work being solved off the window thread.
    solving: Option<mpsc::Receiver<ClientMsg>>,
    /// `TooManyAttempts`: when the wait ends (the message counts down).
    wait_until: Option<Instant>,
    pub characters: Vec<CharacterSummary>,
    /// The character in play, entered again after a reconnect.
    character: Option<CharacterId>,
    /// `--character NAME`: entered (created if missing) without the screen.
    auto: Option<CharacterName>,
    /// `--change-password`: the screen opens after the first session.
    change_password: bool,
    /// `--dev`: reconnecting logs in by name again (no session token).
    dev_name: Option<CharacterName>,
    connecting: Option<mpsc::Receiver<Result<Connection, ConnectionError>>>,
    /// What to send when the connection being opened is ready.
    pending_auth: Option<Auth>,
    pub conn: Option<Connection>,
    /// Translated, shown on the current screen.
    pub error: Option<String>,
    /// The last `Characters` answer added one: select it.
    pub created: bool,
    /// An announced restart: when the server said it stops. A
    /// connection lost after it waits for the server to come back.
    maintenance: Option<Instant>,
    /// Lines for the game log (translated), taken by the game each frame.
    notices: Vec<String>,
}

impl Flow {
    pub fn new(server: String, tls: Option<TlsConfig>, locale: Locale) -> Flow {
        Flow {
            stage: Stage::Login,
            server,
            tls,
            locale,
            account: None,
            password: None,
            token: None,
            device: None,
            device_changed: false,
            challenge: None,
            want_password: None,
            solving: None,
            wait_until: None,
            characters: Vec::new(),
            character: None,
            auto: None,
            change_password: false,
            dev_name: None,
            connecting: None,
            pending_auth: None,
            conn: None,
            error: None,
            created: false,
            maintenance: None,
            notices: Vec::new(),
        }
    }

    /// Lines about the announced restart for the game log.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    /// Waiting for the server after an announced restart.
    pub fn waiting_restart(&self) -> bool {
        self.maintenance.is_some()
    }

    /// `--character` and `--change-password` from the command line.
    pub fn with_options(mut self, auto: Option<CharacterName>, change_password: bool) -> Flow {
        self.auto = auto;
        self.change_password = change_password;
        self
    }

    pub fn in_game(&self) -> bool {
        matches!(self.stage, Stage::Game)
    }

    pub fn account(&self) -> Option<&AccountName> {
        self.account.as_ref()
    }

    /// Login or registration from the screen (or the command line);
    /// `device` is the token this device keeps for that server and account.
    pub fn log_in(
        &mut self,
        t: &UiText,
        server: &str,
        account: &str,
        password: &str,
        register: bool,
        device: Option<DeviceToken>,
    ) {
        if self.waiting() {
            return;
        }
        self.error = None;
        let Ok(account) = AccountName::try_from(account) else {
            self.error = Some(t.t("ui.login.invalid_account"));
            return;
        };
        let Ok(password) = Password::try_from(password.to_owned()) else {
            self.error = Some(t.t("ui.login.invalid_password"));
            return;
        };
        if !server.is_empty() {
            self.server = server.to_owned();
        }
        self.account = Some(account);
        self.password = Some(password);
        self.device = device;
        self.token = None;
        self.character = None;
        self.connect(if register { Auth::Register } else { Auth::Login });
    }

    pub fn dev(&mut self, name: CharacterName) {
        self.dev_name = Some(name.clone());
        self.connect(Auth::Dev(name));
    }

    /// Opens a new connection in a thread; `auth` goes once it is open.
    fn connect(&mut self, auth: Auth) {
        if let Some(old) = self.conn.take() {
            old.close();
        }
        self.challenge = None;
        self.want_password = None;
        self.solving = None;
        let (tx, rx) = mpsc::channel();
        let (server, tls) = (self.server.clone(), self.tls.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Connection::connect(&server, tls.as_ref()));
        });
        self.connecting = Some(rx);
        if !matches!(self.stage, Stage::Reconnecting { .. }) {
            self.stage = Stage::Connecting(auth.clone());
        }
        self.pending_auth = Some(auth);
    }

    fn send(&self, msg: &ClientMsg) {
        if let Some(c) = &self.conn {
            c.send(msg);
        }
    }

    /// `TooManyAttempts` still counting down: the login button waits.
    pub fn waiting(&self) -> bool {
        self.wait_until.is_some_and(|u| u > Instant::now())
    }

    /// Once per frame: a finished connection attempt, a due reconnection,
    /// or a connection that dropped.
    pub fn poll(&mut self, t: &UiText) {
        self.count_down(t);
        if let Some(rx) = &self.solving {
            match rx.try_recv() {
                Ok(msg) => {
                    self.solving = None;
                    self.send(&msg);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.solving = None,
            }
        }
        if let Some(rx) = &self.connecting {
            match rx.try_recv() {
                Ok(Ok(conn)) => {
                    self.connecting = None;
                    self.opened(conn);
                }
                Ok(Err(e)) => {
                    self.connecting = None;
                    warn!(server = %self.server, "cannot connect: {e}");
                    let key = match e {
                        ConnectionError::UntrustedCertificate(_) => "ui.login.untrusted",
                        _ => "ui.login.no_connection",
                    };
                    self.failed(t.t(key));
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.connecting = None;
                    self.failed(t.t("ui.login.no_connection"));
                }
            }
            return;
        }
        if let Stage::Reconnecting { due, .. } = self.stage {
            if Instant::now() >= due {
                let auth = self.dev_name.clone().map_or(Auth::Resume, Auth::Dev);
                self.connect(auth);
            }
            return;
        }
        let open = self
            .conn
            .as_ref()
            .is_some_and(|c| c.state() == ConnectionState::Open);
        if open {
            return;
        }
        match self.stage {
            Stage::Game | Stage::Entering | Stage::Queue { .. } if self.character.is_some() => {
                info!(
                    restart = self.maintenance.is_some(),
                    "connection lost: reconnecting"
                );
                let left = if self.maintenance.is_some() {
                    MAINTENANCE_TRIES
                } else {
                    RECONNECT_TRIES
                };
                self.stage = Stage::Reconnecting {
                    left,
                    // After an announced restart the server needs a moment.
                    due: Instant::now() + self.maintenance.map_or(Duration::ZERO, |_| MAINTENANCE_RETRY),
                };
            }
            Stage::Login => {}
            _ if self.conn.is_some() => {
                self.conn = None;
                if self.error.is_none() {
                    self.error = Some(t.t("ui.login.no_connection"));
                }
                self.stage = Stage::Login;
            }
            _ => {}
        }
    }

    /// A connection attempt failed: try again later while reconnecting,
    /// else back to the login screen.
    fn failed(&mut self, error: String) {
        match self.stage {
            Stage::Reconnecting { left, .. } if left > 1 => {
                self.stage = Stage::Reconnecting {
                    left: left - 1,
                    due: Instant::now() + self.retry_every(),
                };
            }
            _ => {
                self.maintenance = None;
                self.error = Some(error);
                self.stage = Stage::Login;
            }
        }
    }

    fn retry_every(&self) -> Duration {
        if self.maintenance.is_some() {
            MAINTENANCE_RETRY
        } else {
            RETRY_EVERY
        }
    }

    fn opened(&mut self, conn: Connection) {
        conn.send(&ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            locale: self.locale,
        });
        self.conn = Some(conn);
        match self.pending_auth.take().unwrap_or(Auth::Login) {
            Auth::Dev(name) => {
                self.send(&ClientMsg::DevLogin { name });
                self.character = Some(CharacterId::new(0));
                self.stage = Stage::Entering;
                return;
            }
            Auth::Resume if self.token.is_some() => {
                if let Some(token) = self.token.take() {
                    self.send(&ClientMsg::Resume { token });
                }
            }
            Auth::Register => self.send_password(true),
            Auth::Login | Auth::Resume => self.send_password(false),
        }
        if !matches!(self.stage, Stage::Reconnecting { .. }) {
            self.stage = Stage::AwaitSession;
        }
    }

    /// Sends `Login` or `Register` once the connection's `Challenge` is
    /// here, with its proof of work solved in a thread.
    fn send_password(&mut self, register: bool) {
        let Some((seed, bits)) = self.challenge else {
            self.want_password = Some(register);
            return;
        };
        self.want_password = None;
        let (Some(account), Some(password)) = (self.account.clone(), self.password.clone()) else {
            return;
        };
        let device = self.device.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let proof = protocol::solve_proof(&seed, account.as_str(), bits);
            let _ = tx.send(if register {
                ClientMsg::Register {
                    account,
                    password,
                    proof,
                }
            } else {
                ClientMsg::Login {
                    account,
                    password,
                    device,
                    proof,
                }
            });
        });
        self.solving = Some(rx);
    }

    /// The `TooManyAttempts` message with the seconds still left.
    fn count_down(&mut self, t: &UiText) {
        let Some(until) = self.wait_until else { return };
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            self.wait_until = None;
            if self.stage == Stage::Login {
                self.error = None;
            }
        } else if self.stage == Stage::Login {
            let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
            self.error = Some(error_text(
                t,
                ErrorCode::TooManyAttempts,
                &[Arg::Int(secs as i64)],
            ));
        }
    }

    pub fn enter(&mut self, index: usize) {
        let Some(c) = self.characters.get(index) else {
            return;
        };
        self.character = Some(c.id);
        self.error = None;
        self.send(&ClientMsg::Enter { character: c.id });
        self.stage = Stage::Entering;
    }

    pub fn create(&mut self, t: &UiText, name: &str, look: Look) {
        let Ok(name) = CharacterName::try_from(name) else {
            self.error = Some(t.t("ui.create.invalid_name"));
            return;
        };
        self.error = None;
        self.send(&ClientMsg::CreateCharacter { name, look });
        self.stage = Stage::Creating;
    }

    /// Deletes the character at `index` once its name was typed again.
    pub fn delete(&mut self, t: &UiText, index: usize, typed: &str) {
        let Some(c) = self.characters.get(index) else {
            return;
        };
        let Ok(name) = CharacterName::try_from(typed.trim()) else {
            self.error = Some(t.t("ui.select.delete_mismatch"));
            return;
        };
        if !name.eq_ignore_ascii_case(&c.name) {
            self.error = Some(t.t("ui.select.delete_mismatch"));
            return;
        }
        self.error = None;
        self.send(&ClientMsg::DeleteCharacter {
            character: c.id,
            name,
        });
        self.stage = Stage::Deleting;
    }

    pub fn change_password(&mut self, t: &UiText, new: &str) {
        let (Some(old), Ok(new)) = (self.password.clone(), Password::try_from(new.to_owned())) else {
            self.error = Some(t.t("ui.password.too_short"));
            return;
        };
        self.error = None;
        let required = matches!(self.stage, Stage::Password { required: true });
        self.send(&ClientMsg::ChangePassword {
            old,
            new: new.clone(),
        });
        self.password = Some(new);
        self.stage = Stage::ChangingPassword { required };
    }

    pub fn open(&mut self, stage: Stage) {
        self.error = None;
        self.stage = stage;
    }

    /// Leaves the queue: a new connection resumes the session and shows
    /// the selection again.
    pub fn cancel_queue(&mut self) {
        self.character = None;
        self.auto = None;
        self.connect(Auth::Resume);
        self.stage = Stage::Connecting(Auth::Resume);
    }

    /// Takes the answers about the login; the rest is returned for
    /// `WorldState`.
    pub fn on_msg(&mut self, t: &UiText, msg: ServerMsg) -> Option<ServerMsg> {
        match msg {
            ServerMsg::Session {
                token,
                characters,
                device,
                failed_logins,
            } => {
                self.token = Some(token);
                self.characters = characters;
                self.error = None;
                self.wait_until = None;
                if device.is_some() {
                    self.device = device;
                    self.device_changed = true;
                }
                self.after_session();
                // Wrong passwords typed for this account since its
                // last login, on the next screen and in the game log.
                if failed_logins > 0 {
                    let text = t.t_args("ui.login.failed_logins", &[Arg::Int(i64::from(failed_logins))]);
                    self.notices.push(text.clone());
                    self.error = Some(text);
                }
                None
            }
            ServerMsg::Challenge { seed, bits } => {
                self.challenge = Some((seed, bits));
                if let Some(register) = self.want_password {
                    self.send_password(register);
                }
                None
            }
            ServerMsg::Characters { characters } => {
                self.created = characters.len() > self.characters.len();
                self.characters = characters;
                if let Some(id) = self.auto_character() {
                    self.send(&ClientMsg::Enter { character: id });
                    self.character = Some(id);
                    self.stage = Stage::Entering;
                } else {
                    self.stage = Stage::Select;
                }
                None
            }
            ServerMsg::Welcome { .. } => {
                self.stage = Stage::Game;
                self.error = None;
                self.maintenance = None;
                Some(msg)
            }
            ServerMsg::Maintenance { in_secs } => {
                self.maintenance = Some(Instant::now() + Duration::from_secs(u64::from(in_secs)));
                let text = maintenance_text(t, in_secs);
                if self.in_game() {
                    self.notices.push(text);
                } else {
                    self.error = Some(text);
                }
                None
            }
            ServerMsg::MaintenanceCancelled => {
                self.maintenance = None;
                let text = t.t("ui.maintenance.cancelled");
                if self.in_game() {
                    self.notices.push(text);
                } else {
                    self.error = Some(text);
                }
                None
            }
            ServerMsg::QueuePosition {
                position,
                estimate_secs,
            } if !self.in_game() => {
                self.stage = Stage::Queue {
                    position,
                    estimate_secs,
                };
                Some(msg)
            }
            ServerMsg::Error { code, args } if !self.in_game() => {
                self.refused(t, code, &args);
                None
            }
            other => Some(other),
        }
    }

    fn auto_character(&self) -> Option<CharacterId> {
        let name = self.auto.as_ref()?;
        self.characters
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| c.id)
    }

    fn after_session(&mut self) {
        if self.change_password {
            self.change_password = false;
            self.stage = Stage::Password { required: false };
            return;
        }
        // Reconnecting in the game, or the queue: back to the same character.
        if let Some(id) = self
            .character
            .filter(|_| matches!(self.stage, Stage::Reconnecting { .. }))
        {
            self.send(&ClientMsg::Enter { character: id });
            self.stage = Stage::Entering;
            return;
        }
        if let Some(id) = self.auto_character() {
            self.character = Some(id);
            self.send(&ClientMsg::Enter { character: id });
            self.stage = Stage::Entering;
            return;
        }
        if let Some(name) = self.auto.clone() {
            self.send(&ClientMsg::CreateCharacter {
                name,
                look: Look::default(),
            });
            self.stage = Stage::Creating;
            return;
        }
        self.stage = Stage::Select;
    }

    /// An error before the game: shown on the screen it belongs to.
    fn refused(&mut self, t: &UiText, code: ErrorCode, args: &[Arg]) {
        self.error = Some(error_text(t, code, args));
        if code == ErrorCode::TooManyAttempts
            && let Some(Arg::Int(secs)) = args.first()
        {
            self.wait_until = Some(Instant::now() + Duration::from_secs((*secs).max(0) as u64));
        }
        if code == ErrorCode::PasswordChangeRequired {
            self.auto = None;
            self.stage = Stage::Password { required: true };
            return;
        }
        self.stage = match &self.stage {
            Stage::AwaitSession | Stage::Connecting(_) | Stage::Reconnecting { .. } => {
                if code == ErrorCode::SessionExpired && self.password.is_some() {
                    self.error = None;
                    self.send_password(false);
                    return;
                }
                self.character = None;
                Stage::Login
            }
            Stage::Creating => {
                self.auto = None;
                Stage::Create
            }
            Stage::Deleting => Stage::Select,
            Stage::ChangingPassword { required } => Stage::Password { required: *required },
            // A dev login is refused by closing; the rest stays logged in.
            Stage::Entering | Stage::Queue { .. } if self.token.is_none() => Stage::Login,
            Stage::Entering | Stage::Queue { .. } => {
                self.auto = None;
                self.character = None;
                Stage::Select
            }
            other => other.clone(),
        };
    }
}

/// "The server restarts in 5 min" (whole minutes from one minute up).
pub fn maintenance_text(t: &UiText, in_secs: u32) -> String {
    if in_secs >= 60 {
        t.t_args(
            "ui.maintenance.in_minutes",
            &[Arg::Int(i64::from(in_secs.div_ceil(60)))],
        )
    } else {
        t.t_args("ui.maintenance.in_seconds", &[Arg::Int(i64::from(in_secs))])
    }
}

/// `error.<code>` in the interface texts. A reason (`Arg::Ref`) points into
/// the content texts, which come only after the login: left out here.
pub fn error_text(t: &UiText, code: ErrorCode, args: &[Arg]) -> String {
    let args: Vec<Arg> = args
        .iter()
        .map(|a| match a {
            Arg::Ref(_) => Arg::Str(String::new()),
            other => other.clone(),
        })
        .collect();
    t.t_args(code.text_key(), &args)
}

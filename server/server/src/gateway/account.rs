//! Handshake, accounts and sessions: every message a connection may send
//! before it enters the zone, and the tasks that answer them.
//!
//! Nothing here touches the zone. Each slow step (database, argon2) runs in
//! a task and comes back as a [`Done`], so the connection waits in
//! `Phase::Busy` instead of blocking the gateway task.

use super::*;
use protocol::DeviceToken;
use zeroize::Zeroizing;

impl Gateway {
    pub(super) fn spawn_dev_login(&self, c: ConnectionId, name: CharacterName) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        let sheet = new_sheet(dev_look(&self.cfg.content.appearance, &name));
        tokio::spawn(async move {
            let result = match repo.dev_character(&name, &sheet, login::now_unix()).await {
                Ok(character) => Done::Loaded {
                    c,
                    character: Box::new(character),
                    dev: true,
                    muted_until: None,
                },
                Err(e) => failed(c, e, Back::Close),
            };
            let _ = done.send(result);
        });
    }

    pub(super) fn spawn_register(
        &self,
        c: ConnectionId,
        ip: IpAddr,
        account: AccountName,
        password: Password,
    ) {
        let (repo, done, passwords, keys) = (
            self.repo.clone(),
            self.done_tx.clone(),
            self.passwords.clone(),
            self.keys.clone(),
        );
        let ip_key = ip_key(ip);
        tokio::spawn(async move {
            let result = async {
                let now = login::now_unix();
                if let Some(s) = repo.ip_sanction(&ip_key, now).await? {
                    return Ok(refused(c, &s, now, Back::Login, &keys));
                }
                let hash = passwords
                    .hash(Zeroizing::new(password.expose().to_owned()))
                    .await
                    .ok_or(RepoError::Password("hash failed".into()))?;
                let id = repo.create_account(&account, Some(hash), now).await?;
                repo.record_login(id, &ip_key, now).await?;
                info!(%account, %ip, "account created");
                let after = AfterLogin {
                    device: Some(issue_device(&repo, id, None, now).await?),
                    ..AfterLogin::default()
                };
                open_session(&repo, c, id, after, true, false).await
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| failed(c, e, Back::Login)));
        });
    }

    /// The tries were counted before this runs. `account_wait`:
    /// the account itself is waiting, which only a known device skips.
    pub(super) fn spawn_login(&self, c: ConnectionId, req: LoginRequest) {
        let (repo, done, passwords, keys) = (
            self.repo.clone(),
            self.done_tx.clone(),
            self.passwords.clone(),
            self.keys.clone(),
        );
        tokio::spawn(async move {
            let LoginRequest {
                account,
                password,
                device,
                account_wait,
                ip,
            } = req;
            let ip_key = ip_key(ip);
            let result = async {
                let now = login::now_unix();
                if let Some(s) = repo.ip_sanction(&ip_key, now).await? {
                    return Ok(refused(c, &s, now, Back::Login, &keys));
                }
                let row = repo.account(&account).await?;
                let known = match (&device, &row) {
                    (Some(d), Some(r)) => repo.device_ok(r.id, login::device_hash(d), now).await?,
                    _ => false,
                };
                if let Some(secs) = account_wait
                    && !known
                {
                    warn!(%ip, %account, "login refused: account waiting");
                    return Ok(Done::Failed {
                        c,
                        code: ErrorCode::TooManyAttempts,
                        args: vec![Arg::Int(secs as i64)],
                        back: Back::Login,
                    });
                }
                let hash = row.as_ref().and_then(|r| r.password_hash.clone());
                let right = passwords
                    .verify(Zeroizing::new(password.expose().to_owned()), hash)
                    .await;
                let Some(row) = row.filter(|_| right) else {
                    if let Some(r) = repo.account(&account).await? {
                        repo.add_failed_login(r.id).await?;
                    }
                    return Ok(Done::LoginFailed {
                        c,
                        account: account.to_string(),
                        ip: ip_key.clone(),
                    });
                };
                if let Some(s) = repo.account_sanction(row.id, now).await?
                    && s.kind.blocks_login()
                {
                    return Ok(refused(c, &s, now, Back::Login, &keys));
                }
                repo.record_login(row.id, &ip_key, now).await?;
                let old = device.as_ref().filter(|_| known);
                let after = AfterLogin {
                    clear: vec![login::pair_attempt_key(account.as_str(), &ip_key)],
                    refund: vec![
                        login::ip_attempt_key(&ip_key),
                        login::account_attempt_key(account.as_str()),
                    ],
                    device: Some(issue_device(&repo, row.id, old, now).await?),
                    failed_logins: repo.take_failed_logins(row.id).await?,
                };
                open_session(&repo, c, row.id, after, false, row.must_change_password).await
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| failed(c, e, Back::Login)));
        });
    }

    pub(super) fn spawn_resume(&self, c: ConnectionId, token: SessionToken, ip: IpAddr) {
        let (repo, done, keys) = (self.repo.clone(), self.done_tx.clone(), self.keys.clone());
        let ip_key = ip_key(ip);
        tokio::spawn(async move {
            let result = async {
                let now = login::now_unix();
                if let Some(s) = repo.ip_sanction(&ip_key, now).await? {
                    return Ok(refused(c, &s, now, Back::Login, &keys));
                }
                match repo.take_session(login::token_hash(&token), now).await? {
                    Some(account) => {
                        if let Some(s) = repo.account_sanction(account, now).await?
                            && s.kind.blocks_login()
                        {
                            return Ok(refused(c, &s, now, Back::Login, &keys));
                        }
                        repo.record_login(account, &ip_key, now).await?;
                        let must_change = repo
                            .account_by_id(account)
                            .await?
                            .is_some_and(|r| r.must_change_password);
                        open_session(&repo, c, account, AfterLogin::default(), false, must_change).await
                    }
                    None => Ok(Done::Failed {
                        c,
                        code: ErrorCode::SessionExpired,
                        args: Vec::new(),
                        back: Back::Login,
                    }),
                }
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| failed(c, e, Back::Login)));
        });
    }

    /// `old` is checked before the new hash is written; every other session
    /// and device of the account is dropped and a new one is opened.
    pub(super) fn spawn_change_password(
        &self,
        c: ConnectionId,
        account: AccountId,
        old: Password,
        new: Password,
        ip: IpAddr,
    ) {
        let (repo, done, passwords) = (self.repo.clone(), self.done_tx.clone(), self.passwords.clone());
        tokio::spawn(async move {
            let ip_key = ip_key(ip);
            let result = async {
                let Some(row) = repo.account_by_id(account).await? else {
                    return Ok(failed(c, RepoError::NotFound, Back::Authenticated));
                };
                if !passwords
                    .verify(Zeroizing::new(old.expose().to_owned()), row.password_hash)
                    .await
                {
                    return Ok(Done::ChangeFailed {
                        c,
                        account: row.name,
                        ip: ip_key.clone(),
                    });
                }
                let hash = passwords
                    .hash(Zeroizing::new(new.expose().to_owned()))
                    .await
                    .ok_or(RepoError::Password("hash failed".into()))?;
                repo.set_password(account, Some(hash), false).await?;
                repo.delete_sessions(account).await?;
                repo.delete_devices(account).await?;
                let now = login::now_unix();
                let after = AfterLogin {
                    clear: vec![login::change_attempt_key(account)],
                    refund: vec![login::ip_attempt_key(&ip_key)],
                    device: Some(issue_device(&repo, account, None, now).await?),
                    failed_logins: 0,
                };
                open_session(&repo, c, account, after, false, false).await
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| failed(c, e, Back::Authenticated)));
        });
    }

    pub(super) fn spawn_create_character(
        &self,
        c: ConnectionId,
        account: AccountId,
        name: CharacterName,
        look: [String; 4],
    ) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let result = async {
                let sheet = new_sheet(look);
                repo.create_character(account, &name, CHARACTERS_PER_ACCOUNT, &sheet, login::now_unix())
                    .await?;
                Ok(Done::Characters {
                    c,
                    characters: repo.characters(account).await?,
                })
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| failed(c, e, Back::Authenticated)));
        });
    }

    /// Checks the typed name against the account's own list, then
    /// deletes; answers with the new list.
    pub(super) fn spawn_delete_character(
        &self,
        c: ConnectionId,
        account: AccountId,
        character: CharacterId,
        name: CharacterName,
        actor: String,
    ) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let back = Back::Authenticated;
            let result = async {
                let rows = repo.characters(account).await?;
                let Some(row) = rows.iter().find(|r| r.id == character) else {
                    return Ok(Done::Failed {
                        c,
                        code: ErrorCode::NoSuchCharacter,
                        args: Vec::new(),
                        back,
                    });
                };
                if !row.name.as_str().eq_ignore_ascii_case(name.as_str()) {
                    return Ok(Done::Failed {
                        c,
                        code: ErrorCode::InvalidName,
                        args: Vec::new(),
                        back,
                    });
                }
                let deleted = repo
                    .delete_character(account, character, &actor, login::now_unix())
                    .await?;
                info!(%actor, character = %deleted, "character deleted");
                Ok::<_, RepoError>(Done::Characters {
                    c,
                    characters: repo.characters(account).await?,
                })
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| match e {
                RepoError::NotFound => Done::Failed {
                    c,
                    code: ErrorCode::NoSuchCharacter,
                    args: Vec::new(),
                    back,
                },
                e => failed(c, e, back),
            }));
        });
    }

    pub(super) fn spawn_load(&self, c: ConnectionId, account: AccountId, character: CharacterId) {
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let back = Back::AuthenticatedRelease(account);
            let muted_until = match repo.mute_until(account, login::now_unix()).await {
                Ok(m) => m,
                Err(e) => return drop(done.send(failed(c, e, back))),
            };
            let result = match repo.load_character(character).await {
                Ok(Some(ch)) if ch.account == account => Done::Loaded {
                    c,
                    character: Box::new(ch),
                    dev: false,
                    muted_until,
                },
                Ok(_) => Done::Failed {
                    c,
                    code: ErrorCode::NoSuchCharacter,
                    args: Vec::new(),
                    back,
                },
                Err(e) => failed(c, e, back),
            };
            let _ = done.send(result);
        });
    }

    /// Rows of the account as the selection screen shows them.
    pub(super) fn summaries(&self, rows: Vec<CharacterRow>) -> Vec<CharacterSummary> {
        let a = &self.cfg.content.appearance;
        rows.into_iter()
            .map(|r| {
                let [b, s, h, k] = &r.look;
                CharacterSummary {
                    id: r.id,
                    name: r.name,
                    level: r.level,
                    look: a.look_of([b, s, h, k]),
                }
            })
            .collect()
    }

    /// Handshake and login: what a connection may send before it has an
    /// account open.
    pub(super) fn on_login_msg(&mut self, c: ConnectionId, msg: ClientMsg) {
        let Some(conn) = self.conns.get_mut(&c) else {
            return;
        };
        let peer = conn.peer;
        match (&conn.phase, msg) {
            (Phase::AwaitHello, ClientMsg::Hello { version, locale }) => {
                if version != PROTOCOL_VERSION {
                    return self.fail(c, ErrorCode::IncompatibleVersion);
                }
                conn.locale = locale;
                conn.phase = Phase::AwaitLogin;
                self.send_challenge(c);
            }
            (Phase::AwaitLogin, ClientMsg::DevLogin { name }) => {
                if !self.cfg.dev || !peer.ip().is_loopback() {
                    return self.fail(c, ErrorCode::NotAllowed);
                }
                conn.phase = Phase::Busy;
                self.spawn_dev_login(c, name);
            }
            (
                Phase::AwaitLogin,
                ClientMsg::Register {
                    account,
                    password,
                    proof,
                },
            ) => {
                let (seed, bits) = conn.challenge;
                if !protocol::proof_ok(&seed, account.as_str(), proof, bits) {
                    return self.fail(c, ErrorCode::InvalidMessage);
                }
                self.send_challenge(c);
                if password.contains_name(account.as_str()) {
                    return self.reply_error(c, ErrorCode::PasswordHasName, Vec::new());
                }
                if let Some(secs) = self.registrations.blocked(peer.ip(), Instant::now()) {
                    return self.reply_error(c, ErrorCode::TooManyAttempts, vec![Arg::Int(secs as i64)]);
                }
                if let Some(conn) = self.conns.get_mut(&c) {
                    conn.phase = Phase::Busy;
                }
                self.spawn_register(c, peer.ip(), account, password);
            }
            (
                Phase::AwaitLogin,
                ClientMsg::Login {
                    account,
                    password,
                    device,
                    proof,
                },
            ) => {
                let (seed, bits) = conn.challenge;
                if !protocol::proof_ok(&seed, account.as_str(), proof, bits) {
                    return self.fail(c, ErrorCode::InvalidMessage);
                }
                self.send_challenge(c);
                let now = login::now_unix();
                let ip = ip_key(peer.ip());
                let keys = [
                    login::ip_attempt_key(&ip),
                    login::pair_attempt_key(account.as_str(), &ip),
                    login::account_attempt_key(account.as_str()),
                ];
                let [ip_wait, pair_wait, account_wait] =
                    keys.each_ref().map(|k| self.attempts.blocked(k, now));
                // The account's own wait is skipped by a known device, which
                // the task checks; IP and account-from-this-IP never are.
                let wait = ip_wait
                    .max(pair_wait)
                    .or(account_wait.filter(|_| device.is_none()));
                if let Some(secs) = wait {
                    warn!(%ip, %account, "login refused: waiting");
                    return self.reply_error(c, ErrorCode::TooManyAttempts, vec![Arg::Int(secs as i64)]);
                }
                for k in &keys {
                    self.attempts.failed(k, now);
                }
                if let Some(conn) = self.conns.get_mut(&c) {
                    conn.phase = Phase::Busy;
                }
                self.spawn_login(
                    c,
                    LoginRequest {
                        account,
                        password,
                        device,
                        account_wait,
                        ip: peer.ip(),
                    },
                );
            }
            (Phase::AwaitLogin, ClientMsg::Resume { token }) => {
                conn.phase = Phase::Busy;
                self.spawn_resume(c, token, peer.ip());
            }
            _ => self.fail(c, ErrorCode::InvalidMessage),
        }
    }

    /// An open account that is not in the zone yet: characters,
    /// preferences and the password.
    pub(super) fn on_account_msg(&mut self, c: ConnectionId, msg: ClientMsg) {
        let Some(conn) = self.conns.get_mut(&c) else {
            return;
        };
        let peer = conn.peer;
        match (&conn.phase, msg) {
            (Phase::Authenticated, ClientMsg::CreateCharacter { name, look }) => {
                let Some(account) = conn.account else { return };
                if conn.must_change {
                    return self.reply_error(c, ErrorCode::PasswordChangeRequired, Vec::new());
                }
                // A look outside the lists is a broken client, not a choice.
                let Some(ids) = self.cfg.content.appearance.ids_of(look) else {
                    return self.reply_error(c, ErrorCode::InvalidMessage, Vec::new());
                };
                conn.phase = Phase::Busy;
                self.spawn_create_character(c, account, name, ids);
            }
            // Only from the selection screen, never a character in
            // the game or still being saved, and with its name typed again.
            (Phase::Authenticated, ClientMsg::DeleteCharacter { character, name }) => {
                let Some(account) = conn.account else { return };
                if conn.must_change {
                    return self.reply_error(c, ErrorCode::PasswordChangeRequired, Vec::new());
                }
                if self.online_characters.contains(&character) {
                    return self.reply_error(c, ErrorCode::AlreadyOnline, Vec::new());
                }
                let actor = conn.account_name.clone();
                conn.phase = Phase::Busy;
                self.spawn_delete_character(c, account, character, name, actor);
            }
            (Phase::Authenticated, ClientMsg::Enter { character }) => {
                let Some(account) = conn.account else { return };
                if conn.must_change {
                    return self.reply_error(c, ErrorCode::PasswordChangeRequired, Vec::new());
                }
                if self.online_accounts.contains(&account) {
                    return self.reply_error(c, ErrorCode::AlreadyOnline, Vec::new());
                }
                // Reserved now, so a second `Enter` cannot race this one.
                self.online_accounts.insert(account);
                conn.phase = Phase::Busy;
                self.spawn_load(c, account, character);
            }
            (Phase::Authenticated, ClientMsg::SetPreferences(Preference::Locale(locale))) => {
                conn.locale = locale;
                if let Some(account) = conn.account {
                    let repo = self.repo.clone();
                    tokio::spawn(async move {
                        if let Err(e) = repo.set_locale(account, locale).await {
                            warn!(error = %e, "cannot save the locale");
                        }
                    });
                }
            }
            (Phase::Authenticated, ClientMsg::ChangePassword { old, new }) => {
                let Some(account) = conn.account else { return };
                if new.contains_name(&conn.account_name) {
                    return self.reply_error(c, ErrorCode::PasswordHasName, Vec::new());
                }
                // Same wait as `Login`: a stolen token must not become a way
                // to guess the password. Counted before the check.
                let now = login::now_unix();
                let keys = [
                    login::change_attempt_key(account),
                    login::ip_attempt_key(&ip_key(peer.ip())),
                ];
                let wait = keys.iter().filter_map(|k| self.attempts.blocked(k, now)).max();
                if let Some(secs) = wait {
                    return self.reply_error(c, ErrorCode::TooManyAttempts, vec![Arg::Int(secs as i64)]);
                }
                for k in &keys {
                    self.attempts.failed(k, now);
                }
                if let Some(conn) = self.conns.get_mut(&c) {
                    conn.phase = Phase::Busy;
                }
                self.spawn_change_password(c, account, old, new, peer.ip());
            }
            _ => self.fail(c, ErrorCode::InvalidMessage),
        }
    }
}

/// Sheet of a new character with the look chosen at creation.
pub(super) fn new_sheet(look: [String; 4]) -> SavedSheet {
    SavedSheet {
        look,
        ..SavedSheet::default()
    }
}

/// `DevLogin` has no creation screen: the look comes from the name.
pub(super) fn dev_look(lists: &defs::AppearanceDef, name: &CharacterName) -> [String; 4] {
    let mut h = Fnv64::new();
    h.write_str(&name.to_ascii_lowercase());
    let n = h.finish();
    let pick = |list: &[defs::LookOption], shift: u32| {
        let len = list.len().max(1) as u64;
        u8::try_from((n >> shift) % len).unwrap_or(0)
    };
    let look = Look {
        body: pick(lists.bodies(), 0),
        skin: pick(lists.skins(), 8),
        hair_style: pick(lists.hair_styles(), 16),
        hair_color: pick(lists.hair_colors(), 24),
    };
    lists.ids_of(look).unwrap_or_default()
}

/// What `Login`, `Register` and `ChangePassword` hand to the session
/// besides the token.
#[derive(Default)]
pub struct AfterLogin {
    /// Attempt keys to clear, and keys whose counted try is given back.
    pub clear: Vec<String>,
    pub refund: Vec<String>,
    pub device: Option<DeviceToken>,
    pub failed_logins: u32,
}

/// A `Login` that passed the waits, for its task.
pub(super) struct LoginRequest {
    pub account: AccountName,
    pub password: Password,
    pub device: Option<DeviceToken>,
    /// Seconds the account itself still waits (only a known device enters).
    pub account_wait: Option<u64>,
    pub ip: IpAddr,
}

/// A new device token for the account; `old` (the one just used) is
/// dropped, so each token works for one login.
async fn issue_device(
    repo: &Repository,
    account: AccountId,
    old: Option<&DeviceToken>,
    now: u64,
) -> Result<DeviceToken, RepoError> {
    if let Some(old) = old {
        repo.delete_device(login::device_hash(old)).await?;
    }
    let token = login::new_device().ok_or(RepoError::Password("no system randomness".into()))?;
    repo.create_device(
        account,
        login::device_hash(&token),
        now,
        now + persistence::DEVICE_SECS,
    )
    .await?;
    Ok(token)
}

pub(super) async fn open_session(
    repo: &Repository,
    c: ConnectionId,
    account: AccountId,
    after: AfterLogin,
    registered: bool,
    must_change: bool,
) -> Result<Done, RepoError> {
    let token = login::new_token().ok_or(RepoError::Password("no system randomness".into()))?;
    let expires = login::now_unix() + persistence::SESSION_SECS;
    repo.create_session(account, login::token_hash(&token), expires)
        .await?;
    let row = repo.account_by_id(account).await?.ok_or(RepoError::NotFound)?;
    Ok(Done::Session {
        c,
        account,
        role: row.role,
        account_name: row.name,
        token,
        characters: repo.characters(account).await?,
        after,
        registered,
        must_change,
    })
}

/// A sanction blocks this connection: same codes as the login answer, so the
/// client shows the same translated message.
pub(super) fn refused(c: ConnectionId, sanction: &Sanction, now: u64, back: Back, keys: &KeyTable) -> Done {
    let (code, args) = sanction_error(sanction, now, keys);
    Done::Failed { c, code, args, back }
}

pub(super) fn sanction_error(sanction: &Sanction, now: u64, keys: &KeyTable) -> (ErrorCode, Vec<Arg>) {
    let reason = || match keys.id(&sanction.reason) {
        Some(id) => Arg::Ref(Box::new(TextRef::new(id))),
        None => {
            warn!(key = %sanction.reason, "sanction reason has no text key");
            Arg::Str(sanction.reason.clone())
        }
    };
    match sanction.kind {
        SanctionKind::Ban => (ErrorCode::AccountBanned, vec![reason()]),
        SanctionKind::Suspend => (
            ErrorCode::AccountSuspended,
            vec![Arg::Int(sanction.seconds_left(now).unwrap_or(0) as i64), reason()],
        ),
        SanctionKind::IpBan => (
            ErrorCode::IpBlocked,
            vec![Arg::Int(sanction.seconds_left(now).unwrap_or(0) as i64)],
        ),
        SanctionKind::Mute => (ErrorCode::NotAllowed, Vec::new()),
    }
}

/// Database error as an answer: rule violations become their codes, the
/// rest is logged and answered with `Internal`.
pub(super) fn failed(c: ConnectionId, e: RepoError, back: Back) -> Done {
    let code = match e {
        RepoError::AccountTaken => ErrorCode::AccountTaken,
        RepoError::NameTaken => ErrorCode::NameTaken,
        RepoError::TooManyCharacters => ErrorCode::TooManyCharacters,
        other => {
            error!(error = %other, "database");
            ErrorCode::Internal
        }
    };
    Done::Failed {
        c,
        code,
        args: Vec::new(),
        back,
    }
}

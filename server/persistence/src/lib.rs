//! Accounts, sessions and characters on disk. `Repository` is an enum with
//! static dispatch; phase 2 has only SQLite (Postgres arrives in phase 11).
//! Blocking database calls run on tokio's blocking pool, never on the
//! network threads.
//!
//!
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod password;
mod sheet;
mod sqlite;

pub use password::{hash_password, verify_password};
pub use sheet::{SHEET_VERSION, SavedItem, SavedPower, SavedProgress, SavedQuest, SavedSheet, SavedStack};
pub use sqlite::SqliteRepo;

use base::{AccountId, AccountName, CharacterId, CharacterName, Locale, TilePos};
use std::collections::BTreeSet;
use std::fmt;
use std::net::IpAddr;

/// SHA-256 of a session token: the token itself is never stored.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokenHash(pub [u8; 32]);

impl fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenHash(***)")
    }
}

/// Account role. `Admin` may use the in-game admin commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Player,
    Admin,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "player" => Some(Self::Player),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountRow {
    pub id: AccountId,
    /// Lower case, as stored.
    pub name: String,
    /// `None` for the `dev` account, which cannot log in.
    pub password_hash: Option<String>,
    pub locale: Locale,
    pub role: Role,
    /// A temporary password was given by an admin: change it before playing.
    pub must_change_password: bool,
}

/// Punishment kind. `Mute` only has an effect with chat (phase 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SanctionKind {
    Mute,
    Suspend,
    Ban,
    IpBan,
}

impl SanctionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mute => "mute",
            Self::Suspend => "suspend",
            Self::Ban => "ban",
            Self::IpBan => "ip_ban",
        }
    }

    pub fn parse(s: &str) -> Option<SanctionKind> {
        match s {
            "mute" => Some(Self::Mute),
            "suspend" => Some(Self::Suspend),
            "ban" => Some(Self::Ban),
            "ip_ban" => Some(Self::IpBan),
            _ => None,
        }
    }

    /// Kinds that refuse a login while active.
    pub const fn blocks_login(self) -> bool {
        matches!(self, Self::Suspend | Self::Ban | Self::IpBan)
    }
}

impl fmt::Display for SanctionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of `sanctions`. Nothing is deleted: a revoked sanction keeps its
/// row and gets `revoked_at`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sanction {
    pub id: SanctionId,
    pub kind: SanctionKind,
    /// Account sanction (`mute`, `suspend`, `ban`) ...
    pub account: Option<AccountId>,
    /// ... or IP sanction (`ip_ban`), already normalized by [`ip_key`].
    pub ip: Option<String>,
    /// Text key of the reason, translated by the client.
    pub reason: String,
    /// Free note: audit only, never shown to the player.
    pub note: String,
    pub created_at: u64,
    /// `None` for permanent bans.
    pub expires_at: Option<u64>,
    pub revoked_at: Option<u64>,
    pub revoked_by: Option<String>,
    pub created_by: String,
}

impl Sanction {
    /// Active now: not revoked and not expired.
    pub fn active_at(&self, now: u64) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|end| end > now)
    }

    /// Seconds left, if it expires.
    pub fn seconds_left(&self, now: u64) -> Option<u64> {
        self.expires_at.map(|end| end.saturating_sub(now))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SanctionId(pub u64);

impl fmt::Display for SanctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A sanction to create. `account` and `ip` are exclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSanction {
    pub kind: SanctionKind,
    pub account: Option<AccountId>,
    pub ip: Option<String>,
    /// Account name or IP, for the audit log.
    pub target: String,
    pub reason: String,
    pub note: String,
    /// `None` only for a permanent `ban`.
    pub expires_at: Option<u64>,
    pub created_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminLogRow {
    pub id: u64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub args: String,
    pub note: String,
    pub created_at: u64,
}

/// A character the player deleted; it can be restored by an admin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletedCharacterRow {
    pub id: CharacterId,
    pub name: CharacterName,
    pub deleted_at: u64,
}

/// A key's wrong passwords and wait, saved so a restart does
/// not reset them. Unix seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginBlock {
    pub key: String,
    pub count: u32,
    pub last_at: u64,
    pub blocked_until: u64,
}

/// An account with wrong passwords since its last login (admin report).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedLoginRow {
    pub name: String,
    pub failed_logins: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRow {
    pub ip: String,
    pub created_at: u64,
}

/// Normalizes an IP for `ip_ban` and `logins`: IPv4 as is, IPv6 by its /64
/// prefix, because one address is often shared.
pub fn ip_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let mut octets = v6.octets();
            for b in &mut octets[8..] {
                *b = 0;
            }
            format!("{}/64", std::net::Ipv6Addr::from(octets))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterRow {
    pub id: CharacterId,
    pub name: CharacterName,
    /// From the sheet, for the selection screen (phase 5).
    pub level: u8,
    pub look: [String; 4],
}

/// A chat line kept with a report. Chat text has no control characters,
/// so lines are stored as tab-separated text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    pub at: u64,
    /// `local` or `whisper:<receiver>`.
    pub channel: String,
    pub from: String,
    pub text: String,
}

/// A player report (phase 5, `admin reports`): who, whom, and the last chat
/// lines the reporter could see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatReport {
    pub id: u64,
    pub reporter: String,
    pub target: String,
    pub lines: Vec<ChatLine>,
    pub created_at: u64,
}

/// Chat reports older than this are deleted when a new one arrives: 7 days.
pub const CHAT_REPORT_KEEP_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedCharacter {
    pub id: CharacterId,
    pub account: AccountId,
    pub name: CharacterName,
    /// Where it was saved; `None` until the first save (enters at the spawn).
    pub place: Option<(String, TilePos)>,
    pub sheet: SavedSheet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterSave {
    pub id: CharacterId,
    pub zone: String,
    pub pos: TilePos,
    pub sheet: SavedSheet,
}

/// What custody holds (rule 4): each piece whole (id, crafter,
/// rarity, durability, hidden bonuses), never a copy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EscrowParcel {
    pub coins: u64,
    pub stacks: Vec<SavedStack>,
    pub items: Vec<SavedItem>,
}

/// One parcel waiting in custody.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscrowRow {
    pub id: u64,
    /// `None`: put there by the server (a delivery that found no room).
    pub sender: Option<CharacterId>,
    pub recipient: CharacterId,
    pub parcel: EscrowParcel,
    pub created_at: u64,
}

/// One telemetry sample: written by every batch save (about every 30 s
/// while the character plays). Kept for `TELEMETRY_KEEP_SECS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryRow {
    pub character: CharacterId,
    pub at: u64,
    pub level: u8,
    /// Sum of the `kills` counters.
    pub kills: u64,
}

/// Coins created or destroyed by one source, summed over a batch save
/// (phase 7). `created`: a source; otherwise a
/// sink. `flow` is the name the simulation gives it (`monster_loot`,
/// `shop_purchase`...), kept as text so a new source needs no migration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoinFlowRow {
    pub at: u64,
    pub flow: String,
    pub created: bool,
    pub amount: u64,
}

/// Telemetry samples older than this are deleted on save: 7 days.
pub const TELEMETRY_KEEP_SECS: u64 = 7 * 24 * 60 * 60;

/// Backup copies are `game-<unix>.db` in the backup folder
pub const BACKUP_PREFIX: &str = "game-";
/// The newest hourly copies kept.
pub const BACKUP_KEEP_HOURLY: usize = 48;
/// Days the first copy of each day is kept.
pub const BACKUP_KEEP_DAYS: u64 = 30;

/// What one backup did: the copy written and the old ones deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupDone {
    pub file: std::path::PathBuf,
    pub removed: Vec<std::path::PathBuf>,
}

/// The unix second of a backup file name (`game-1763042400.db`).
pub fn backup_stamp(file_name: &str) -> Option<u64> {
    file_name
        .strip_prefix(BACKUP_PREFIX)?
        .strip_suffix(".db")?
        .parse()
        .ok()
}

/// Retention: the `BACKUP_KEEP_HOURLY` newest copies, plus the first copy
/// of each day (UTC) for `BACKUP_KEEP_DAYS` days. Returns the stamps to
/// delete, oldest first.
pub fn backups_to_remove(stamps: &[u64], now: u64) -> Vec<u64> {
    const DAY: u64 = 24 * 60 * 60;
    let mut sorted = stamps.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let newest: Vec<u64> = sorted.iter().rev().take(BACKUP_KEEP_HOURLY).copied().collect();
    let oldest_day = now.saturating_sub(BACKUP_KEEP_DAYS * DAY) / DAY;
    let mut first_of_day = BTreeSet::new();
    let mut days_seen = BTreeSet::new();
    for &s in &sorted {
        if days_seen.insert(s / DAY) && s / DAY >= oldest_day {
            first_of_day.insert(s);
        }
    }
    sorted
        .into_iter()
        .filter(|s| !newest.contains(s) && !first_of_day.contains(s))
        .collect()
}

/// How long a session token is valid.
pub const SESSION_SECS: u64 = 24 * 60 * 60;

/// Live characters an account may have (creating and restoring).
pub const CHARACTERS_PER_ACCOUNT: u32 = 5;

/// How long a device token is valid.
pub const DEVICE_SECS: u64 = 90 * 24 * 60 * 60;
/// Device tokens kept per account; the oldest goes first.
pub const DEVICES_PER_ACCOUNT: usize = 10;

/// Name of the account that holds the `DevLogin` characters.
pub const DEV_ACCOUNT: &str = "dev";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoError {
    AccountTaken,
    NameTaken,
    TooManyCharacters,
    /// A character or sanction id that does not exist.
    NotFound,
    /// Stored data that does not decode (unknown sheet version, bad name).
    Corrupt(String),
    Db(String),
    /// The blocking task did not finish (runtime shutting down).
    Task,
    BadUrl(String),
    Password(String),
    /// This piece belongs to another character or to custody;
    /// the whole transaction was refused.
    ItemOwned(u64),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountTaken => f.write_str("account name already used"),
            Self::NameTaken => f.write_str("character name already used"),
            Self::TooManyCharacters => f.write_str("account has the maximum number of characters"),
            Self::NotFound => f.write_str("not found"),
            Self::Corrupt(e) => write!(f, "stored data is corrupt: {e}"),
            Self::Db(e) => write!(f, "database: {e}"),
            Self::Task => f.write_str("database task did not finish"),
            Self::BadUrl(u) => write!(f, "unsupported database url `{u}` (use sqlite:PATH)"),
            Self::Password(e) => write!(f, "password hashing: {e}"),
            Self::ItemOwned(id) => write!(f, "item {id} already has an owner"),
        }
    }
}

impl std::error::Error for RepoError {}

/// Who holds a unique title or class (`unique_holders`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniqueHolder {
    /// `title:<id>` or `class:<id>`.
    pub key: String,
    pub character: CharacterId,
    pub name: CharacterName,
    /// Unix second of the holder's last save; 0 for a deleted character.
    pub last_seen: u64,
}

pub enum Repository {
    Sqlite(SqliteRepo),
}

impl Repository {
    /// `sqlite:PATH` or `sqlite::memory:` (tests). Creates the file and
    /// applies schema migrations in order.
    pub fn open(url: &str) -> Result<Repository, RepoError> {
        match url.strip_prefix("sqlite:") {
            Some(path) => Ok(Repository::Sqlite(SqliteRepo::open(path)?)),
            None => Err(RepoError::BadUrl(url.to_owned())),
        }
    }

    pub async fn create_account(
        &self,
        name: &AccountName,
        password_hash: Option<String>,
        now: u64,
    ) -> Result<AccountId, RepoError> {
        match self {
            Repository::Sqlite(r) => r.create_account(name.clone(), password_hash, now).await,
        }
    }

    pub async fn account(&self, name: &AccountName) -> Result<Option<AccountRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.account(name.clone()).await,
        }
    }

    pub async fn account_by_id(&self, id: AccountId) -> Result<Option<AccountRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.account_by_id(id).await,
        }
    }

    pub async fn set_locale(&self, account: AccountId, locale: Locale) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.set_locale(account, locale).await,
        }
    }

    pub async fn create_session(
        &self,
        account: AccountId,
        token: TokenHash,
        expires_at: u64,
    ) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.create_session(account, token, expires_at).await,
        }
    }

    /// Uses the session up: a token works once. `None` if unknown or expired.
    pub async fn take_session(&self, token: TokenHash, now: u64) -> Result<Option<AccountId>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.take_session(token, now).await,
        }
    }

    pub async fn characters(&self, account: AccountId) -> Result<Vec<CharacterRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.characters(account).await,
        }
    }

    pub async fn create_character(
        &self,
        account: AccountId,
        name: &CharacterName,
        limit: u32,
        sheet: &SavedSheet,
        now: u64,
    ) -> Result<CharacterId, RepoError> {
        match self {
            Repository::Sqlite(r) => {
                r.create_character(account, name.clone(), limit, sheet.clone(), now)
                    .await
            }
        }
    }

    pub async fn load_character(&self, id: CharacterId) -> Result<Option<LoadedCharacter>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.load_character(id).await,
        }
    }

    /// The `DevLogin` character with this name, created in the `dev`
    /// account if new. `NameTaken` if a real account owns the name.
    pub async fn dev_character(
        &self,
        name: &CharacterName,
        sheet: &SavedSheet,
        now: u64,
    ) -> Result<LoadedCharacter, RepoError> {
        match self {
            Repository::Sqlite(r) => r.dev_character(name.clone(), sheet.clone(), now).await,
        }
    }

    /// All saves in one transaction: they are from the same tick. Items
    /// with identity go to `item_owners` in the same transaction; a save
    /// holding an item another character owns is refused alone and
    /// returned as `(character, item id)` (: never two owners).
    pub async fn save_zone_batch(
        &self,
        saves: Vec<CharacterSave>,
        now: u64,
    ) -> Result<Vec<(CharacterId, u64)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.save_zone_batch(saves, now).await,
        }
    }

    /// Rule 4: the sender's sheet (without the parcel), the
    /// parcel for the character named `to`, and its pieces moved to
    /// custody in `item_owners`, in one transaction. `NotFound`: no live
    /// character with that name (or the sender itself); `ItemOwned`: a
    /// piece is not the sender's. On any error nothing was written.
    pub async fn commit_escrow(
        &self,
        save: CharacterSave,
        to: CharacterName,
        parcel: EscrowParcel,
        now: u64,
    ) -> Result<u64, RepoError> {
        match self {
            Repository::Sqlite(r) => r.commit_escrow(save, to, parcel, now).await,
        }
    }

    /// A parcel back into custody for `recipient` with no sheet: a delivery
    /// with no room, or a refused deposit whose sender already left. Its
    /// pieces must have no owner, or be `sender`'s or `recipient`'s.
    pub async fn restore_escrow(
        &self,
        sender: Option<CharacterId>,
        recipient: CharacterId,
        parcel: EscrowParcel,
        now: u64,
    ) -> Result<u64, RepoError> {
        match self {
            Repository::Sqlite(r) => r.restore_escrow(sender, recipient, parcel, now).await,
        }
    }

    /// Parcels waiting for `recipient`, oldest first.
    pub async fn escrows_for(&self, recipient: CharacterId) -> Result<Vec<EscrowRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.escrows_for(recipient).await,
        }
    }

    /// Takes one parcel out of custody (row and piece owners gone in one
    /// transaction). `None`: no such parcel for this recipient (already
    /// taken). The pieces get their owner with the recipient's next save.
    pub async fn take_escrow(
        &self,
        id: u64,
        recipient: CharacterId,
    ) -> Result<Option<EscrowParcel>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.take_escrow(id, recipient).await,
        }
    }

    /// Reserves `count` item ids for a zone: `[start, end)`.
    pub async fn reserve_item_ids(&self, count: u64) -> Result<(u64, u64), RepoError> {
        match self {
            Repository::Sqlite(r) => r.reserve_item_ids(count).await,
        }
    }

    pub async fn item_owner(&self, item: u64) -> Result<Option<CharacterId>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.item_owner(item).await,
        }
    }

    /// Telemetry samples since `since`, by character then time.
    pub async fn telemetry(&self, since: u64) -> Result<Vec<TelemetryRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.telemetry(since).await,
        }
    }

    /// Coins created and destroyed by source, summed per batch save (phase 7).
    pub async fn save_coin_flow(&self, rows: Vec<CoinFlowRow>) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.save_coin_flow(rows).await,
        }
    }

    /// Coin flow samples since `since`, oldest first.
    pub async fn coin_flow(&self, since: u64) -> Result<Vec<CoinFlowRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.coin_flow(since).await,
        }
    }

    /// Every character with its sheet, by id (admin reports).
    pub async fn all_sheets(&self) -> Result<Vec<(CharacterId, CharacterName, SavedSheet)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.all_sheets().await,
        }
    }

    /// Replaces the password hash and the `must_change_password` flag.
    pub async fn set_password(
        &self,
        account: AccountId,
        password_hash: Option<String>,
        must_change: bool,
    ) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.set_password(account, password_hash, must_change).await,
        }
    }

    /// Drops every session of the account (punishment, password change).
    pub async fn delete_sessions(&self, account: AccountId) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.delete_sessions(account).await,
        }
    }

    /// A device token for the account (hash only); drops expired
    /// ones and keeps the newest `DEVICES_PER_ACCOUNT` of the account.
    pub async fn create_device(
        &self,
        account: AccountId,
        token: TokenHash,
        now: u64,
        expires_at: u64,
    ) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.create_device(account, token, now, expires_at).await,
        }
    }

    /// The token belongs to the account and has not expired.
    pub async fn device_ok(&self, account: AccountId, token: TokenHash, now: u64) -> Result<bool, RepoError> {
        match self {
            Repository::Sqlite(r) => r.device_ok(account, token, now).await,
        }
    }

    pub async fn delete_device(&self, token: TokenHash) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.delete_device(token).await,
        }
    }

    /// Every device of the account (the password changed).
    pub async fn delete_devices(&self, account: AccountId) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.delete_devices(account).await,
        }
    }

    /// One more wrong password for the account.
    pub async fn add_failed_login(&self, account: AccountId) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.add_failed_login(account).await,
        }
    }

    /// Wrong passwords since the last login, set back to 0.
    pub async fn take_failed_logins(&self, account: AccountId) -> Result<u32, RepoError> {
        match self {
            Repository::Sqlite(r) => r.take_failed_logins(account).await,
        }
    }

    /// Accounts with at least `min` wrong passwords since their last
    /// login, most first.
    pub async fn failed_logins(&self, min: u32) -> Result<Vec<FailedLoginRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.failed_logins(min).await,
        }
    }

    pub async fn login_blocks(&self) -> Result<Vec<LoginBlock>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.login_blocks().await,
        }
    }

    pub async fn save_login_blocks(&self, blocks: Vec<LoginBlock>) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.save_login_blocks(blocks).await,
        }
    }

    /// `true` when `character` is the first of the server to reach `key`.
    pub async fn record_first(&self, key: &str, character: CharacterId, now: u64) -> Result<bool, RepoError> {
        match self {
            Repository::Sqlite(r) => r.record_first(key, character, now).await,
        }
    }

    /// Every server goal saved: id, progress, unix second it was reached.
    pub async fn server_goals(&self) -> Result<Vec<(String, u64, Option<u64>)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.server_goals().await,
        }
    }

    /// Writes the state of server goals (`(id, progress, done_at)`).
    pub async fn save_server_goals(&self, goals: Vec<(String, u64, Option<u64>)>) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.save_server_goals(goals).await,
        }
    }

    /// Every server first: key, character name, unix second, oldest first.
    pub async fn server_firsts(&self) -> Result<Vec<(String, CharacterName, u64)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.server_firsts().await,
        }
    }

    /// Takes a unique `key` for `character` if free; returns the holder.
    pub async fn claim_unique(
        &self,
        key: &str,
        character: CharacterId,
        now: u64,
    ) -> Result<CharacterId, RepoError> {
        match self {
            Repository::Sqlite(r) => r.claim_unique(key, character, now).await,
        }
    }

    /// Every unique holder, by key.
    pub async fn unique_holders(&self) -> Result<Vec<UniqueHolder>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.unique_holders().await,
        }
    }

    /// Frees a unique `key` whoever holds it.
    pub async fn release_key(&self, key: &str) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.release_key(key).await,
        }
    }

    /// Frees a unique `key` if `character` holds it.
    pub async fn release_unique(&self, key: &str, character: CharacterId) -> Result<bool, RepoError> {
        match self {
            Repository::Sqlite(r) => r.release_unique(key, character).await,
        }
    }

    /// Records a dungeon clear.
    pub async fn add_clear(&self, dungeon: &str, party: &str, secs: u64, now: u64) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.add_clear(dungeon, party, secs, now).await,
        }
    }

    /// The fastest clears of a dungeon: party and seconds.
    pub async fn fastest_clears(&self, dungeon: &str, limit: u32) -> Result<Vec<(String, u64)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.fastest_clears(dungeon, limit).await,
        }
    }

    /// `character` may not enter `dungeon` before `until`.
    pub async fn set_lockout(
        &self,
        character: CharacterId,
        dungeon: &str,
        until: u64,
    ) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.set_lockout(character, dungeon, until).await,
        }
    }

    /// Lockouts still running at `now`.
    pub async fn lockouts(&self, now: u64) -> Result<Vec<(CharacterId, String, u64)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.lockouts(now).await,
        }
    }

    /// Appends to `logins` and drops rows older than 30 days.
    pub async fn record_login(&self, account: AccountId, ip: &str, now: u64) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.record_login(account, ip, now).await,
        }
    }

    /// Strongest active account sanction: ban, then suspend, then mute.
    pub async fn account_sanction(
        &self,
        account: AccountId,
        now: u64,
    ) -> Result<Option<Sanction>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.account_sanction(account, now).await,
        }
    }

    pub async fn ip_sanction(&self, ip: &str, now: u64) -> Result<Option<Sanction>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.ip_sanction(ip, now).await,
        }
    }

    /// Sanctions created or revoked at or after `since`, for the server to
    /// apply without a restart.
    pub async fn sanctions_changed_since(&self, since: u64) -> Result<Vec<Sanction>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.sanctions_changed_since(since).await,
        }
    }

    pub async fn sanctions_of(&self, account: AccountId) -> Result<Vec<Sanction>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.sanctions_of(account).await,
        }
    }

    /// Writes the sanction and its audit row; suspend and ban also drop the
    /// account's sessions.
    pub async fn add_sanction(&self, new: NewSanction, now: u64) -> Result<SanctionId, RepoError> {
        match self {
            Repository::Sqlite(r) => r.add_sanction(new, now).await,
        }
    }

    /// `false` when the id does not exist or was already revoked.
    pub async fn revoke_sanction(&self, id: SanctionId, actor: &str, now: u64) -> Result<bool, RepoError> {
        match self {
            Repository::Sqlite(r) => r.revoke_sanction(id, actor, now).await,
        }
    }

    pub async fn set_role(
        &self,
        account: AccountId,
        role: Role,
        actor: &str,
        target: &str,
        now: u64,
    ) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.set_role(account, role, actor, target, now).await,
        }
    }

    /// Temporary password: sets the hash, marks `must_change_password` and
    /// drops every session.
    pub async fn reset_password(
        &self,
        account: AccountId,
        password_hash: String,
        actor: &str,
        target: &str,
        now: u64,
    ) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.reset_password(account, password_hash, actor, target, now).await,
        }
    }

    /// Renames a character; the old name is freed and the new one reserved.
    pub async fn rename_character(
        &self,
        old: &CharacterName,
        new: &CharacterName,
        actor: &str,
        now: u64,
    ) -> Result<CharacterId, RepoError> {
        match self {
            Repository::Sqlite(r) => r.rename_character(old, new, actor, now).await,
        }
    }

    pub async fn recent_logins(&self, account: AccountId, limit: u32) -> Result<Vec<LoginRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.recent_logins(account, limit).await,
        }
    }

    /// Audit rows, newest first. `account` matches the target.
    pub async fn admin_log(
        &self,
        account: Option<&str>,
        since: Option<u64>,
        limit: u32,
    ) -> Result<Vec<AdminLogRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.admin_log(account, since, limit).await,
        }
    }

    /// Audit rows at or after `since`, oldest first (server polling).
    pub async fn admin_log_since(&self, since: u64) -> Result<Vec<AdminLogRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.admin_log_since(since).await,
        }
    }

    /// Id, account and account name of a character, by name (any case).
    pub async fn character_by_name(
        &self,
        name: &CharacterName,
    ) -> Result<Option<(CharacterId, AccountId, String)>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.character_by_name(name.clone()).await,
        }
    }

    /// End of the longest active mute of the account (`u64::MAX`: none).
    pub async fn mute_until(&self, account: AccountId, now: u64) -> Result<Option<u64>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.mute_until(account, now).await,
        }
    }

    /// Audit row of an in-game admin action that is not a sanction.
    pub async fn log_admin(&self, row: AdminLogRow) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.log_admin(row).await,
        }
    }

    pub async fn add_chat_report(&self, report: ChatReport) -> Result<(), RepoError> {
        match self {
            Repository::Sqlite(r) => r.add_chat_report(report).await,
        }
    }

    /// Chat reports at or after `since`, newest first.
    pub async fn chat_reports(&self, since: u64) -> Result<Vec<ChatReport>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.chat_reports(since).await,
        }
    }

    /// Deletes a character of the account: hidden, name freed,
    /// its items with identity gone; the row stays for the audit. `actor`
    /// is the account name for `admin_log`.
    pub async fn delete_character(
        &self,
        account: AccountId,
        character: CharacterId,
        actor: &str,
        now: u64,
    ) -> Result<CharacterName, RepoError> {
        match self {
            Repository::Sqlite(r) => {
                r.delete_character(account, character, actor.to_owned(), now)
                    .await
            }
        }
    }

    /// A consistent copy into `dir` and the retention applied
    pub async fn deleted_characters(
        &self,
        account: AccountId,
    ) -> Result<Vec<DeletedCharacterRow>, RepoError> {
        match self {
            Repository::Sqlite(r) => r.deleted_characters(account).await,
        }
    }

    /// Restores a deleted character (P01): `TooManyCharacters` when the
    /// account has `limit` already, `NameTaken` when the name was reused.
    pub async fn restore_character(
        &self,
        id: CharacterId,
        limit: u32,
        actor: &str,
        now: u64,
    ) -> Result<CharacterName, RepoError> {
        match self {
            Repository::Sqlite(r) => r.restore_character(id, limit, actor.to_owned(), now).await,
        }
    }

    pub async fn backup(&self, dir: std::path::PathBuf, now: u64) -> Result<BackupDone, RepoError> {
        match self {
            Repository::Sqlite(r) => r.backup(dir, now).await,
        }
    }

    /// Read-only check of a backup copy: integrity, known schema version,
    /// and the number of accounts and characters.
    pub fn verify_backup(path: &str) -> Result<(u64, u64), RepoError> {
        SqliteRepo::verify_backup(path)
    }
}

#[cfg(test)]
mod tests;

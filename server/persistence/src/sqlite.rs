//! SQLite implementation. One connection behind a mutex; every call runs on
//! tokio's blocking pool. The schema version is `PRAGMA user_version`;
//! migrations run in order when the file is opened.

use crate::sheet::{decode, encode};
use crate::{
    AccountRow, AdminLogRow, CHAT_REPORT_KEEP_SECS, CharacterRow, CharacterSave, ChatLine, ChatReport,
    CoinFlowRow, DEV_ACCOUNT, DeletedCharacterRow, EscrowParcel, EscrowRow, FailedLoginRow, LoadedCharacter,
    LoginBlock, LoginRow, NewSanction, RepoError, Role, Sanction, SanctionId, SanctionKind, SavedSheet,
    TokenHash, UniqueHolder,
};
use base::{AccountId, AccountName, CharacterId, CharacterName, Locale, TilePos};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params, params_from_iter};
use std::sync::{Arc, Mutex, PoisonError};

/// Each entry moves the schema one version up. Append only.
const MIGRATIONS: &[&str] = &[
    // 1: accounts, sessions, characters (phase 2).
    "CREATE TABLE accounts (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        password_hash TEXT,
        locale TEXT NOT NULL DEFAULT 'en',
        created_at INTEGER NOT NULL
    );
    CREATE TABLE sessions (
        token_hash BLOB PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        expires_at INTEGER NOT NULL
    );
    CREATE INDEX sessions_expiry ON sessions(expires_at);
    CREATE TABLE characters (
        id INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        name TEXT NOT NULL,
        name_key TEXT NOT NULL UNIQUE,
        zone TEXT,
        x INTEGER,
        y INTEGER,
        version INTEGER NOT NULL,
        sheet BLOB NOT NULL,
        created_at INTEGER NOT NULL,
        saved_at INTEGER NOT NULL
    );
    CREATE INDEX characters_account ON characters(account_id);",
    // 2: moderation (phase 2): roles, temporary passwords,
    // sanctions, audit log and login history.
    "ALTER TABLE accounts ADD COLUMN role TEXT NOT NULL DEFAULT 'player';
    ALTER TABLE accounts ADD COLUMN must_change_password INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE sanctions (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        account_id INTEGER REFERENCES accounts(id),
        ip TEXT,
        reason TEXT NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        expires_at INTEGER,
        revoked_at INTEGER,
        revoked_by TEXT,
        created_by TEXT NOT NULL,
        CHECK ((account_id IS NOT NULL) <> (ip IS NOT NULL))
    );
    CREATE INDEX sanctions_account ON sanctions(account_id);
    CREATE INDEX sanctions_ip ON sanctions(ip);
    CREATE TABLE admin_log (
        id INTEGER PRIMARY KEY,
        actor TEXT NOT NULL,
        action TEXT NOT NULL,
        target TEXT NOT NULL,
        args TEXT NOT NULL DEFAULT '',
        note TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    );
    CREATE INDEX admin_log_created ON admin_log(created_at);
    CREATE TABLE logins (
        id INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        ip TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX logins_account ON logins(account_id);
    CREATE INDEX logins_created ON logins(created_at);",
    // Phase 4: who holds each item with identity and the id
    // counter zones reserve blocks from.
    "CREATE TABLE item_owners (
        item_id INTEGER PRIMARY KEY,
        owner_kind TEXT NOT NULL DEFAULT 'character',
        character_id INTEGER REFERENCES characters(id),
        item TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX item_owners_character ON item_owners(character_id);
    CREATE TABLE id_counters (name TEXT PRIMARY KEY, next INTEGER NOT NULL);
    INSERT INTO id_counters (name, next) VALUES ('item', 1);",
    // Phase 4: one sample per character per batch save, for the bot
    // telemetry behind `admin suspects`.
    "CREATE TABLE telemetry (
        character_id INTEGER NOT NULL REFERENCES characters(id),
        at INTEGER NOT NULL,
        level INTEGER NOT NULL,
        kills INTEGER NOT NULL
    );
    CREATE INDEX telemetry_character ON telemetry(character_id, at);
    CREATE INDEX telemetry_at ON telemetry(at);",
    // Phase 5: chat reports with the lines the reporter could see, kept
    // `CHAT_REPORT_KEEP_SECS` (`admin reports`).
    "CREATE TABLE chat_reports (
        id INTEGER PRIMARY KEY,
        reporter TEXT NOT NULL,
        target TEXT NOT NULL,
        lines TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX chat_reports_created ON chat_reports(created_at);",
    // Phase 7: coins created and destroyed, by source and sink, for the
    // economy telemetry. One row per source
    // per batch save, kept `TELEMETRY_KEEP_SECS`.
    "CREATE TABLE coin_flow (
        at INTEGER NOT NULL,
        flow TEXT NOT NULL,
        created INTEGER NOT NULL,
        amount INTEGER NOT NULL
    );
    CREATE INDEX coin_flow_at ON coin_flow(at);",
    // Alpha: a deleted character keeps its row and sheet for the
    // audit; `deleted_at` hides it and its `name_key` becomes
    // `#deleted:<id>`, which frees the name.
    "ALTER TABLE characters ADD COLUMN deleted_at INTEGER;",
    // Phase 11 (rule 4): custody. A parcel (coins, stacks and
    // pieces, encoded as a sheet with its version) waits for `recipient`;
    // its pieces are `item_owners` rows of kind `escrow` with `escrow_id`.
    "CREATE TABLE escrow (
        id INTEGER PRIMARY KEY,
        sender INTEGER REFERENCES characters(id),
        recipient INTEGER NOT NULL REFERENCES characters(id),
        version INTEGER NOT NULL,
        parcel BLOB NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX escrow_recipient ON escrow(recipient);
    ALTER TABLE item_owners ADD COLUMN escrow_id INTEGER;
    CREATE INDEX item_owners_escrow ON item_owners(escrow_id);",
    // Device tokens (hash only), wrong passwords since the last
    // login, and the waits after wrong passwords, kept across restarts.
    "CREATE TABLE devices (
        token_hash BLOB PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    );
    CREATE INDEX devices_account ON devices(account_id);
    ALTER TABLE accounts ADD COLUMN failed_logins INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE login_blocks (
        key TEXT PRIMARY KEY,
        count INTEGER NOT NULL,
        last_at INTEGER NOT NULL,
        blocked_until INTEGER NOT NULL
    );",
    // Server firsts (a title first earned on the server, kept for good)
    // and unique holders (a title or class only one character holds; the
    // row goes when the holder stops playing and an heir may take it).
    "CREATE TABLE server_firsts (
        key TEXT PRIMARY KEY,
        character_id INTEGER NOT NULL REFERENCES characters(id),
        at INTEGER NOT NULL
    );
    CREATE TABLE unique_holders (
        key TEXT PRIMARY KEY,
        character_id INTEGER NOT NULL REFERENCES characters(id),
        since INTEGER NOT NULL
    );",
    // Dungeons: every clear (party names, seconds) for the fastest-clear
    // rankings, and who may not enter a dungeon until when.
    "CREATE TABLE dungeon_clears (
        dungeon TEXT NOT NULL,
        party TEXT NOT NULL,
        secs INTEGER NOT NULL,
        at INTEGER NOT NULL
    );
    CREATE INDEX dungeon_clears_time ON dungeon_clears(dungeon, secs);
    CREATE TABLE lockouts (
        character_id INTEGER NOT NULL REFERENCES characters(id),
        dungeon TEXT NOT NULL,
        until INTEGER NOT NULL,
        PRIMARY KEY (character_id, dungeon)
    );",
];

/// `name_key` of a deleted character: no valid name starts with `#`.
fn deleted_key(id: i64) -> String {
    format!("#deleted:{id}")
}

pub struct SqliteRepo {
    conn: Arc<Mutex<Connection>>,
    /// The database file (`None` in memory): backups read it through a
    /// connection of their own, so the game's saves never wait for a copy.
    path: Option<std::path::PathBuf>,
}

fn db(e: rusqlite::Error) -> RepoError {
    RepoError::Db(e.to_string())
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation)
}

fn id_u32(v: i64) -> Result<u32, RepoError> {
    u32::try_from(v).map_err(|_| RepoError::Corrupt(format!("id {v} out of range")))
}

fn name_key(name: &CharacterName) -> String {
    name.as_str().to_ascii_lowercase()
}

impl SqliteRepo {
    pub fn open(path: &str) -> Result<SqliteRepo, RepoError> {
        let mut conn = if path == ":memory:" {
            Connection::open_in_memory()
        } else {
            if let Some(dir) = std::path::Path::new(path).parent()
                && !dir.as_os_str().is_empty()
            {
                std::fs::create_dir_all(dir).map_err(|e| RepoError::Db(format!("{}: {e}", dir.display())))?;
            }
            Connection::open(path)
        }
        .map_err(db)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(db)?;
        migrate(&mut conn)?;
        Ok(SqliteRepo {
            conn: Arc::new(Mutex::new(conn)),
            path: (path != ":memory:").then(|| std::path::PathBuf::from(path)),
        })
    }

    /// `VACUUM INTO <dir>/game-<now>.db`: a consistent copy taken while the
    /// game runs, then the copies past the retention are deleted
    /// ([`crate::backups_to_remove`]).
    pub async fn backup(&self, dir: std::path::PathBuf, now: u64) -> Result<crate::BackupDone, RepoError> {
        let path = self.path.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&dir).map_err(|e| RepoError::Db(format!("{}: {e}", dir.display())))?;
            let file = dir.join(format!("{}{now}.db", crate::BACKUP_PREFIX));
            let target = file.to_string_lossy().into_owned();
            match &path {
                // A reader of its own: WAL lets it copy beside the writer.
                Some(p) => {
                    let reader = Connection::open_with_flags(p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .map_err(db)?;
                    reader.execute("VACUUM INTO ?1", params![target]).map_err(db)?;
                }
                None => {
                    let c = conn.lock().unwrap_or_else(PoisonError::into_inner);
                    c.execute("VACUUM INTO ?1", params![target]).map_err(db)?;
                }
            }
            let mut stamps = Vec::new();
            for entry in std::fs::read_dir(&dir).map_err(|e| RepoError::Db(e.to_string()))? {
                let name = entry.map_err(|e| RepoError::Db(e.to_string()))?.file_name();
                if let Some(stamp) = crate::backup_stamp(&name.to_string_lossy()) {
                    stamps.push(stamp);
                }
            }
            let mut removed = Vec::new();
            for stamp in crate::backups_to_remove(&stamps, now) {
                let old = dir.join(format!("{}{stamp}.db", crate::BACKUP_PREFIX));
                std::fs::remove_file(&old).map_err(|e| RepoError::Db(format!("{}: {e}", old.display())))?;
                removed.push(old);
            }
            Ok(crate::BackupDone { file, removed })
        })
        .await
        .map_err(|_| RepoError::Task)?
    }

    /// Hides the character and frees its name; its items with identity
    /// die with it (their `item_owners` rows go). The row and the sheet stay
    /// for the audit. `NotFound` when the account does not own
    /// a live character with this id.
    pub async fn delete_character(
        &self,
        account: AccountId,
        id: CharacterId,
        actor: String,
        now: u64,
    ) -> Result<CharacterName, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let name: Option<String> = tx
                .query_row(
                    "SELECT name FROM characters WHERE id = ?1 AND account_id = ?2 AND deleted_at IS NULL",
                    params![id.get(), account.get()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db)?;
            let Some(name) = name else {
                return Err(RepoError::NotFound);
            };
            tx.execute(
                "UPDATE characters SET deleted_at = ?1, name_key = ?2 WHERE id = ?3",
                params![now as i64, deleted_key(i64::from(id.get())), id.get()],
            )
            .map_err(db)?;
            let items = tx
                .execute(
                    "DELETE FROM item_owners WHERE character_id = ?1",
                    params![id.get()],
                )
                .map_err(db)?;
            let args = format!("id {} items {items}", id.get());
            insert_log(&tx, &actor, "delete_character", &name, &args, "", now)?;
            tx.commit().map_err(db)?;
            CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))
        })
        .await
    }

    /// Characters of the account deleted by the player, newest
    /// first.
    pub async fn deleted_characters(
        &self,
        account: AccountId,
    ) -> Result<Vec<DeletedCharacterRow>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT id, name, deleted_at FROM characters
                     WHERE account_id = ?1 AND deleted_at IS NOT NULL ORDER BY deleted_at DESC, id DESC",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![account.get()], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                let (id, name, at) = row.map_err(db)?;
                out.push(DeletedCharacterRow {
                    id: CharacterId::new(id_u32(id)?),
                    name: CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))?,
                    deleted_at: at as u64,
                });
            }
            Ok(out)
        })
        .await
    }

    /// Brings a deleted character back with its sheet (P01). Refused when
    /// the account is full or the name was taken meanwhile; pieces of the
    /// sheet are claimed again in `item_owners` (ids are never reused, so
    /// a piece another owner holds is corruption and refuses).
    pub async fn restore_character(
        &self,
        id: CharacterId,
        limit: u32,
        actor: String,
        now: u64,
    ) -> Result<CharacterName, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let row: Option<(i64, String, u16, Vec<u8>)> = tx
                .query_row(
                    "SELECT account_id, name, version, sheet FROM characters
                     WHERE id = ?1 AND deleted_at IS NOT NULL",
                    params![id.get()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(db)?;
            let Some((account, name, version, bytes)) = row else {
                return Err(RepoError::NotFound);
            };
            let alive: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM characters WHERE account_id = ?1 AND deleted_at IS NULL",
                    params![account],
                    |r| r.get(0),
                )
                .map_err(db)?;
            if alive >= i64::from(limit) {
                return Err(RepoError::TooManyCharacters);
            }
            let name = CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))?;
            match tx.execute(
                "UPDATE characters SET deleted_at = NULL, name_key = ?1 WHERE id = ?2",
                params![name_key(&name), id.get()],
            ) {
                Ok(_) => {}
                Err(e) if is_unique_violation(&e) => return Err(RepoError::NameTaken),
                Err(e) => return Err(db(e)),
            }
            let sheet = decode(version, &bytes)?;
            for i in &sheet.items {
                let held: Option<i64> = tx
                    .query_row(
                        "SELECT item_id FROM item_owners WHERE item_id = ?1",
                        params![i.id as i64],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(db)?;
                if held.is_some() {
                    return Err(RepoError::Corrupt(format!("piece {} has another owner", i.id)));
                }
                tx.execute(
                    "INSERT INTO item_owners (item_id, owner_kind, character_id, item, updated_at)
                     VALUES (?1, 'character', ?2, ?3, ?4)",
                    params![i.id as i64, id.get(), i.item, now as i64],
                )
                .map_err(db)?;
            }
            let args = format!("id {} items {}", id.get(), sheet.items.len());
            insert_log(&tx, &actor, "restore_character", name.as_str(), &args, "", now)?;
            tx.commit().map_err(db)?;
            Ok(name)
        })
        .await
    }

    async fn run<T, F>(&self, f: F) -> Result<T, RepoError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, RepoError> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut c = conn.lock().unwrap_or_else(PoisonError::into_inner);
            f(&mut c)
        })
        .await
        .map_err(|_| RepoError::Task)?
    }

    pub async fn create_account(
        &self,
        name: AccountName,
        password_hash: Option<String>,
        now: u64,
    ) -> Result<AccountId, RepoError> {
        self.run(move |c| insert_account(c, &name, password_hash.as_deref(), now))
            .await
    }

    pub async fn account(&self, name: AccountName) -> Result<Option<AccountRow>, RepoError> {
        self.run(move |c| {
            let row = c
                .query_row(
                    "SELECT id, password_hash, locale, role, must_change_password, name
                     FROM accounts WHERE name = ?1",
                    params![name.as_str()],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(db)?;
            row.map(|(id, password_hash, locale, role, must_change, name)| {
                Ok(AccountRow {
                    id: AccountId::new(id_u32(id)?),
                    name,
                    password_hash,
                    locale: locale.parse().unwrap_or(Locale::SOURCE),
                    role: Role::parse(&role)
                        .ok_or_else(|| RepoError::Corrupt(format!("unknown role `{role}`")))?,
                    must_change_password: must_change != 0,
                })
            })
            .transpose()
        })
        .await
    }

    pub async fn account_by_id(&self, id: AccountId) -> Result<Option<AccountRow>, RepoError> {
        self.run(move |c| {
            let row = c
                .query_row(
                    "SELECT id, password_hash, locale, role, must_change_password, name
                     FROM accounts WHERE id = ?1",
                    params![id.get()],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(db)?;
            row.map(|(id, password_hash, locale, role, must_change, name)| {
                Ok(AccountRow {
                    id: AccountId::new(id_u32(id)?),
                    name,
                    password_hash,
                    locale: locale.parse().unwrap_or(Locale::SOURCE),
                    role: Role::parse(&role)
                        .ok_or_else(|| RepoError::Corrupt(format!("unknown role `{role}`")))?,
                    must_change_password: must_change != 0,
                })
            })
            .transpose()
        })
        .await
    }

    pub async fn set_locale(&self, account: AccountId, locale: Locale) -> Result<(), RepoError> {
        self.run(move |c| {
            c.execute(
                "UPDATE accounts SET locale = ?1 WHERE id = ?2",
                params![locale.code(), account.get()],
            )
            .map(|_| ())
            .map_err(db)
        })
        .await
    }

    pub async fn create_session(
        &self,
        account: AccountId,
        token: TokenHash,
        expires_at: u64,
    ) -> Result<(), RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            // Housekeeping: a session only ever needs to outlive its expiry.
            tx.execute(
                "DELETE FROM sessions WHERE expires_at < ?1",
                params![expires_at.saturating_sub(crate::SESSION_SECS) as i64],
            )
            .map_err(db)?;
            tx.execute(
                "INSERT INTO sessions (token_hash, account_id, expires_at) VALUES (?1, ?2, ?3)",
                params![&token.0[..], account.get(), expires_at as i64],
            )
            .map_err(db)?;
            tx.commit().map_err(db)
        })
        .await
    }

    pub async fn take_session(&self, token: TokenHash, now: u64) -> Result<Option<AccountId>, RepoError> {
        self.run(move |c| {
            let row = c
                .query_row(
                    "DELETE FROM sessions WHERE token_hash = ?1 RETURNING account_id, expires_at",
                    params![&token.0[..]],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()
                .map_err(db)?;
            match row {
                Some((account, expires)) if expires > now as i64 => {
                    Ok(Some(AccountId::new(id_u32(account)?)))
                }
                _ => Ok(None),
            }
        })
        .await
    }

    pub async fn characters(&self, account: AccountId) -> Result<Vec<CharacterRow>, RepoError> {
        self.run(move |c| list_characters(c, account)).await
    }

    pub async fn create_character(
        &self,
        account: AccountId,
        name: CharacterName,
        limit: u32,
        sheet: SavedSheet,
        now: u64,
    ) -> Result<CharacterId, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM characters WHERE account_id = ?1 AND deleted_at IS NULL",
                    params![account.get()],
                    |r| r.get(0),
                )
                .map_err(db)?;
            if count >= i64::from(limit) {
                return Err(RepoError::TooManyCharacters);
            }
            let id = insert_character(&tx, account, &name, &sheet, now)?;
            tx.commit().map_err(db)?;
            Ok(id)
        })
        .await
    }

    pub async fn load_character(&self, id: CharacterId) -> Result<Option<LoadedCharacter>, RepoError> {
        self.run(move |c| select_character(c, "id = ?1 AND deleted_at IS NULL", params![id.get()]))
            .await
    }

    pub async fn dev_character(
        &self,
        name: CharacterName,
        sheet: SavedSheet,
        now: u64,
    ) -> Result<LoadedCharacter, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let dev_name =
                AccountName::try_from(DEV_ACCOUNT).map_err(|e| RepoError::Corrupt(e.to_string()))?;
            let dev = tx
                .query_row(
                    "SELECT id FROM accounts WHERE name = ?1",
                    params![dev_name.as_str()],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(db)?;
            let dev = match dev {
                Some(id) => AccountId::new(id_u32(id)?),
                None => insert_account(&tx, &dev_name, None, now)?,
            };
            let existing = select_character(&tx, "name_key = ?1", params![name_key(&name)])?;
            let loaded = match existing {
                Some(ch) if ch.account == dev => ch,
                Some(_) => return Err(RepoError::NameTaken),
                None => {
                    let id = insert_character(&tx, dev, &name, &sheet, now)?;
                    LoadedCharacter {
                        id,
                        account: dev,
                        name,
                        place: None,
                        sheet,
                    }
                }
            };
            tx.commit().map_err(db)?;
            Ok(loaded)
        })
        .await
    }

    pub async fn save_zone_batch(
        &self,
        saves: Vec<CharacterSave>,
        now: u64,
    ) -> Result<Vec<(CharacterId, u64)>, RepoError> {
        if saves.is_empty() {
            return Ok(Vec::new());
        }
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let mut rejected = Vec::new();
            // Items leaving a character first, so one moving between two
            // characters of the same batch finds its old row gone.
            for s in &saves {
                let keep: Vec<i64> = s.sheet.items.iter().map(|i| i.id as i64).collect();
                let mut held = tx
                    .prepare_cached("SELECT item_id FROM item_owners WHERE character_id = ?1")
                    .map_err(db)?;
                let rows: Vec<i64> = held
                    .query_map(params![s.id.get()], |r| r.get(0))
                    .map_err(db)?
                    .collect::<Result<_, _>>()
                    .map_err(db)?;
                for id in rows.into_iter().filter(|id| !keep.contains(id)) {
                    tx.execute("DELETE FROM item_owners WHERE item_id = ?1", params![id])
                        .map_err(db)?;
                }
            }
            for s in &saves {
                // One savepoint per character: an item owned by someone
                // else refuses this save only (never two owners).
                tx.execute_batch("SAVEPOINT one").map_err(db)?;
                if let Some(id) = save_one(&tx, s, now)? {
                    tx.execute_batch("ROLLBACK TO one; RELEASE one").map_err(db)?;
                    rejected.push((s.id, id));
                    continue;
                }
                let kills: u64 = s
                    .sheet
                    .progress
                    .counters
                    .iter()
                    .filter(|(family, _, _)| family == "kills")
                    .map(|(_, _, n)| *n)
                    .sum();
                tx.execute(
                    "INSERT INTO telemetry (character_id, at, level, kills) VALUES (?1, ?2, ?3, ?4)",
                    params![s.id.get(), now as i64, s.sheet.progress.level, kills as i64],
                )
                .map_err(db)?;
                tx.execute_batch("RELEASE one").map_err(db)?;
            }
            tx.execute(
                "DELETE FROM telemetry WHERE at < ?1",
                params![now.saturating_sub(crate::TELEMETRY_KEEP_SECS) as i64],
            )
            .map_err(db)?;
            tx.commit().map_err(db)?;
            Ok(rejected)
        })
        .await
    }

    pub async fn commit_escrow(
        &self,
        save: CharacterSave,
        to: CharacterName,
        parcel: EscrowParcel,
        now: u64,
    ) -> Result<u64, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let recipient = tx
                .query_row(
                    "SELECT id FROM characters WHERE name_key = ?1 AND deleted_at IS NULL",
                    params![name_key(&to)],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(db)?
                .ok_or(RepoError::NotFound)?;
            let recipient = CharacterId::new(id_u32(recipient)?);
            if recipient == save.id {
                return Err(RepoError::NotFound);
            }
            let id = insert_escrow(&tx, Some(save.id), recipient, &parcel, &[save.id], now)?;
            if let Some(item) = save_one(&tx, &save, now)? {
                return Err(RepoError::ItemOwned(item));
            }
            tx.commit().map_err(db)?;
            Ok(id)
        })
        .await
    }

    pub async fn restore_escrow(
        &self,
        sender: Option<CharacterId>,
        recipient: CharacterId,
        parcel: EscrowParcel,
        now: u64,
    ) -> Result<u64, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let allowed: Vec<CharacterId> = sender.into_iter().chain([recipient]).collect();
            let id = insert_escrow(&tx, sender, recipient, &parcel, &allowed, now)?;
            tx.commit().map_err(db)?;
            Ok(id)
        })
        .await
    }

    pub async fn escrows_for(&self, recipient: CharacterId) -> Result<Vec<EscrowRow>, RepoError> {
        self.run(move |c| {
            let mut st = c
                .prepare_cached(
                    "SELECT id, sender, version, parcel, created_at FROM escrow WHERE recipient = ?1 ORDER BY id",
                )
                .map_err(db)?;
            let rows = st
                .query_map(params![recipient.get()], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, u16>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                })
                .map_err(db)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db)?;
            rows.into_iter()
                .map(|(id, sender, version, bytes, at)| {
                    Ok(EscrowRow {
                        id: id as u64,
                        sender: sender.map(id_u32).transpose()?.map(CharacterId::new),
                        recipient,
                        parcel: sheet_parcel(decode(version, &bytes)?),
                        created_at: at as u64,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn take_escrow(
        &self,
        id: u64,
        recipient: CharacterId,
    ) -> Result<Option<EscrowParcel>, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let row: Option<(u16, Vec<u8>)> = tx
                .query_row(
                    "SELECT version, parcel FROM escrow WHERE id = ?1 AND recipient = ?2",
                    params![id as i64, recipient.get()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?;
            let Some((version, bytes)) = row else {
                return Ok(None);
            };
            let parcel = sheet_parcel(decode(version, &bytes)?);
            tx.execute("DELETE FROM escrow WHERE id = ?1", params![id as i64])
                .map_err(db)?;
            // The pieces have no owner until the recipient's next save
            // claims them; nobody else can hold them meanwhile.
            tx.execute("DELETE FROM item_owners WHERE escrow_id = ?1", params![id as i64])
                .map_err(db)?;
            tx.commit().map_err(db)?;
            Ok(Some(parcel))
        })
        .await
    }

    /// Writes the coins created and destroyed since the last save, one row
    /// per source, and drops samples older than `TELEMETRY_KEEP_SECS`.
    pub async fn save_coin_flow(&self, rows: Vec<CoinFlowRow>) -> Result<(), RepoError> {
        if rows.is_empty() {
            return Ok(());
        }
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let newest = rows.iter().map(|r| r.at).max().unwrap_or(0);
            for r in &rows {
                tx.execute(
                    "INSERT INTO coin_flow (at, flow, created, amount) VALUES (?1, ?2, ?3, ?4)",
                    params![r.at as i64, r.flow, i64::from(r.created), r.amount as i64],
                )
                .map_err(db)?;
            }
            tx.execute(
                "DELETE FROM coin_flow WHERE at < ?1",
                params![newest.saturating_sub(crate::TELEMETRY_KEEP_SECS) as i64],
            )
            .map_err(db)?;
            tx.commit().map_err(db)?;
            Ok(())
        })
        .await
    }

    /// Coin flow samples since `since`, oldest first.
    pub async fn coin_flow(&self, since: u64) -> Result<Vec<CoinFlowRow>, RepoError> {
        self.run(move |c| {
            let mut st = c
                .prepare(
                    "SELECT at, flow, created, amount FROM coin_flow WHERE at >= ?1
                     ORDER BY at, flow",
                )
                .map_err(db)?;
            let rows = st
                .query_map(params![since as i64], |r| {
                    Ok(CoinFlowRow {
                        at: r.get::<_, i64>(0)? as u64,
                        flow: r.get(1)?,
                        created: r.get::<_, i64>(2)? != 0,
                        amount: r.get::<_, i64>(3)? as u64,
                    })
                })
                .map_err(db)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db)?;
            Ok(rows)
        })
        .await
    }

    /// Reserves `count` item ids: `[start, end)`, never handed out again.
    pub async fn reserve_item_ids(&self, count: u64) -> Result<(u64, u64), RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let start: i64 = tx
                .query_row("SELECT next FROM id_counters WHERE name = 'item'", [], |r| {
                    r.get(0)
                })
                .map_err(db)?;
            let end = start
                .checked_add(count as i64)
                .ok_or_else(|| RepoError::Corrupt("item id counter overflow".into()))?;
            tx.execute(
                "UPDATE id_counters SET next = ?1 WHERE name = 'item'",
                params![end],
            )
            .map_err(db)?;
            tx.commit().map_err(db)?;
            Ok((start as u64, end as u64))
        })
        .await
    }

    /// Owner of an item with identity.
    pub async fn item_owner(&self, item: u64) -> Result<Option<CharacterId>, RepoError> {
        self.run(move |c| {
            c.query_row(
                "SELECT character_id FROM item_owners WHERE item_id = ?1",
                params![item as i64],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(db)?
            .flatten()
            .map(|v| id_u32(v).map(CharacterId::new))
            .transpose()
        })
        .await
    }

    /// Every character with its sheet (admin reports).
    pub async fn telemetry(&self, since: u64) -> Result<Vec<crate::TelemetryRow>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare(
                    "SELECT character_id, at, level, kills FROM telemetry WHERE at >= ?1
                     ORDER BY character_id, at",
                )
                .map_err(db)?;
            let rows: Vec<(i64, i64, i64, i64)> = stmt
                .query_map(params![since as i64], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map_err(db)?
                .collect::<Result<_, _>>()
                .map_err(db)?;
            rows.into_iter()
                .map(|(id, at, level, kills)| {
                    Ok(crate::TelemetryRow {
                        character: CharacterId::new(id_u32(id)?),
                        at: at as u64,
                        level: level as u8,
                        kills: kills as u64,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn all_sheets(&self) -> Result<Vec<(CharacterId, CharacterName, SavedSheet)>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, name, version, sheet FROM characters WHERE deleted_at IS NULL ORDER BY id",
                )
                .map_err(db)?;
            let rows: Vec<(i64, String, i64, Vec<u8>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .map_err(db)?
                .collect::<Result<_, _>>()
                .map_err(db)?;
            rows.into_iter()
                .map(|(id, name, version, bytes)| {
                    let name = CharacterName::try_from(name.as_str())
                        .map_err(|_| RepoError::Corrupt(format!("character {id}: bad name")))?;
                    let sheet = decode(version as u16, &bytes)?;
                    Ok((CharacterId::new(id_u32(id)?), name, sheet))
                })
                .collect()
        })
        .await
    }

    pub async fn set_password(
        &self,
        account: AccountId,
        password_hash: Option<String>,
        must_change: bool,
    ) -> Result<(), RepoError> {
        self.run(move |c| {
            c.execute(
                "UPDATE accounts SET password_hash = ?1, must_change_password = ?2 WHERE id = ?3",
                params![password_hash, if must_change { 1i64 } else { 0 }, account.get()],
            )
            .map(|_| ())
            .map_err(db)
        })
        .await
    }

    pub async fn delete_sessions(&self, account: AccountId) -> Result<(), RepoError> {
        self.run(move |c| {
            c.execute(
                "DELETE FROM sessions WHERE account_id = ?1",
                params![account.get()],
            )
            .map(|_| ())
            .map_err(db)
        })
        .await
    }

    pub async fn create_device(
        &self,
        account: AccountId,
        token: TokenHash,
        now: u64,
        expires_at: u64,
    ) -> Result<(), RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute("DELETE FROM devices WHERE expires_at <= ?1", params![now as i64])
                .map_err(db)?;
            tx.execute(
                "INSERT INTO devices (token_hash, account_id, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![&token.0[..], account.get(), now as i64, expires_at as i64],
            )
            .map_err(db)?;
            // Only the newest devices of the account stay.
            tx.execute(
                "DELETE FROM devices WHERE account_id = ?1 AND token_hash NOT IN (
                     SELECT token_hash FROM devices WHERE account_id = ?1
                     ORDER BY created_at DESC, rowid DESC LIMIT ?2)",
                params![account.get(), crate::DEVICES_PER_ACCOUNT as i64],
            )
            .map_err(db)?;
            tx.commit().map_err(db)
        })
        .await
    }

    pub async fn device_ok(&self, account: AccountId, token: TokenHash, now: u64) -> Result<bool, RepoError> {
        self.run(move |c| {
            c.query_row(
                "SELECT 1 FROM devices WHERE token_hash = ?1 AND account_id = ?2 AND expires_at > ?3",
                params![&token.0[..], account.get(), now as i64],
                |_| Ok(()),
            )
            .optional()
            .map(|r| r.is_some())
            .map_err(db)
        })
        .await
    }

    pub async fn delete_device(&self, token: TokenHash) -> Result<(), RepoError> {
        self.run(move |c| {
            c.execute("DELETE FROM devices WHERE token_hash = ?1", params![&token.0[..]])
                .map(|_| ())
                .map_err(db)
        })
        .await
    }

    pub async fn delete_devices(&self, account: AccountId) -> Result<(), RepoError> {
        self.run(move |c| {
            c.execute(
                "DELETE FROM devices WHERE account_id = ?1",
                params![account.get()],
            )
            .map(|_| ())
            .map_err(db)
        })
        .await
    }

    pub async fn add_failed_login(&self, account: AccountId) -> Result<(), RepoError> {
        self.run(move |c| {
            c.execute(
                "UPDATE accounts SET failed_logins = failed_logins + 1 WHERE id = ?1",
                params![account.get()],
            )
            .map(|_| ())
            .map_err(db)
        })
        .await
    }

    pub async fn take_failed_logins(&self, account: AccountId) -> Result<u32, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let n = tx
                .query_row(
                    "SELECT failed_logins FROM accounts WHERE id = ?1",
                    params![account.get()],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(db)?
                .unwrap_or(0);
            if n != 0 {
                tx.execute(
                    "UPDATE accounts SET failed_logins = 0 WHERE id = ?1",
                    params![account.get()],
                )
                .map_err(db)?;
            }
            tx.commit().map_err(db)?;
            Ok(u32::try_from(n).unwrap_or(u32::MAX))
        })
        .await
    }

    pub async fn failed_logins(&self, min: u32) -> Result<Vec<FailedLoginRow>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT name, failed_logins FROM accounts WHERE failed_logins >= ?1
                     ORDER BY failed_logins DESC, name",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![i64::from(min.max(1))], |r| {
                    Ok(FailedLoginRow {
                        name: r.get(0)?,
                        failed_logins: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(u32::MAX),
                    })
                })
                .map_err(db)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(db)
        })
        .await
    }

    pub async fn login_blocks(&self) -> Result<Vec<LoginBlock>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached("SELECT key, count, last_at, blocked_until FROM login_blocks ORDER BY key")
                .map_err(db)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(LoginBlock {
                        key: r.get(0)?,
                        count: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(u32::MAX),
                        last_at: r.get::<_, i64>(2)? as u64,
                        blocked_until: r.get::<_, i64>(3)? as u64,
                    })
                })
                .map_err(db)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(db)
        })
        .await
    }

    /// Replaces every saved wait with `blocks`.
    pub async fn save_login_blocks(&self, blocks: Vec<LoginBlock>) -> Result<(), RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute("DELETE FROM login_blocks", []).map_err(db)?;
            {
                let mut stmt = tx
                    .prepare_cached(
                        "INSERT INTO login_blocks (key, count, last_at, blocked_until) VALUES (?1, ?2, ?3, ?4)",
                    )
                    .map_err(db)?;
                for b in &blocks {
                    stmt.execute(params![
                        b.key,
                        i64::from(b.count),
                        b.last_at as i64,
                        b.blocked_until as i64
                    ])
                    .map_err(db)?;
                }
            }
            tx.commit().map_err(db)
        })
        .await
    }

    /// `true` when `character` is the first to reach `key` (a new row).
    pub async fn record_first(&self, key: &str, character: CharacterId, now: u64) -> Result<bool, RepoError> {
        let key = key.to_owned();
        self.run(move |c| {
            let n = c
                .execute(
                    "INSERT OR IGNORE INTO server_firsts (key, character_id, at) VALUES (?1, ?2, ?3)",
                    params![key, character.get(), now as i64],
                )
                .map_err(db)?;
            Ok(n == 1)
        })
        .await
    }

    /// Every server first: key, character name, unix second, oldest first.
    pub async fn server_firsts(&self) -> Result<Vec<(String, CharacterName, u64)>, RepoError> {
        self.run(move |c| {
            let mut st = c
                .prepare(
                    "SELECT f.key, ch.name, f.at FROM server_firsts f
                     JOIN characters ch ON ch.id = f.character_id ORDER BY f.at, f.key",
                )
                .map_err(db)?;
            let rows = st
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                let (key, name, at) = row.map_err(db)?;
                let name = CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))?;
                out.push((key, name, u64::try_from(at).unwrap_or(0)));
            }
            Ok(out)
        })
        .await
    }

    /// Takes `key` for `character` if nobody holds it; returns the holder
    /// (`character` itself when it took it or already held it).
    pub async fn claim_unique(
        &self,
        key: &str,
        character: CharacterId,
        now: u64,
    ) -> Result<CharacterId, RepoError> {
        let key = key.to_owned();
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute(
                "INSERT OR IGNORE INTO unique_holders (key, character_id, since) VALUES (?1, ?2, ?3)",
                params![key, character.get(), now as i64],
            )
            .map_err(db)?;
            let holder: i64 = tx
                .query_row(
                    "SELECT character_id FROM unique_holders WHERE key = ?1",
                    params![key],
                    |r| r.get(0),
                )
                .map_err(db)?;
            tx.commit().map_err(db)?;
            Ok(CharacterId::new(id_u32(holder)?))
        })
        .await
    }

    /// Every unique holder: key, character, name and the unix second of the
    /// character's last save (deleted characters count as never seen).
    pub async fn unique_holders(&self) -> Result<Vec<UniqueHolder>, RepoError> {
        self.run(move |c| {
            let mut st = c
                .prepare(
                    "SELECT u.key, u.character_id, ch.name, CASE WHEN ch.deleted_at IS NULL THEN ch.saved_at ELSE 0 END
                     FROM unique_holders u JOIN characters ch ON ch.id = u.character_id ORDER BY u.key",
                )
                .map_err(db)?;
            let rows = st
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                let (key, id, name, seen) = row.map_err(db)?;
                out.push(UniqueHolder {
                    key,
                    character: CharacterId::new(id_u32(id)?),
                    name: CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))?,
                    last_seen: u64::try_from(seen).unwrap_or(0),
                });
            }
            Ok(out)
        })
        .await
    }

    /// Frees `key` whoever holds it (a unique item that left the game).
    pub async fn release_key(&self, key: &str) -> Result<(), RepoError> {
        let key = key.to_owned();
        self.run(move |c| {
            c.execute("DELETE FROM unique_holders WHERE key = ?1", params![key])
                .map_err(db)?;
            Ok(())
        })
        .await
    }

    /// Frees `key` if `character` still holds it; `true` when it did.
    pub async fn release_unique(&self, key: &str, character: CharacterId) -> Result<bool, RepoError> {
        let key = key.to_owned();
        self.run(move |c| {
            let n = c
                .execute(
                    "DELETE FROM unique_holders WHERE key = ?1 AND character_id = ?2",
                    params![key, character.get()],
                )
                .map_err(db)?;
            Ok(n == 1)
        })
        .await
    }

    /// Records a dungeon clear: the party's names and its time.
    pub async fn add_clear(&self, dungeon: &str, party: &str, secs: u64, now: u64) -> Result<(), RepoError> {
        let (dungeon, party) = (dungeon.to_owned(), party.to_owned());
        self.run(move |c| {
            c.execute(
                "INSERT INTO dungeon_clears (dungeon, party, secs, at) VALUES (?1, ?2, ?3, ?4)",
                params![dungeon, party, secs as i64, now as i64],
            )
            .map_err(db)?;
            Ok(())
        })
        .await
    }

    /// The `limit` fastest clears of a dungeon: party and seconds, the
    /// oldest first on a tie.
    pub async fn fastest_clears(&self, dungeon: &str, limit: u32) -> Result<Vec<(String, u64)>, RepoError> {
        let dungeon = dungeon.to_owned();
        self.run(move |c| {
            let mut st = c
                .prepare(
                    "SELECT party, secs FROM dungeon_clears WHERE dungeon = ?1 ORDER BY secs, at LIMIT ?2",
                )
                .map_err(db)?;
            let rows = st
                .query_map(params![dungeon, limit], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                let (party, secs) = row.map_err(db)?;
                out.push((party, u64::try_from(secs).unwrap_or(0)));
            }
            Ok(out)
        })
        .await
    }

    /// `character` may not enter `dungeon` before `until`.
    pub async fn set_lockout(
        &self,
        character: CharacterId,
        dungeon: &str,
        until: u64,
    ) -> Result<(), RepoError> {
        let dungeon = dungeon.to_owned();
        self.run(move |c| {
            c.execute(
                "INSERT INTO lockouts (character_id, dungeon, until) VALUES (?1, ?2, ?3)
                 ON CONFLICT (character_id, dungeon) DO UPDATE SET until = excluded.until",
                params![character.get(), dungeon, until as i64],
            )
            .map_err(db)?;
            Ok(())
        })
        .await
    }

    /// Lockouts still running at `now` (older rows are deleted).
    pub async fn lockouts(&self, now: u64) -> Result<Vec<(CharacterId, String, u64)>, RepoError> {
        self.run(move |c| {
            c.execute("DELETE FROM lockouts WHERE until <= ?1", params![now as i64])
                .map_err(db)?;
            let mut st = c
                .prepare("SELECT character_id, dungeon, until FROM lockouts ORDER BY character_id, dungeon")
                .map_err(db)?;
            let rows = st
                .query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                let (id, dungeon, until) = row.map_err(db)?;
                out.push((
                    CharacterId::new(id_u32(id)?),
                    dungeon,
                    u64::try_from(until).unwrap_or(0),
                ));
            }
            Ok(out)
        })
        .await
    }

    pub async fn record_login(&self, account: AccountId, ip: &str, now: u64) -> Result<(), RepoError> {
        let ip = ip.to_owned();
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute(
                "INSERT INTO logins (account_id, ip, created_at) VALUES (?1, ?2, ?3)",
                params![account.get(), ip, now as i64],
            )
            .map_err(db)?;
            // Login rows are kept for 30 days.
            tx.execute(
                "DELETE FROM logins WHERE created_at < ?1",
                params![now.saturating_sub(LOGIN_RETENTION_SECS) as i64],
            )
            .map_err(db)?;
            tx.commit().map_err(db)
        })
        .await
    }

    pub async fn account_sanction(
        &self,
        account: AccountId,
        now: u64,
    ) -> Result<Option<Sanction>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT id, kind, account_id, ip, reason, note, created_at, expires_at,
                            revoked_at, revoked_by, created_by
                     FROM sanctions
                     WHERE account_id = ?1 AND revoked_at IS NULL
                       AND (expires_at IS NULL OR expires_at > ?2)",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![account.get(), now as i64], sanction_row)
                .map_err(db)?;
            let mut best: Option<Sanction> = None;
            for row in rows {
                let s = row.map_err(db)?;
                // Strongest kind; between two of the same kind, the one that
                // ends last. `None` (permanent) ends last.
                let ends = |x: &Sanction| x.expires_at.unwrap_or(u64::MAX);
                let better = best.as_ref().is_none_or(|b| {
                    (strength(s.kind), std::cmp::Reverse(ends(&s)))
                        < (strength(b.kind), std::cmp::Reverse(ends(b)))
                });
                if better {
                    best = Some(s);
                }
            }
            Ok(best)
        })
        .await
    }

    pub async fn ip_sanction(&self, ip: &str, now: u64) -> Result<Option<Sanction>, RepoError> {
        let ip = ip.to_owned();
        self.run(move |c| {
            c.query_row(
                "SELECT id, kind, account_id, ip, reason, note, created_at, expires_at,
                        revoked_at, revoked_by, created_by
                 FROM sanctions
                 WHERE ip = ?1 AND kind = 'ip_ban' AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > ?2)
                 ORDER BY id DESC LIMIT 1",
                params![ip, now as i64],
                sanction_row,
            )
            .optional()
            .map_err(db)
        })
        .await
    }

    pub async fn sanctions_changed_since(&self, since: u64) -> Result<Vec<Sanction>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT id, kind, account_id, ip, reason, note, created_at, expires_at,
                            revoked_at, revoked_by, created_by
                     FROM sanctions
                     WHERE created_at >= ?1 OR revoked_at >= ?1
                     ORDER BY id",
                )
                .map_err(db)?;
            let rows = stmt.query_map(params![since as i64], sanction_row).map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn sanctions_of(&self, account: AccountId) -> Result<Vec<Sanction>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT id, kind, account_id, ip, reason, note, created_at, expires_at,
                            revoked_at, revoked_by, created_by
                     FROM sanctions WHERE account_id = ?1 ORDER BY id DESC",
                )
                .map_err(db)?;
            let rows = stmt.query_map(params![account.get()], sanction_row).map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn add_sanction(&self, new: NewSanction, now: u64) -> Result<SanctionId, RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute(
                "INSERT INTO sanctions
                    (kind, account_id, ip, reason, note, created_at, expires_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    new.kind.as_str(),
                    new.account.map(|a| a.get()),
                    new.ip,
                    new.reason,
                    new.note,
                    now as i64,
                    new.expires_at.map(|e| e as i64),
                    new.created_by,
                ],
            )
            .map_err(db)?;
            let id = SanctionId(id_u64(tx.last_insert_rowid())?);
            // Suspension and ban end every session.
            if let (Some(account), true) = (new.account, new.kind.blocks_login()) {
                tx.execute(
                    "DELETE FROM sessions WHERE account_id = ?1",
                    params![account.get()],
                )
                .map_err(db)?;
            }
            insert_log(
                &tx,
                &new.created_by,
                new.kind.as_str(),
                &new.target,
                &new.reason,
                &new.note,
                now,
            )?;
            tx.commit().map_err(db)?;
            Ok(id)
        })
        .await
    }

    pub async fn revoke_sanction(&self, id: SanctionId, actor: &str, now: u64) -> Result<bool, RepoError> {
        let actor = actor.to_owned();
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let changed = tx
                .execute(
                    "UPDATE sanctions SET revoked_at = ?1, revoked_by = ?2
                     WHERE id = ?3 AND revoked_at IS NULL",
                    params![now as i64, actor, id.0 as i64],
                )
                .map_err(db)?;
            if changed == 0 {
                return Ok(false);
            }
            insert_log(&tx, &actor, "revoke", &id.to_string(), "", "", now)?;
            tx.commit().map_err(db)?;
            Ok(true)
        })
        .await
    }

    pub async fn set_role(
        &self,
        account: AccountId,
        role: Role,
        actor: &str,
        target: &str,
        now: u64,
    ) -> Result<(), RepoError> {
        let (actor, target) = (actor.to_owned(), target.to_owned());
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute(
                "UPDATE accounts SET role = ?1 WHERE id = ?2",
                params![role.as_str(), account.get()],
            )
            .map_err(db)?;
            let action = if role == Role::Admin { "promote" } else { "demote" };
            insert_log(&tx, &actor, action, &target, "", "", now)?;
            tx.commit().map_err(db)
        })
        .await
    }

    pub async fn reset_password(
        &self,
        account: AccountId,
        password_hash: String,
        actor: &str,
        target: &str,
        now: u64,
    ) -> Result<(), RepoError> {
        let (actor, target) = (actor.to_owned(), target.to_owned());
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            tx.execute(
                "UPDATE accounts SET password_hash = ?1, must_change_password = 1 WHERE id = ?2",
                params![password_hash, account.get()],
            )
            .map_err(db)?;
            tx.execute(
                "DELETE FROM sessions WHERE account_id = ?1",
                params![account.get()],
            )
            .map_err(db)?;
            tx.execute(
                "DELETE FROM devices WHERE account_id = ?1",
                params![account.get()],
            )
            .map_err(db)?;
            insert_log(&tx, &actor, "reset_password", &target, "", "", now)?;
            tx.commit().map_err(db)
        })
        .await
    }

    pub async fn rename_character(
        &self,
        old: &CharacterName,
        new: &CharacterName,
        actor: &str,
        now: u64,
    ) -> Result<CharacterId, RepoError> {
        let (old, new, actor) = (old.clone(), new.clone(), actor.to_owned());
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let found: Option<(i64, String)> = tx
                .query_row(
                    "SELECT id, name FROM characters WHERE name_key = ?1",
                    params![name_key(&old)],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?;
            let Some((id, old_name)) = found else {
                return Err(RepoError::NotFound);
            };
            match tx.execute(
                "UPDATE characters SET name = ?1, name_key = ?2 WHERE id = ?3",
                params![new.as_str(), name_key(&new), id],
            ) {
                Ok(_) => {}
                Err(e) if is_unique_violation(&e) => return Err(RepoError::NameTaken),
                Err(e) => return Err(db(e)),
            }
            // Target is the id (the server matches online characters by it);
            // both names stay in the audit.
            let names = format!("{old_name} -> {new}");
            insert_log(&tx, &actor, "rename", &id.to_string(), &names, "", now)?;
            tx.commit().map_err(db)?;
            Ok(CharacterId::new(id_u32(id)?))
        })
        .await
    }

    pub async fn recent_logins(&self, account: AccountId, limit: u32) -> Result<Vec<LoginRow>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT ip, created_at FROM logins WHERE account_id = ?1
                     ORDER BY created_at DESC, id DESC LIMIT ?2",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![account.get(), i64::from(limit)], |r| {
                    Ok(LoginRow {
                        ip: r.get(0)?,
                        created_at: r.get::<_, i64>(1)? as u64,
                    })
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn admin_log(
        &self,
        account: Option<&str>,
        since: Option<u64>,
        limit: u32,
    ) -> Result<Vec<AdminLogRow>, RepoError> {
        let account = account.map(str::to_owned);
        self.run(move |c| {
            let mut sql =
                String::from("SELECT id, actor, action, target, args, note, created_at FROM admin_log");
            let mut conds: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();
            if let Some(a) = account {
                values.push(a.into());
                conds.push(format!("target = ?{}", values.len()));
            }
            if let Some(s) = since {
                values.push((s as i64).into());
                conds.push(format!("created_at >= ?{}", values.len()));
            }
            if !conds.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conds.join(" AND "));
            }
            sql.push_str(" ORDER BY id DESC LIMIT ?");
            values.push(i64::from(limit).into());
            sql.push_str(&values.len().to_string());
            let mut stmt = c.prepare_cached(&sql).map_err(db)?;
            let rows = stmt
                .query_map(params_from_iter(values.iter()), log_row)
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn admin_log_since(&self, since: u64) -> Result<Vec<AdminLogRow>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT id, actor, action, target, args, note, created_at
                     FROM admin_log WHERE created_at >= ?1 ORDER BY id",
                )
                .map_err(db)?;
            let rows = stmt.query_map(params![since as i64], log_row).map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db)?);
            }
            Ok(out)
        })
        .await
    }

    /// Id, account and account name of a character, by name (any case).
    pub async fn character_by_name(
        &self,
        name: CharacterName,
    ) -> Result<Option<(CharacterId, AccountId, String)>, RepoError> {
        self.run(move |c| {
            let row = c
                .query_row(
                    "SELECT c.id, c.account_id, a.name FROM characters c
                     JOIN accounts a ON a.id = c.account_id WHERE c.name_key = ?1",
                    params![name_key(&name)],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
                )
                .optional()
                .map_err(db)?;
            row.map(|(id, account, name)| {
                Ok((
                    CharacterId::new(id_u32(id)?),
                    AccountId::new(id_u32(account)?),
                    name,
                ))
            })
            .transpose()
        })
        .await
    }

    /// End of the longest active mute (`u64::MAX`: no end), if any.
    pub async fn mute_until(&self, account: AccountId, now: u64) -> Result<Option<u64>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT expires_at FROM sanctions
                     WHERE account_id = ?1 AND kind = 'mute' AND revoked_at IS NULL
                       AND (expires_at IS NULL OR expires_at > ?2)",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![account.get(), now as i64], |r| r.get::<_, Option<i64>>(0))
                .map_err(db)?;
            let mut end: Option<u64> = None;
            for row in rows {
                let e = row.map_err(db)?.map_or(u64::MAX, |e| e as u64);
                end = Some(end.map_or(e, |x| x.max(e)));
            }
            Ok(end)
        })
        .await
    }

    /// An admin action that is not a sanction (`kick`, `replay`, `who`).
    pub async fn log_admin(&self, row: AdminLogRow) -> Result<(), RepoError> {
        self.run(move |c| {
            insert_log(
                c,
                &row.actor,
                &row.action,
                &row.target,
                &row.args,
                &row.note,
                row.created_at,
            )
        })
        .await
    }

    /// Stores a report and deletes the ones older than `CHAT_REPORT_KEEP_SECS`.
    pub async fn add_chat_report(&self, report: ChatReport) -> Result<(), RepoError> {
        self.run(move |c| {
            let tx = c.transaction().map_err(db)?;
            let cutoff = report.created_at.saturating_sub(CHAT_REPORT_KEEP_SECS);
            tx.execute(
                "DELETE FROM chat_reports WHERE created_at < ?1",
                params![cutoff as i64],
            )
            .map_err(db)?;
            let lines: Vec<String> = report
                .lines
                .iter()
                .map(|l| format!("{}\t{}\t{}\t{}", l.at, l.channel, l.from, l.text))
                .collect();
            tx.execute(
                "INSERT INTO chat_reports (reporter, target, lines, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    report.reporter,
                    report.target,
                    lines.join("\n"),
                    report.created_at as i64
                ],
            )
            .map_err(db)?;
            tx.commit().map_err(db)
        })
        .await
    }

    /// Reports at or after `since`, newest first.
    pub async fn chat_reports(&self, since: u64) -> Result<Vec<ChatReport>, RepoError> {
        self.run(move |c| {
            let mut stmt = c
                .prepare_cached(
                    "SELECT id, reporter, target, lines, created_at FROM chat_reports
                     WHERE created_at >= ?1 ORDER BY id DESC",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map(params![since as i64], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                })
                .map_err(db)?;
            let mut out = Vec::new();
            for row in rows {
                let (id, reporter, target, lines, created_at) = row.map_err(db)?;
                let lines = lines
                    .lines()
                    .filter_map(|l| {
                        let mut it = l.splitn(4, '\t');
                        Some(ChatLine {
                            at: it.next()?.parse().ok()?,
                            channel: it.next()?.to_owned(),
                            from: it.next()?.to_owned(),
                            text: it.next()?.to_owned(),
                        })
                    })
                    .collect();
                out.push(ChatReport {
                    id: id as u64,
                    reporter,
                    target,
                    lines,
                    created_at: created_at as u64,
                });
            }
            Ok(out)
        })
        .await
    }

    /// Read-only check of a backup copy: `PRAGMA integrity_check`, a schema
    /// this build knows, and the number of accounts and characters.
    pub fn verify_backup(path: &str) -> Result<(u64, u64), RepoError> {
        let conn =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(db)?;
        let check: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(db)?;
        if check != "ok" {
            return Err(RepoError::Corrupt(check));
        }
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db)?;
        if version < 1 || version > MIGRATIONS.len() as i64 {
            return Err(RepoError::Corrupt(format!(
                "schema version {version}; this build knows 1 to {}",
                MIGRATIONS.len()
            )));
        }
        let count = |table: &str| -> Result<u64, RepoError> {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(db)
            .and_then(id_u64)
        };
        Ok((count("accounts")?, count("characters")?))
    }
}

/// `logins` retention.
const LOGIN_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

fn id_u64(v: i64) -> Result<u64, RepoError> {
    u64::try_from(v).map_err(|_| RepoError::Corrupt(format!("id {v} out of range")))
}

/// Login-blocking strength: a ban wins over a suspension, which wins over a
/// mute (only chat, phase 5).
fn strength(kind: SanctionKind) -> u8 {
    match kind {
        SanctionKind::Ban => 0,
        SanctionKind::Suspend => 1,
        SanctionKind::IpBan => 2,
        SanctionKind::Mute => 3,
    }
}

fn sanction_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Sanction> {
    let corrupt = |col: usize, what: String| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, what.into())
    };
    let kind: String = r.get(1)?;
    // An unknown kind is corruption, never a weaker sanction: a ban read
    // as a mute would let a banned account in.
    let kind =
        SanctionKind::parse(&kind).ok_or_else(|| corrupt(1, format!("unknown sanction kind `{kind}`")))?;
    let account = match r.get::<_, Option<i64>>(2)? {
        Some(id) => {
            Some(AccountId::new(u32::try_from(id).map_err(|_| {
                corrupt(2, format!("account id {id} out of range"))
            })?))
        }
        None => None,
    };
    Ok(Sanction {
        id: SanctionId(r.get::<_, i64>(0)? as u64),
        kind,
        account,
        ip: r.get(3)?,
        reason: r.get(4)?,
        note: r.get(5)?,
        created_at: r.get::<_, i64>(6)? as u64,
        expires_at: r.get::<_, Option<i64>>(7)?.map(|e| e as u64),
        revoked_at: r.get::<_, Option<i64>>(8)?.map(|e| e as u64),
        revoked_by: r.get(9)?,
        created_by: r.get(10)?,
    })
}

fn log_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AdminLogRow> {
    Ok(AdminLogRow {
        id: r.get::<_, i64>(0)? as u64,
        actor: r.get(1)?,
        action: r.get(2)?,
        target: r.get(3)?,
        args: r.get(4)?,
        note: r.get(5)?,
        created_at: r.get::<_, i64>(6)? as u64,
    })
}

fn insert_log(
    c: &Connection,
    actor: &str,
    action: &str,
    target: &str,
    args: &str,
    note: &str,
    now: u64,
) -> Result<(), RepoError> {
    c.execute(
        "INSERT INTO admin_log (actor, action, target, args, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![actor, action, target, args, note, now as i64],
    )
    .map(|_| ())
    .map_err(db)
}

fn migrate(conn: &mut Connection) -> Result<(), RepoError> {
    let current: usize = conn
        .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
        .map_err(db)?
        .try_into()
        .map_err(|_| RepoError::Corrupt("negative schema version".into()))?;
    if current > MIGRATIONS.len() {
        return Err(RepoError::Corrupt(format!(
            "database schema {current} is newer than this server ({})",
            MIGRATIONS.len()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction().map_err(db)?;
        tx.execute_batch(sql).map_err(db)?;
        tx.pragma_update(None, "user_version", i as i64 + 1).map_err(db)?;
        tx.commit().map_err(db)?;
    }
    Ok(())
}

fn insert_account(
    c: &Connection,
    name: &AccountName,
    password_hash: Option<&str>,
    now: u64,
) -> Result<AccountId, RepoError> {
    match c.execute(
        "INSERT INTO accounts (name, password_hash, created_at) VALUES (?1, ?2, ?3)",
        params![name.as_str(), password_hash, now as i64],
    ) {
        Ok(_) => Ok(AccountId::new(id_u32(c.last_insert_rowid())?)),
        Err(e) if is_unique_violation(&e) => Err(RepoError::AccountTaken),
        Err(e) => Err(db(e)),
    }
}

fn insert_character(
    c: &Connection,
    account: AccountId,
    name: &CharacterName,
    sheet: &SavedSheet,
    now: u64,
) -> Result<CharacterId, RepoError> {
    let (version, bytes) = encode(sheet)?;
    match c.execute(
        "INSERT INTO characters (account_id, name, name_key, version, sheet, created_at, saved_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            account.get(),
            name.as_str(),
            name_key(name),
            version,
            bytes,
            now as i64
        ],
    ) {
        Ok(_) => Ok(CharacterId::new(id_u32(c.last_insert_rowid())?)),
        Err(e) if is_unique_violation(&e) => Err(RepoError::NameTaken),
        Err(e) => Err(db(e)),
    }
}

fn list_characters(c: &Connection, account: AccountId) -> Result<Vec<CharacterRow>, RepoError> {
    let mut stmt = c
        .prepare_cached(
            "SELECT id, name, version, sheet FROM characters WHERE account_id = ?1 AND deleted_at IS NULL ORDER BY id",
        )
        .map_err(db)?;
    let rows = stmt
        .query_map(params![account.get()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u16>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(db)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, version, sheet) = row.map_err(db)?;
        let sheet = decode(version, &sheet)?;
        out.push(CharacterRow {
            id: CharacterId::new(id_u32(id)?),
            name: CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))?,
            level: sheet.progress.level.max(1),
            look: sheet.look,
        });
    }
    Ok(out)
}

/// Writes one sheet and claims its pieces in `item_owners`. `Some(id)`: a
/// piece another owner (character or custody) holds; the caller rolls back.
fn save_one(tx: &rusqlite::Transaction<'_>, s: &CharacterSave, now: u64) -> Result<Option<u64>, RepoError> {
    for i in &s.sheet.items {
        let owner: Option<Option<i64>> = tx
            .query_row(
                "SELECT character_id FROM item_owners WHERE item_id = ?1",
                params![i.id as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        match owner {
            Some(Some(o)) if o == i64::from(s.id.get()) => {}
            Some(_) => return Ok(Some(i.id)),
            None => {
                tx.execute(
                    "INSERT INTO item_owners (item_id, owner_kind, character_id, item, updated_at)
                     VALUES (?1, 'character', ?2, ?3, ?4)",
                    params![i.id as i64, s.id.get(), i.item, now as i64],
                )
                .map_err(db)?;
            }
        }
    }
    let (version, bytes) = encode(&s.sheet)?;
    tx.execute(
        "UPDATE characters SET zone = ?1, x = ?2, y = ?3, version = ?4, sheet = ?5, saved_at = ?6
         WHERE id = ?7",
        params![s.zone, s.pos.x, s.pos.y, version, bytes, now as i64, s.id.get()],
    )
    .map_err(db)?;
    Ok(None)
}

/// A parcel into custody for `recipient`: one `escrow` row and its pieces
/// as `item_owners` rows of kind `escrow`. A piece held by anyone but
/// `allowed` (or by no one) is refused: `ItemOwned`.
fn insert_escrow(
    tx: &rusqlite::Transaction<'_>,
    sender: Option<CharacterId>,
    recipient: CharacterId,
    parcel: &EscrowParcel,
    allowed: &[CharacterId],
    now: u64,
) -> Result<u64, RepoError> {
    let (version, bytes) = encode(&parcel_sheet(parcel))?;
    tx.execute(
        "INSERT INTO escrow (sender, recipient, version, parcel, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            sender.map(CharacterId::get),
            recipient.get(),
            version,
            bytes,
            now as i64
        ],
    )
    .map_err(db)?;
    let id = tx.last_insert_rowid();
    for i in &parcel.items {
        let owner: Option<(String, Option<i64>)> = tx
            .query_row(
                "SELECT owner_kind, character_id FROM item_owners WHERE item_id = ?1",
                params![i.id as i64],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db)?;
        match owner {
            None => {}
            Some((kind, Some(c)))
                if kind == "character" && allowed.iter().any(|a| i64::from(a.get()) == c) => {}
            Some(_) => return Err(RepoError::ItemOwned(i.id)),
        }
        tx.execute(
            "INSERT OR REPLACE INTO item_owners (item_id, owner_kind, character_id, item, updated_at, escrow_id)
             VALUES (?1, 'escrow', NULL, ?2, ?3, ?4)",
            params![i.id as i64, i.item, now as i64, id],
        )
        .map_err(db)?;
    }
    u64::try_from(id).map_err(|_| RepoError::Corrupt(format!("escrow id {id}")))
}

/// A parcel is stored as a sheet with only coins, backpack and items, so
/// it migrates with the sheet's versions.
fn parcel_sheet(p: &EscrowParcel) -> SavedSheet {
    SavedSheet {
        coins: p.coins,
        backpack: p.stacks.iter().cloned().map(Some).collect(),
        items: p.items.clone(),
        ..SavedSheet::default()
    }
}

fn sheet_parcel(s: SavedSheet) -> EscrowParcel {
    EscrowParcel {
        coins: s.coins,
        stacks: s.backpack.into_iter().flatten().collect(),
        items: s.items,
    }
}

fn select_character(
    c: &Connection,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<Option<LoadedCharacter>, RepoError> {
    type Row = (
        i64,
        i64,
        String,
        Option<String>,
        Option<i32>,
        Option<i32>,
        u16,
        Vec<u8>,
    );
    let sql =
        format!("SELECT id, account_id, name, zone, x, y, version, sheet FROM characters WHERE {filter}");
    let row: Option<Row> = c
        .query_row(&sql, args, |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .optional()
        .map_err(db)?;
    let Some((id, account, name, zone, x, y, version, sheet)) = row else {
        return Ok(None);
    };
    let place = match (zone, x, y) {
        (Some(zone), Some(x), Some(y)) => Some((zone, TilePos::new(x, y))),
        _ => None,
    };
    Ok(Some(LoadedCharacter {
        id: CharacterId::new(id_u32(id)?),
        account: AccountId::new(id_u32(account)?),
        name: CharacterName::try_from(name).map_err(|e| RepoError::Corrupt(e.to_string()))?,
        place,
        sheet: decode(version, &sheet)?,
    }))
}

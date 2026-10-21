//! Economy and Moderation screens: the `admin` tool in the page. Reads and
//! writes the game database through the `admin` library, so every action
//! is the same transaction, with the same `admin_log` row, as from the
//! terminal. The actor is `editor:<user>`.

use admin::{Action, reports};
use persistence::{Repository, Sanction};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

/// The database, when the editor was started next to one.
pub struct Db {
    pub url: String,
    pub repo: Option<Repository>,
    /// Why there is no database (shown in the page).
    pub missing: String,
}

impl Db {
    /// Opens `url` only when its file exists: the editor never creates an
    /// empty game database by accident.
    pub fn open(url: &str) -> Db {
        let path = url.strip_prefix("sqlite:").unwrap_or(url);
        if !Path::new(path).is_file() {
            return Db {
                url: url.to_owned(),
                repo: None,
                missing: format!("{path} does not exist: start the editor with --db sqlite:PATH"),
            };
        }
        match Repository::open(url) {
            Ok(repo) => Db {
                url: url.to_owned(),
                repo: Some(repo),
                missing: String::new(),
            },
            Err(e) => Db {
                url: url.to_owned(),
                repo: None,
                missing: e.to_string(),
            },
        }
    }

    pub fn status(&self) -> Value {
        json!({ "url": self.url, "open": self.repo.is_some(), "missing": self.missing })
    }

    fn repo(&self) -> Result<&Repository, String> {
        self.repo.as_ref().ok_or_else(|| self.missing.clone())
    }
}

fn actor() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    format!("editor:{user}")
}

/// Coins, items and levels held now, and the coin flow of the last week.
pub async fn economy(db: &Db) -> Result<Value, String> {
    let repo = db.repo()?;
    let sheets = repo.all_sheets().await.map_err(|e| e.to_string())?;
    let e = reports::economy(&sheets);
    let since = admin::now_unix().saturating_sub(persistence::TELEMETRY_KEEP_SECS);
    let rows = repo.coin_flow(since).await.map_err(|e| e.to_string())?;
    let flow = reports::coin_flow(&rows);
    Ok(json!({
        "characters": e.characters,
        "coins": e.coins,
        "levels": e.levels.iter().map(|(l, n)| json!([l, n])).collect::<Vec<_>>(),
        "items": e.items.iter().map(|(i, (s, u))| json!([i, s, u])).collect::<Vec<_>>(),
        "richest": e.richest.iter().map(|(n, c)| json!([n.as_str(), c])).collect::<Vec<_>>(),
        "flow": {
            "created": flow.created,
            "destroyed": flow.destroyed,
            "sink_permille": flow.sink_permille(),
            "sources": flow.sources,
            "sinks": flow.sinks,
        },
    }))
}

#[derive(Deserialize)]
pub struct SuspectQuery {
    #[serde(default = "day")]
    since: String,
    #[serde(default = "kills")]
    kills_per_hour: u64,
    #[serde(default = "session")]
    max_session: String,
}

fn day() -> String {
    "24h".into()
}
fn kills() -> u64 {
    300
}
fn session() -> String {
    "8h".into()
}

/// Characters above the bot telemetry limits, for a human to look at.
pub async fn suspects(db: &Db, q: SuspectQuery) -> Result<Value, String> {
    let repo = db.repo()?;
    let from = admin::now_unix().saturating_sub(admin::parse_duration(&q.since)?);
    let limits = reports::Limits {
        kills_per_hour: q.kills_per_hour,
        session_secs: admin::parse_duration(&q.max_session)?,
    };
    let rows = repo.telemetry(from).await.map_err(|e| e.to_string())?;
    let names: std::collections::BTreeMap<_, _> = repo
        .all_sheets()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id, name, _)| (id, name))
        .collect();
    let list: Vec<Value> = reports::suspects(&rows, limits)
        .into_iter()
        .map(|s| {
            json!({
                "character": names.get(&s.character).map_or("?", |n| n.as_str()),
                "kills_per_hour": s.kills_per_hour,
                "longest_session_secs": s.longest_session_secs,
                "levels_gained": s.levels_gained,
            })
        })
        .collect();
    Ok(json!(list))
}

fn sanction(s: &Sanction, now: u64) -> Value {
    json!({
        "id": s.id.0,
        "kind": s.kind.to_string(),
        "account": s.account.map(|a| a.get()),
        "ip": s.ip,
        "reason": s.reason,
        "note": s.note,
        "created_at": s.created_at,
        "expires_at": s.expires_at,
        "revoked_at": s.revoked_at,
        "revoked_by": s.revoked_by,
        "created_by": s.created_by,
        "active": s.active_at(now),
    })
}

/// An account by its name, or by the name of one of its characters.
pub async fn account(db: &Db, name: &str) -> Result<Value, String> {
    let repo = db.repo()?;
    let mut found = admin::account_row(repo, name).await;
    if found.is_err()
        && let Ok(ch) = base::CharacterName::try_from(name.trim())
        && let Some((_, account, _)) = repo.character_by_name(&ch).await.map_err(|e| e.to_string())?
        && let Some(row) = repo.account_by_id(account).await.map_err(|e| e.to_string())?
    {
        let account = base::AccountName::try_from(row.name.as_str()).map_err(|e| e.to_string())?;
        found = Ok((account, row));
    }
    let (account, row) = found?;
    let now = admin::now_unix();
    let characters: Vec<Value> = repo
        .characters(row.id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|c| json!({ "id": c.id.get(), "name": c.name.as_str(), "level": c.level }))
        .collect();
    let deleted: Vec<Value> = repo
        .deleted_characters(row.id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|c| json!({ "id": c.id.get(), "name": c.name.as_str(), "at": c.deleted_at }))
        .collect();
    let sanctions: Vec<Value> = repo
        .sanctions_of(row.id)
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .map(|s| sanction(s, now))
        .collect();
    let logins: Vec<Value> = repo
        .recent_logins(row.id, 20)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|l| json!({ "ip": l.ip, "at": l.created_at }))
        .collect();
    Ok(json!({
        "name": account.as_str(),
        "id": row.id.get(),
        "role": row.role.to_string(),
        "locale": row.locale.code(),
        "must_change_password": row.must_change_password,
        "characters": characters,
        "deleted": deleted,
        "sanctions": sanctions,
        "logins": logins,
    }))
}

/// Accounts with at least 10 wrong passwords since their owner last
/// logged in, most first.
pub async fn failed_logins(db: &Db) -> Result<Value, String> {
    let rows = db.repo()?.failed_logins(10).await.map_err(|e| e.to_string())?;
    Ok(json!(
        rows.into_iter()
            .map(|r| json!({ "account": r.name, "failed_logins": r.failed_logins }))
            .collect::<Vec<_>>()
    ))
}

/// Chat reports since `since` (`7d`), newest first, with the lines.
pub async fn chat_reports(db: &Db, since: &str) -> Result<Value, String> {
    let repo = db.repo()?;
    let from = admin::now_unix().saturating_sub(admin::parse_duration(since)?);
    let list: Vec<Value> = repo
        .chat_reports(from)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "reporter": r.reporter,
                "target": r.target,
                "created_at": r.created_at,
                "lines": r.lines.iter().map(|l| json!({ "at": l.at, "channel": l.channel, "from": l.from, "text": l.text })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!(list))
}

/// The admin audit log, newest first.
pub async fn log(db: &Db, account: Option<&str>, since: &str) -> Result<Value, String> {
    let repo = db.repo()?;
    let from = admin::now_unix().saturating_sub(admin::parse_duration(since)?);
    let rows = repo
        .admin_log(account.filter(|a| !a.is_empty()), Some(from), 200)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!(
        rows.into_iter()
            .map(|r| json!({ "at": r.created_at, "actor": r.actor, "action": r.action, "target": r.target, "args": r.args, "note": r.note }))
            .collect::<Vec<_>>()
    ))
}

/// What the page sends to act: the action name and its fields.
#[derive(Deserialize)]
pub struct ActionBody {
    action: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    ip: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    character: String,
    #[serde(default)]
    new_name: String,
}

/// Runs one action; the answer is the line the terminal tool would print
/// (or the temporary password, shown once).
pub async fn act(db: &Db, content: &Path, b: ActionBody) -> Result<String, String> {
    let repo = db.repo()?;
    let secs = || admin::parse_duration(&b.duration);
    let action = match b.action.as_str() {
        "promote" => Action::Promote { account: b.account },
        "demote" => Action::Demote { account: b.account },
        "suspend" => Action::Suspend {
            secs: secs()?,
            account: b.account,
            reason: b.reason,
            note: b.note,
        },
        "ban" => Action::Ban {
            account: b.account,
            reason: b.reason,
            note: b.note,
        },
        "mute" => Action::Mute {
            secs: secs()?,
            account: b.account,
            reason: b.reason,
            note: b.note,
        },
        "ip_ban" => Action::IpBan {
            secs: secs()?,
            ip: b.ip,
            reason: b.reason,
            note: b.note,
        },
        "revoke" => Action::Revoke { id: b.id },
        "reset_password" => Action::ResetPassword { account: b.account },
        "rename" => Action::Rename {
            character: b.character,
            new_name: b.new_name,
        },
        "restore" => Action::Restore {
            character: u32::try_from(b.id).map_err(|_| format!("character id {} out of range", b.id))?,
        },
        "maintenance" => Action::Maintenance {
            secs: admin::maintenance_secs(&b.duration)?,
        },
        other => return Err(format!("unknown action `{other}`")),
    };
    admin::apply(repo, content, &actor(), admin::now_unix(), action).await
}

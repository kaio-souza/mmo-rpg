//! `admin` as a library: the moderation actions and the reports, shared by
//! the terminal tool (`admin`) and the editor's Moderation and Economy
//! screens. Every action is one transaction and leaves a row in
//! `admin_log`.

pub mod reports;

use base::{AccountName, CharacterName};
use persistence::{AccountRow, NewSanction, Repository, Role, SanctionId, SanctionKind, ip_key};
use std::net::IpAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Longest IP ban.
pub const MAX_IP_BAN: u64 = 30 * 24 * 60 * 60;
/// Longest suspension or mute; a permanent block is `ban`.
pub const MAX_DURATION: u64 = 10 * 365 * 24 * 60 * 60;
/// The `admin_log` action the server polls for.
pub const MAINTENANCE_ACTION: &str = "maintenance";
/// Longest wait before an announced restart.
pub const MAX_MAINTENANCE: u64 = 24 * 60 * 60;

/// One moderation action. Durations are in seconds, already checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Promote {
        account: String,
    },
    Demote {
        account: String,
    },
    Suspend {
        account: String,
        secs: u64,
        reason: String,
        note: String,
    },
    Ban {
        account: String,
        reason: String,
        note: String,
    },
    IpBan {
        ip: String,
        secs: u64,
        reason: String,
        note: String,
    },
    Mute {
        account: String,
        secs: u64,
        reason: String,
        note: String,
    },
    Revoke {
        id: u64,
    },
    /// Answers with the temporary password, shown once.
    ResetPassword {
        account: String,
    },
    Rename {
        character: String,
        new_name: String,
    },
    /// Brings back a character the player deleted (P01).
    Restore {
        character: u32,
    },
    /// 0 cancels an announced restart.
    Maintenance {
        secs: u64,
    },
}

/// Runs `action` as `actor` and says what was done (in English, for the
/// log line of the tool that asked).
pub async fn apply(
    repo: &Repository,
    content: &Path,
    actor: &str,
    now: u64,
    action: Action,
) -> Result<String, String> {
    let e = |e: persistence::RepoError| e.to_string();
    match action {
        Action::Promote { account } => {
            let (_, row) = account_row(repo, &account).await?;
            repo.set_role(row.id, Role::Admin, actor, &account, now)
                .await
                .map_err(e)?;
            Ok(format!("`{account}` is now admin"))
        }
        Action::Demote { account } => {
            let (_, row) = account_row(repo, &account).await?;
            repo.set_role(row.id, Role::Player, actor, &account, now)
                .await
                .map_err(e)?;
            Ok(format!("`{account}` is now player"))
        }
        Action::Suspend {
            account,
            secs,
            reason,
            note,
        } => {
            let id = account_sanction(
                repo,
                content,
                actor,
                now,
                SanctionKind::Suspend,
                &account,
                Some(secs),
                &reason,
                note,
            )
            .await?;
            Ok(format!("sanction {id}: `{account}` suspended for {secs} s"))
        }
        Action::Ban {
            account,
            reason,
            note,
        } => {
            let id = account_sanction(
                repo,
                content,
                actor,
                now,
                SanctionKind::Ban,
                &account,
                None,
                &reason,
                note,
            )
            .await?;
            Ok(format!("sanction {id}: `{account}` banned"))
        }
        Action::Mute {
            account,
            secs,
            reason,
            note,
        } => {
            let id = account_sanction(
                repo,
                content,
                actor,
                now,
                SanctionKind::Mute,
                &account,
                Some(secs),
                &reason,
                note,
            )
            .await?;
            Ok(format!("sanction {id}: `{account}` muted for {secs} s"))
        }
        Action::IpBan {
            ip,
            secs,
            reason,
            note,
        } => {
            if secs > MAX_IP_BAN {
                return Err(format!(
                    "an IP ban lasts at most {} days",
                    MAX_IP_BAN / (24 * 60 * 60)
                ));
            }
            check_secs(secs)?;
            let reason = check_reason(content, &reason)?;
            let ip: IpAddr = ip
                .trim()
                .parse()
                .map_err(|_| format!("`{ip}` is not an IP address"))?;
            let key = ip_key(ip);
            let new = NewSanction {
                kind: SanctionKind::IpBan,
                account: None,
                ip: Some(key.clone()),
                target: key.clone(),
                reason,
                note,
                expires_at: Some(now + secs),
                created_by: actor.to_owned(),
            };
            let id = repo.add_sanction(new, now).await.map_err(e)?;
            Ok(format!("sanction {id}: `{key}` blocked for {secs} s"))
        }
        Action::Revoke { id } => {
            let id = SanctionId(id);
            if repo.revoke_sanction(id, actor, now).await.map_err(e)? {
                Ok(format!("sanction {id} revoked"))
            } else {
                Err(format!("sanction {id} does not exist or is already revoked"))
            }
        }
        Action::ResetPassword { account } => {
            let (_, row) = account_row(repo, &account).await?;
            let password = temp_password()?;
            let hash = persistence::hash_password(&password).map_err(|e| e.to_string())?;
            repo.reset_password(row.id, hash, actor, &account, now)
                .await
                .map_err(e)?;
            Ok(password)
        }
        Action::Rename { character, new_name } => {
            let old =
                CharacterName::try_from(character.as_str()).map_err(|e| format!("`{character}`: {e}"))?;
            let new = CharacterName::try_from(new_name.as_str()).map_err(|e| format!("`{new_name}`: {e}"))?;
            let id = repo.rename_character(&old, &new, actor, now).await.map_err(e)?;
            Ok(format!("character {} renamed to `{new}`", id.get()))
        }
        Action::Restore { character } => {
            let id = base::CharacterId::new(character);
            let name = repo
                .restore_character(id, persistence::CHARACTERS_PER_ACCOUNT, actor, now)
                .await
                .map_err(e)?;
            Ok(format!("character {character} `{name}` restored"))
        }
        Action::Maintenance { secs } => {
            if secs > MAX_MAINTENANCE {
                return Err("a restart is announced at most 24h ahead".into());
            }
            repo.log_admin(persistence::AdminLogRow {
                id: 0,
                actor: actor.to_owned(),
                action: MAINTENANCE_ACTION.to_owned(),
                target: "server".to_owned(),
                args: secs.to_string(),
                note: String::new(),
                created_at: now,
            })
            .await
            .map_err(e)?;
            Ok(if secs == 0 {
                "maintenance cancelled (the server reads it within 30 s)".to_owned()
            } else {
                format!("maintenance in {secs} s (the server reads it within 30 s)")
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn account_sanction(
    repo: &Repository,
    content: &Path,
    actor: &str,
    now: u64,
    kind: SanctionKind,
    account: &str,
    secs: Option<u64>,
    reason: &str,
    note: String,
) -> Result<SanctionId, String> {
    if let Some(s) = secs {
        check_secs(s)?;
    }
    let reason = check_reason(content, reason)?;
    let (_, row) = account_row(repo, account).await?;
    let new = NewSanction {
        kind,
        account: Some(row.id),
        ip: None,
        target: account.to_owned(),
        reason,
        note,
        expires_at: secs.map(|s| now + s),
        created_by: actor.to_owned(),
    };
    repo.add_sanction(new, now).await.map_err(|e| e.to_string())
}

fn check_secs(secs: u64) -> Result<(), String> {
    if secs == 0 {
        Err("a duration must be more than zero".into())
    } else if secs > MAX_DURATION {
        Err("longer than 10 years".into())
    } else {
        Ok(())
    }
}

/// The account by name, or an error that names it.
pub async fn account_row(repo: &Repository, name: &str) -> Result<(AccountName, AccountRow), String> {
    let account = AccountName::try_from(name.trim()).map_err(|e| format!("`{name}`: {e}"))?;
    let row = repo
        .account(&account)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("account `{name}` not found"))?;
    Ok((account, row))
}

/// Seconds until the restart; 0 for `cancel`.
pub fn maintenance_secs(when: &str) -> Result<u64, String> {
    if when.trim().eq_ignore_ascii_case("cancel") {
        return Ok(0);
    }
    let secs = parse_duration(when)?;
    if secs > MAX_MAINTENANCE {
        return Err(format!("`{when}`: at most 24h"));
    }
    Ok(secs)
}

/// `30s`, `10m`, `1h`, `3d`: seconds, minutes, hours or days.
pub fn parse_duration(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("`{text}`: add a unit (s, m, h or d)"))?;
    let (number, unit) = text.split_at(split);
    let n: u64 = number.parse().map_err(|_| format!("`{text}`: not a duration"))?;
    let unit_secs: u64 = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        _ => return Err(format!("`{text}`: unit must be s, m, h or d")),
    };
    // Ten years at most: an end date past that is a typo, and the sum with
    // the current time must not overflow.
    let seconds = n
        .checked_mul(unit_secs)
        .filter(|s| *s <= MAX_DURATION)
        .ok_or_else(|| format!("`{text}`: longer than 10 years"))?;
    if seconds == 0 {
        return Err("a duration must be more than zero".into());
    }
    Ok(seconds)
}

/// The full text keys of the moderation reasons (`content/moderation/`).
pub fn reasons(content: &Path) -> Result<Vec<String>, String> {
    let catalog = defs::load(content).map_err(|e| e.to_string())?;
    let catalog = catalog.validate().map_err(|problems| {
        problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    Ok(catalog.moderation_reasons().to_vec())
}

/// The full key of a reason; `cheating` is accepted for
/// `moderation.reason.cheating`.
pub fn check_reason(content: &Path, reason: &str) -> Result<String, String> {
    let allowed = reasons(content)?;
    let full = if allowed.iter().any(|r| r == reason) {
        reason.to_owned()
    } else {
        format!("moderation.reason.{reason}")
    };
    if allowed.contains(&full) {
        Ok(full)
    } else {
        let names: Vec<&str> = allowed
            .iter()
            .map(|r| r.strip_prefix("moderation.reason.").unwrap_or(r))
            .collect();
        Err(format!(
            "unknown reason `{reason}`; use one of: {}",
            names.join(", ")
        ))
    }
}

/// A temporary password: 16 characters, no look-alikes.
pub fn temp_password() -> Result<String, String> {
    // No look-alike characters (0/O, 1/l/I): the player types it by hand.
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    const LEN: usize = 16;
    // Rejection sampling: bytes past the last multiple of the alphabet size
    // are dropped, so every character is equally likely.
    let limit = 256 - 256 % ALPHABET.len();
    let mut out = String::with_capacity(LEN);
    while out.len() < LEN {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
        for b in bytes {
            let b = usize::from(b);
            if b < limit && out.len() < LEN {
                out.push(char::from(ALPHABET[b % ALPHABET.len()]));
            }
        }
    }
    Ok(out)
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn maintenance_takes_a_duration_or_cancel() {
        assert_eq!(maintenance_secs("10m"), Ok(600));
        assert_eq!(maintenance_secs("cancel"), Ok(0));
        assert!(maintenance_secs("2d").is_err());
        assert!(maintenance_secs("soon").is_err());
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("30s"), Ok(30));
        assert_eq!(parse_duration("10m"), Ok(600));
        assert_eq!(parse_duration("1h"), Ok(3600));
        assert_eq!(parse_duration("3d"), Ok(3 * 24 * 3600));
        assert!(parse_duration("3").is_err());
        assert!(parse_duration("3w").is_err());
        assert!(parse_duration("0d").is_err());
        assert!(parse_duration("99999999999999999999d").is_err());
        assert!(parse_duration("4000d").is_err());
    }

    #[test]
    fn temporary_password_has_no_look_alikes() {
        let p = temp_password().unwrap();
        assert_eq!(p.len(), 16);
        assert!(!p.contains(['0', 'O', '1', 'l', 'I']));
    }
}

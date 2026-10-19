//! `admin`: moderation from the terminal (phase 2).
//!
//! Opens the database directly, the same way the server does, so it works
//! with the server online (SQLite WAL, short transactions). Every action is
//! one transaction and leaves a row in `admin_log`.
//!
//! ```text
//! admin --db sqlite:data/game.db promote CONTA
//! admin suspend CONTA --for 3d --reason botting [--note "..."]
//! admin ban CONTA --reason cheating [--note "..."]
//! admin ip-ban 203.0.113.7 --for 7d --reason spam
//! admin mute CONTA --for 1h --reason harassment
//! admin revoke SANCTION_ID
//! admin reset-password CONTA
//! admin rename PERSONAGEM NOVO_NOME
//! admin show CONTA
//! admin log [--account CONTA] [--since 7d]
//! admin verify-backup ARQUIVO
//! admin maintenance 10m | cancel
//! admin report economy
//! admin suspects [--since 24h] [--kills-per-hour 300] [--max-session 8h]
//! admin reports [--since 7d]
//! ```
//!
//!

use admin::{Action, account_row, maintenance_secs, now_unix, parse_duration, reports};
use clap::{Parser, Subcommand};
use persistence::{Repository, Sanction};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(version, about = "Moderation tool")]
struct Args {
    /// Database URL, same format as the server (`sqlite:PATH`).
    #[arg(long, default_value = "sqlite:data/game.db", global = true)]
    db: String,
    /// Content folder, for the reason list.
    #[arg(long, default_value = "content", global = true)]
    content: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug, Clone)]
enum Cmd {
    /// Gives the account the `admin` role.
    Promote { account: String },
    /// Takes the `admin` role from the account.
    Demote { account: String },
    /// Blocks the account for a while.
    Suspend {
        account: String,
        #[arg(long = "for", value_name = "DURATION")]
        duration: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Blocks the account forever.
    Ban {
        account: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Blocks every account connecting from an IP (IPv6: /64).
    IpBan {
        ip: String,
        #[arg(long = "for", value_name = "DURATION")]
        duration: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Silences the account in chat (phase 5).
    Mute {
        account: String,
        #[arg(long = "for", value_name = "DURATION")]
        duration: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Revokes a sanction; the row stays with `revoked_at`.
    Revoke { id: u64 },
    /// Sets a temporary password, shown once; the player must change it.
    ResetPassword { account: String },
    /// Renames a character; an online one is disconnected.
    Rename { character: String, new_name: String },
    /// Brings back a deleted character by id (see `show`); refused when the
    /// account is full or the name was taken.
    Restore { character: u32 },
    /// Shows characters, sanctions and recent logins.
    Show { account: String },
    /// Shows the admin audit log, newest first.
    Log {
        #[arg(long)]
        account: Option<String>,
        #[arg(long, value_name = "DURATION")]
        since: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Runs `PRAGMA integrity_check` on a backup copy.
    VerifyBackup { file: String },
    /// Announces a restart: players get a countdown, and at zero the server
    /// saves everyone and stops. `cancel` calls it off. The
    /// server reads it within 30 s.
    Maintenance {
        /// `10m`, `90s`... (at most 24h), or `cancel`.
        when: String,
    },
    /// Reports read from the database (phase 4).
    Report {
        #[command(subcommand)]
        kind: Report,
    },
    /// Characters above the bot telemetry limits, for human review.
    Suspects {
        #[arg(long, value_name = "DURATION", default_value = "24h")]
        since: String,
        #[arg(long, default_value_t = 300)]
        kills_per_hour: u64,
        #[arg(long, value_name = "DURATION", default_value = "8h")]
        max_session: String,
    },
    /// Chat reports made in the game, newest first, with the lines the
    /// reporter could see (phase 5; kept 7 days).
    Reports {
        #[arg(long, value_name = "DURATION", default_value = "7d")]
        since: String,
    },
    /// Accounts with wrong passwords since their last login, most first
    /// a target of guessing. Cleared when the owner logs in.
    FailedLogins {
        #[arg(long, default_value_t = 10)]
        min: u32,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum Report {
    /// Coins, items and levels held by every character right now.
    Economy,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: cannot start the runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), String> {
    if let Cmd::VerifyBackup { file } = &args.cmd {
        let (accounts, characters) = Repository::verify_backup(file).map_err(|e| e.to_string())?;
        println!("backup ok: {file} ({accounts} accounts, {characters} characters)");
        return Ok(());
    }

    let repo = Repository::open(&args.db).map_err(|e| e.to_string())?;
    let actor = actor();
    let now = now_unix();
    let simple = match args.cmd.clone() {
        Cmd::Promote { account } => Some(Action::Promote { account }),
        Cmd::Demote { account } => Some(Action::Demote { account }),
        Cmd::Suspend {
            account,
            duration,
            reason,
            note,
        } => Some(Action::Suspend {
            account,
            secs: parse_duration(&duration)?,
            reason,
            note,
        }),
        Cmd::Ban {
            account,
            reason,
            note,
        } => Some(Action::Ban {
            account,
            reason,
            note,
        }),
        Cmd::IpBan {
            ip,
            duration,
            reason,
            note,
        } => Some(Action::IpBan {
            ip,
            secs: parse_duration(&duration)?,
            reason,
            note,
        }),
        Cmd::Mute {
            account,
            duration,
            reason,
            note,
        } => Some(Action::Mute {
            account,
            secs: parse_duration(&duration)?,
            reason,
            note,
        }),
        Cmd::Revoke { id } => Some(Action::Revoke { id }),
        Cmd::ResetPassword { ref account } => Some(Action::ResetPassword {
            account: account.clone(),
        }),
        Cmd::Rename { character, new_name } => Some(Action::Rename { character, new_name }),
        Cmd::Restore { character } => Some(Action::Restore { character }),
        Cmd::Maintenance { ref when } => Some(Action::Maintenance {
            secs: maintenance_secs(when)?,
        }),
        _ => None,
    };
    if let Some(action) = simple {
        let reset = match &action {
            Action::ResetPassword { account } => Some(account.clone()),
            _ => None,
        };
        let said = admin::apply(&repo, &args.content, &actor, now, action).await?;
        match reset {
            Some(account) => println!("temporary password for `{account}` (shown once): {said}"),
            None => println!("{said}"),
        }
        return Ok(());
    }
    match args.cmd {
        Cmd::Promote { .. }
        | Cmd::Demote { .. }
        | Cmd::Suspend { .. }
        | Cmd::Ban { .. }
        | Cmd::IpBan { .. }
        | Cmd::Mute { .. }
        | Cmd::Revoke { .. }
        | Cmd::ResetPassword { .. }
        | Cmd::Rename { .. }
        | Cmd::Restore { .. }
        | Cmd::Maintenance { .. } => unreachable!("handled above"),
        Cmd::Show { account } => {
            let (name, row) = account_row(&repo, &account).await?;
            println!(
                "account {name} (id {}, role {}, locale {}, must_change_password {})",
                row.id.get(),
                row.role,
                row.locale,
                row.must_change_password
            );
            if let Some(f) = repo
                .failed_logins(1)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|f| f.name == row.name)
            {
                println!("wrong passwords since the last login: {}", f.failed_logins);
            }
            println!("characters:");
            for ch in repo.characters(row.id).await.map_err(|e| e.to_string())? {
                println!("  {} {}", ch.id.get(), ch.name);
            }
            let deleted = repo.deleted_characters(row.id).await.map_err(|e| e.to_string())?;
            if !deleted.is_empty() {
                println!("deleted characters (admin restore ID):");
                for ch in deleted {
                    println!("  {} {} deleted_at {}", ch.id.get(), ch.name, ch.deleted_at);
                }
            }
            println!("sanctions:");
            for s in repo.sanctions_of(row.id).await.map_err(|e| e.to_string())? {
                print_sanction(&s);
            }
            println!("last logins:");
            for l in repo.recent_logins(row.id, 20).await.map_err(|e| e.to_string())? {
                println!("  {} {}", l.created_at, l.ip);
            }
        }
        Cmd::Log {
            account,
            since,
            limit,
        } => {
            let since = match since {
                Some(text) => Some(now.saturating_sub(parse_duration(&text)?)),
                None => None,
            };
            let rows = repo
                .admin_log(account.as_deref(), since, limit)
                .await
                .map_err(|e| e.to_string())?;
            for r in rows {
                println!(
                    "{} {} {} target={} args={} note={}",
                    r.created_at, r.actor, r.action, r.target, r.args, r.note
                );
            }
        }
        Cmd::VerifyBackup { .. } => unreachable!("handled before opening the database"),
        Cmd::Report {
            kind: Report::Economy,
        } => {
            let sheets = repo.all_sheets().await.map_err(|e| e.to_string())?;
            let e = reports::economy(&sheets);
            println!("characters: {}", e.characters);
            println!("coins (copper): {}", e.coins);
            println!("levels:");
            for (level, n) in &e.levels {
                println!("  {level}: {n}");
            }
            println!("items (in stacks, with identity):");
            for (item, (stacked, unique)) in &e.items {
                println!("  {item}: {stacked} {unique}");
            }
            println!("richest:");
            for (name, coins) in &e.richest {
                println!("  {} {coins}", name.as_str());
            }
            // Coins created and destroyed in the last week (phase 7).
            let since = now_unix().saturating_sub(persistence::TELEMETRY_KEEP_SECS);
            let rows = repo.coin_flow(since).await.map_err(|e| e.to_string())?;
            let flow = reports::coin_flow(&rows);
            println!("coins created (last 7 days): {}", flow.created);
            println!("coins destroyed: {}", flow.destroyed);
            match flow.sink_permille() {
                // The design wants sinks at 90% of the sources or more.
                Some(p) => println!("destroyed / created: {}.{}%", p / 10, p % 10),
                None => println!("destroyed / created: no coins created yet"),
            }
            println!("sources:");
            for (name, coins) in &flow.sources {
                println!("  {name}: {coins}");
            }
            println!("sinks:");
            for (name, coins) in &flow.sinks {
                println!("  {name}: {coins}");
            }
        }
        Cmd::FailedLogins { min } => {
            let rows = repo.failed_logins(min).await.map_err(|e| e.to_string())?;
            if rows.is_empty() {
                println!("no account with {min} or more wrong passwords");
            }
            for r in rows {
                println!("  {} {}", r.failed_logins, r.name);
            }
        }
        Cmd::Reports { since } => {
            let from = now.saturating_sub(parse_duration(&since)?);
            let list = repo.chat_reports(from).await.map_err(|e| e.to_string())?;
            if list.is_empty() {
                println!("no reports since {since}");
            }
            for r in list {
                println!("#{} {} {} reported {}", r.id, r.created_at, r.reporter, r.target);
                for l in r.lines {
                    println!("  {} [{}] {}: {}", l.at, l.channel, l.from, l.text);
                }
            }
        }
        Cmd::Suspects {
            since,
            kills_per_hour,
            max_session,
        } => {
            let from = now.saturating_sub(parse_duration(&since)?);
            let limits = reports::Limits {
                kills_per_hour,
                session_secs: parse_duration(&max_session)?,
            };
            let rows = repo.telemetry(from).await.map_err(|e| e.to_string())?;
            let names: std::collections::BTreeMap<_, _> = repo
                .all_sheets()
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|(id, name, _)| (id, name))
                .collect();
            let list = reports::suspects(&rows, limits);
            if list.is_empty() {
                println!("no suspects since {since}");
            }
            for s in list {
                let name = names.get(&s.character).map_or("?", |n| n.as_str());
                println!(
                    "{name} kills/h={} longest_session={}s levels+={}",
                    s.kills_per_hour, s.longest_session_secs, s.levels_gained
                );
            }
        }
    }
    Ok(())
}

fn print_sanction(s: &Sanction) {
    let target = match (s.account, &s.ip) {
        (Some(id), _) => format!("account {}", id.get()),
        (None, Some(ip)) => format!("ip {ip}"),
        (None, None) => "?".into(),
    };
    let end = match s.expires_at {
        Some(t) => format!("until {t}"),
        None => "permanent".into(),
    };
    let revoked = match (s.revoked_at, &s.revoked_by) {
        (Some(t), Some(by)) => format!(" revoked at {t} by {by}"),
        (Some(t), None) => format!(" revoked at {t}"),
        _ => String::new(),
    };
    println!(
        "  #{} {} {target} reason={} note={} created={} {end}{revoked}",
        s.id, s.kind, s.reason, s.note, s.created_at
    );
}

fn actor() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    format!("cli:{user}")
}

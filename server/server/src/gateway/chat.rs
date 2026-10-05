//! Chat, reports and in-game admin commands (phase 5,
//!). Chat runs in the gateway: it knows
//! every player's tile from `FromZone::Moved`, so the zone tick stays
//! deterministic and free of text.

use super::{Done, Gateway, Phase};
use crate::login;
use base::{Arg, CharacterName, ChatText, PlayerId, TextRef, TilePos};
use persistence::{AdminLogRow, ChatLine, ChatReport, NewSanction, RepoError, Role, SanctionKind};
use protocol::{AdminCommand, ChatChannel, ClientMsg, ErrorCode, ServerMsg};
use server_net::ConnectionId;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Chat lines per player: at most this many per second and per minute
/// more are refused with `TooFast`.
const PER_SECOND: usize = 2;
const PER_MINUTE: usize = 20;
/// Recent lines kept for reports, over the whole gateway.
const HISTORY: usize = 2000;
/// Lines stored with a report.
pub(crate) const REPORT_LINES: usize = 50;

/// Send times of the last minute.
#[derive(Default)]
pub(crate) struct Limiter {
    times: VecDeque<Instant>,
}

impl Limiter {
    /// `true` and counts the line when it is within both limits.
    pub(crate) fn allow(&mut self, now: Instant) -> bool {
        while self
            .times
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            self.times.pop_front();
        }
        let last_second = self
            .times
            .iter()
            .filter(|t| now.duration_since(**t) < Duration::from_secs(1))
            .count();
        if last_second >= PER_SECOND || self.times.len() >= PER_MINUTE {
            return false;
        }
        self.times.push_back(now);
        true
    }
}

/// A line said in the game, for reports.
pub(crate) struct Said {
    at: u64,
    pos: TilePos,
    channel: ChatChannel,
    from: CharacterName,
    text: ChatText,
}

#[derive(Default)]
pub(crate) struct History {
    lines: VecDeque<Said>,
}

impl History {
    fn push(&mut self, said: Said) {
        if self.lines.len() == HISTORY {
            self.lines.pop_front();
        }
        self.lines.push_back(said);
    }

    /// The last `REPORT_LINES` lines `who` at `pos` could have seen: local
    /// lines within `radius`, and whispers to or from them.
    pub(crate) fn seen_by(&self, who: &CharacterName, pos: TilePos, radius: u32) -> Vec<ChatLine> {
        let same = |a: &CharacterName, b: &CharacterName| a.eq_ignore_ascii_case(b);
        let mut out: Vec<ChatLine> = self
            .lines
            .iter()
            .rev()
            .filter(|s| match &s.channel {
                ChatChannel::Local => s.pos.distance(pos) <= radius,
                ChatChannel::Whisper(to) => same(to, who) || same(&s.from, who),
            })
            .take(REPORT_LINES)
            .map(|s| ChatLine {
                at: s.at,
                channel: match &s.channel {
                    ChatChannel::Local => "local".into(),
                    ChatChannel::Whisper(to) => format!("whisper:{to}"),
                },
                from: s.from.to_string(),
                text: s.text.to_string(),
            })
            .collect();
        out.reverse();
        out
    }
}

/// `harassment` -> `moderation.reason.harassment`, if it is in the list.
pub(crate) fn reason_key(reasons: &[String], short: &str) -> Option<String> {
    let full = format!("moderation.reason.{short}");
    reasons.iter().find(|r| **r == full).cloned()
}

impl Gateway {
    /// A System line built from a text key of the content.
    pub(super) fn system(&self, c: ConnectionId, key: &str, args: Vec<Arg>) {
        let Some(id) = self.keys.id(key) else {
            warn!(key, "system text key missing");
            return;
        };
        let mut text = TextRef::new(id);
        text.args.extend(args);
        // Answers to chat commands: the log is enough.
        self.net.send_msg(c, &ServerMsg::System { text, window: false });
    }

    /// The connection of an online character, by name (any case).
    pub(super) fn online(&self, name: &CharacterName) -> Option<(PlayerId, ConnectionId)> {
        let (player, _) = self
            .in_game
            .iter()
            .find(|(_, who)| who.name.eq_ignore_ascii_case(name))?;
        let c = *self.players.get(player)?;
        self.conns
            .get(&c)
            .filter(|conn| matches!(conn.phase, Phase::InZone(_)))
            .map(|_| (*player, c))
    }

    /// Chat, reports and admin commands of a player in the zone.
    pub(super) fn on_chat_msg(&mut self, c: ConnectionId, msg: ClientMsg) {
        let in_zone = self
            .conns
            .get(&c)
            .is_some_and(|conn| matches!(conn.phase, Phase::InZone(_)));
        if !in_zone {
            return self.fail(c, ErrorCode::InvalidMessage);
        }
        match msg {
            ClientMsg::Chat { channel, text } => self.on_chat(c, channel, text),
            ClientMsg::ReportChat { name } => self.on_report(c, name),
            ClientMsg::Admin(cmd) => self.on_admin(c, cmd),
            _ => self.fail(c, ErrorCode::InvalidMessage),
        }
    }

    fn speaker(&self, c: ConnectionId) -> Option<(CharacterName, TilePos)> {
        let conn = self.conns.get(&c)?;
        let Phase::InZone(player) = conn.phase else {
            return None;
        };
        Some((self.in_game.get(&player)?.name.clone(), conn.pos))
    }

    pub(super) fn on_chat(&mut self, c: ConnectionId, channel: ChatChannel, text: ChatText) {
        let now = login::now_unix();
        let Some((from, pos)) = self.speaker(c) else {
            return;
        };
        let Some(conn) = self.conns.get_mut(&c) else {
            return;
        };
        if let Some(until) = conn.muted_until.filter(|u| *u > now) {
            let secs = i64::try_from(until - now).unwrap_or(i64::MAX);
            return self.reply_error(c, ErrorCode::Muted, vec![Arg::Int(secs)]);
        }
        if !conn.chat.allow(Instant::now()) {
            return self.reply_error(c, ErrorCode::TooFast, Vec::new());
        }
        let msg = ServerMsg::ChatMessage {
            channel: channel.clone(),
            from: from.clone(),
            text: text.clone(),
        };
        let mut frame = Vec::new();
        if protocol::encode(&msg, &mut frame).is_err() {
            return;
        }
        let frame = bytes::Bytes::from(frame);
        match &channel {
            ChatChannel::Local => {
                let radius = u32::from(self.cfg.view_radius);
                for (&to, other) in &self.conns {
                    if matches!(other.phase, Phase::InZone(_)) && other.pos.distance(pos) <= radius {
                        self.net.send(to, frame.clone());
                    }
                }
            }
            ChatChannel::Whisper(name) => {
                let Some((_, to)) = self.online(name) else {
                    return self.reply_error(c, ErrorCode::PlayerOffline, Vec::new());
                };
                self.net.send(to, frame.clone());
                if to != c {
                    self.net.send(c, frame);
                }
            }
        }
        self.chat_history.push(Said {
            at: now,
            pos,
            channel,
            from,
            text,
        });
    }

    /// `/report NAME`: stores what the reporter could see. Counts against
    /// the chat limit, so it cannot flood the database.
    pub(super) fn on_report(&mut self, c: ConnectionId, target: CharacterName) {
        let Some((reporter, pos)) = self.speaker(c) else {
            return;
        };
        let Some(conn) = self.conns.get_mut(&c) else {
            return;
        };
        if !conn.chat.allow(Instant::now()) {
            return self.reply_error(c, ErrorCode::TooFast, Vec::new());
        }
        let report = ChatReport {
            id: 0,
            reporter: reporter.to_string(),
            target: target.to_string(),
            lines: self
                .chat_history
                .seen_by(&reporter, pos, u32::from(self.cfg.view_radius)),
            created_at: login::now_unix(),
        };
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let result = match repo.add_chat_report(report).await {
                Ok(()) => Done::Reply {
                    c,
                    key: "chat.reported",
                    args: vec![Arg::Str(target.to_string())],
                    poll: false,
                },
                Err(e) => {
                    error!(error = %e, "cannot store the report");
                    Done::Error {
                        c,
                        code: ErrorCode::Internal,
                    }
                }
            };
            let _ = done.send(result);
        });
    }

    pub(super) fn on_admin(&mut self, c: ConnectionId, cmd: AdminCommand) {
        let Some(conn) = self.conns.get(&c) else { return };
        if conn.role != Role::Admin {
            warn!(target: "security", peer = %conn.peer, account = ?conn.account, ?cmd, "admin command from a player account");
            return self.reply_error(c, ErrorCode::NotAllowed, Vec::new());
        }
        let actor = conn.account_name.clone();
        let zone = "world".to_owned();
        info!(%actor, ?cmd, "admin command");
        let log = |action: &str, target: &str, args: String| AdminLogRow {
            id: 0,
            actor: actor.clone(),
            action: action.to_owned(),
            target: target.to_owned(),
            args,
            note: String::new(),
            created_at: login::now_unix(),
        };
        match cmd {
            AdminCommand::Kick { name } => {
                let Some((_, target)) = self.online(&name) else {
                    return self.reply_error(c, ErrorCode::PlayerOffline, Vec::new());
                };
                // The zone saves the character on the way out, as on any drop.
                self.net.disconnect(target);
                self.audit(log("kick", name.as_str(), String::new()));
                self.system(c, "admin.kicked", vec![Arg::Str(name.to_string())]);
            }
            AdminCommand::Replay => {
                for z in self.zones.values() {
                    let _ = z.tx.send(crate::zone_link::ToZone::SaveReplay);
                }
                self.audit(log("replay", &zone, String::new()));
                self.system(c, "admin.replay", Vec::new());
            }
            AdminCommand::Who { name } => {
                let found = self.online(&name).and_then(|(player, to)| {
                    let w = self.in_game.get(&player)?;
                    let pos = self.conns.get(&to)?.pos;
                    Some((w.account, w.zone, pos))
                });
                let Some((account, zone_id, pos)) = found else {
                    return self.reply_error(c, ErrorCode::PlayerOffline, Vec::new());
                };
                let account = account.map_or_else(|| "dev".to_owned(), |a| a.get().to_string());
                let zone = self.zone_name(zone_id);
                self.audit(log("who", name.as_str(), String::new()));
                self.system(
                    c,
                    "admin.who",
                    vec![
                        Arg::Str(name.to_string()),
                        Arg::Str(account),
                        Arg::Str(zone),
                        Arg::Int(i64::from(pos.x)),
                        Arg::Int(i64::from(pos.y)),
                    ],
                );
            }
            AdminCommand::Mute { name, secs, reason } => {
                self.sanction(c, actor, SanctionKind::Mute, name, Some(secs), reason.as_str())
            }
            AdminCommand::Suspend { name, secs, reason } => {
                self.sanction(c, actor, SanctionKind::Suspend, name, Some(secs), reason.as_str())
            }
            AdminCommand::Ban { name, reason } => {
                self.sanction(c, actor, SanctionKind::Ban, name, None, reason.as_str())
            }
            // The same row as `admin maintenance`, applied by the
            // poll right after it is written.
            AdminCommand::Maintenance { secs } => {
                let row = log(super::MAINTENANCE_ACTION, "server", secs.to_string());
                let (repo, done) = (self.repo.clone(), self.done_tx.clone());
                tokio::spawn(async move {
                    let reply = match repo.log_admin(row).await {
                        Ok(()) => Done::Reply {
                            c,
                            key: if secs == 0 {
                                "admin.maintenance_cancelled"
                            } else {
                                "admin.maintenance"
                            },
                            args: if secs == 0 {
                                Vec::new()
                            } else {
                                vec![Arg::Int(i64::from(secs))]
                            },
                            poll: true,
                        },
                        Err(e) => {
                            error!(error = %e, "cannot write the maintenance row");
                            Done::Error {
                                c,
                                code: ErrorCode::Internal,
                            }
                        }
                    };
                    let _ = done.send(reply);
                });
            }
        }
    }

    fn audit(&self, row: AdminLogRow) {
        let repo = self.repo.clone();
        tokio::spawn(async move {
            if let Err(e) = repo.log_admin(row).await {
                error!(error = %e, "cannot write the admin log");
            }
        });
    }

    /// Mute, suspension or ban of the account that owns `name`, online or
    /// not. Applied right away by the same poll that applies `tools/admin`.
    fn sanction(
        &self,
        c: ConnectionId,
        actor: String,
        kind: SanctionKind,
        name: CharacterName,
        secs: Option<u32>,
        reason: &str,
    ) {
        let Some(reason) = reason_key(&self.cfg.content.reasons, reason) else {
            return self.system(c, "admin.bad_reason", vec![Arg::Str(reason.to_owned())]);
        };
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let result = async {
                let Some((_, account, account_name)) = repo.character_by_name(&name).await? else {
                    return Ok(Done::Error {
                        c,
                        code: ErrorCode::PlayerOffline,
                    });
                };
                let now = login::now_unix();
                let new = NewSanction {
                    kind,
                    account: Some(account),
                    ip: None,
                    target: account_name,
                    reason,
                    note: format!("in game, character {name}"),
                    expires_at: secs.map(|s| now + u64::from(s)),
                    created_by: actor,
                };
                repo.add_sanction(new, now).await?;
                let key = match kind {
                    SanctionKind::Mute => "admin.muted",
                    SanctionKind::Suspend => "admin.suspended",
                    _ => "admin.banned",
                };
                Ok::<_, RepoError>(Done::Reply {
                    c,
                    key,
                    args: vec![Arg::Str(name.to_string())],
                    poll: true,
                })
            }
            .await;
            let _ = done.send(result.unwrap_or_else(|e| {
                error!(error = %e, "cannot write the sanction");
                Done::Error {
                    c,
                    code: ErrorCode::Internal,
                }
            }));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_counts_per_second_and_minute() {
        let mut l = Limiter::default();
        let t = Instant::now();
        assert!(l.allow(t) && l.allow(t));
        assert!(!l.allow(t), "third line in the same second");
        let mut sent = 2;
        for s in 1..60 {
            if l.allow(t + Duration::from_secs(s)) {
                sent += 1;
            }
        }
        assert_eq!(sent, PER_MINUTE);
        assert!(l.allow(t + Duration::from_secs(61)));
    }

    #[test]
    fn history_keeps_what_the_reporter_saw() {
        let name = |s: &str| CharacterName::try_from(s).unwrap();
        let mut h = History::default();
        let line = |from: &str, channel, x| Said {
            at: 1,
            pos: TilePos::new(x, 0),
            channel,
            from: name(from),
            text: ChatText::try_from("hi").unwrap(),
        };
        h.push(line("Kai", ChatChannel::Local, 0));
        h.push(line("Kai", ChatChannel::Local, 500));
        h.push(line("Kai", ChatChannel::Whisper(name("ana")), 900));
        h.push(line("Kai", ChatChannel::Whisper(name("Bob")), 0));
        let seen = h.seen_by(&name("Ana"), TilePos::new(3, 0), 20);
        let channels: Vec<&str> = seen.iter().map(|l| l.channel.as_str()).collect();
        assert_eq!(channels, ["local", "whisper:ana"]);
        for i in 0..100 {
            h.push(line("Kai", ChatChannel::Local, i % 3));
        }
        assert_eq!(
            h.seen_by(&name("Ana"), TilePos::new(0, 0), 20).len(),
            REPORT_LINES
        );
        assert_eq!(
            reason_key(&["moderation.reason.spam".into()], "spam").as_deref(),
            Some("moderation.reason.spam")
        );
        assert_eq!(reason_key(&["moderation.reason.spam".into()], "x"), None);
    }
}

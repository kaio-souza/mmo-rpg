//! Chat input: plain text is local chat; `/` starts a command
//! The server checks everything again: this only builds typed messages.

use base::{CharacterName, ChatText};
use protocol::{AdminCommand, ChatChannel, ClientMsg, ReasonId};

/// What a typed line becomes.
#[derive(Debug, PartialEq, Eq)]
pub enum Typed {
    Send(ClientMsg),
    /// Wrong use of a known command: the syntax to show.
    Usage(&'static str),
    /// `/name` that does not exist.
    Unknown(String),
    /// Phase 9: a group or trade command; names become entities in the game.
    Social(Social),
    /// Phase 9: `/emote NAME`, or `/emote` alone to list them.
    Emote(Option<String>),
}

/// Group and trade commands as typed.
#[derive(Debug, PartialEq, Eq)]
pub enum Social {
    Invite(CharacterName),
    Accept,
    Decline,
    Leave,
    PartyKick(CharacterName),
    PartyLead(CharacterName),
    Trade(CharacterName),
    /// `(backpack slot, quantity)` pairs and copper.
    Offer {
        items: Vec<(u16, u32)>,
        coins: u64,
    },
    Confirm,
    Cancel,
}

const USAGE_WHISPER: &str = "/w NAME TEXT";
const USAGE_KICK: &str = "/kick NAME";
const USAGE_MUTE: &str = "/mute NAME 1h REASON";
const USAGE_SUSPEND: &str = "/suspend NAME 3d REASON";
const USAGE_BAN: &str = "/ban NAME REASON";
const USAGE_WHO: &str = "/who NAME";
const USAGE_MAINTENANCE: &str = "/maintenance 10m | cancel";
const USAGE_REPORT: &str = "/report NAME";
const USAGE_INVITE: &str = "/invite NAME";
const USAGE_PARTY_KICK: &str = "/partykick NAME";
const USAGE_PARTY_LEAD: &str = "/lead NAME";
const USAGE_TRADE: &str = "/trade NAME";
const USAGE_OFFER: &str = "/offer SLOT QTY [SLOT QTY ...] [coins N]";

/// `3 2 5 1 coins 200`: slot and quantity pairs, then optional copper.
fn offer(args: &[&str]) -> Option<Social> {
    let (pairs, coins) = match args.iter().position(|a| a.eq_ignore_ascii_case("coins")) {
        Some(i) => (args.get(..i)?, args.get(i + 1..)?),
        None => (args, &[][..]),
    };
    let coins = match coins {
        [] => 0,
        [n] => n.parse().ok()?,
        _ => return None,
    };
    if pairs.len() % 2 != 0 {
        return None;
    }
    let items = pairs
        .chunks(2)
        .map(|p| Some((p.first()?.parse().ok()?, p.get(1)?.parse().ok()?)))
        .collect::<Option<Vec<(u16, u32)>>>()?;
    Some(Social::Offer { items, coins })
}

/// `30m`, `1h`, `3d` (or plain seconds) in seconds; at most a year.
pub fn duration(text: &str) -> Option<u32> {
    let (num, unit) = match text.char_indices().last()? {
        (i, c) if c.is_ascii_alphabetic() => (&text[..i], c),
        _ => (text, 's'),
    };
    let n: u32 = num.parse().ok()?;
    let mult = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86_400,
        _ => return None,
    };
    n.checked_mul(mult).filter(|s| *s > 0 && *s <= 366 * 86_400)
}

pub fn parse(line: &str) -> Typed {
    let line = line.trim();
    let Some(rest) = line.strip_prefix('/') else {
        return match ChatText::try_from(line) {
            Ok(text) => Typed::Send(ClientMsg::Chat {
                channel: ChatChannel::Local,
                text,
            }),
            // The field already caps the length; nothing else can fail.
            Err(_) => Typed::Usage(USAGE_WHISPER),
        };
    };
    let mut words = rest.split_whitespace();
    let cmd = words.next().unwrap_or_default().to_ascii_lowercase();
    let args: Vec<&str> = words.collect();
    let name = |i: usize| args.get(i).and_then(|n| CharacterName::try_from(*n).ok());
    let reason = |i: usize| args.get(i).and_then(|r| ReasonId::try_from(*r).ok());
    let admin = |c: AdminCommand| Typed::Send(ClientMsg::Admin(c));
    match cmd.as_str() {
        "w" | "whisper" => {
            // The text keeps its own spacing: take it from the line.
            let text = rest
                .split_once(char::is_whitespace)
                .and_then(|(_, r)| r.trim_start().split_once(char::is_whitespace))
                .map(|(_, t)| t.trim());
            match (name(0), text.and_then(|t| ChatText::try_from(t).ok())) {
                (Some(to), Some(text)) if !text.is_empty() => Typed::Send(ClientMsg::Chat {
                    channel: ChatChannel::Whisper(to),
                    text,
                }),
                _ => Typed::Usage(USAGE_WHISPER),
            }
        }
        "report" => match (name(0), args.len()) {
            (Some(name), 1) => Typed::Send(ClientMsg::ReportChat { name }),
            _ => Typed::Usage(USAGE_REPORT),
        },
        "kick" => match (name(0), args.len()) {
            (Some(name), 1) => admin(AdminCommand::Kick { name }),
            _ => Typed::Usage(USAGE_KICK),
        },
        "who" => match (name(0), args.len()) {
            (Some(name), 1) => admin(AdminCommand::Who { name }),
            _ => Typed::Usage(USAGE_WHO),
        },
        "replay" if args.is_empty() => admin(AdminCommand::Replay),
        // Announced restart; `cancel` calls it off (0).
        "maintenance" => match (args.first(), args.len()) {
            (Some(a), 1) if a.eq_ignore_ascii_case("cancel") => admin(AdminCommand::Maintenance { secs: 0 }),
            (Some(d), 1) => match duration(d).filter(|s| (1..=24 * 60 * 60).contains(s)) {
                Some(secs) => admin(AdminCommand::Maintenance { secs }),
                None => Typed::Usage(USAGE_MAINTENANCE),
            },
            _ => Typed::Usage(USAGE_MAINTENANCE),
        },
        "mute" | "suspend" => {
            let secs = args.get(1).and_then(|d| duration(d));
            match (name(0), secs, reason(2), args.len()) {
                (Some(name), Some(secs), Some(reason), 3) => admin(if cmd == "mute" {
                    AdminCommand::Mute { name, secs, reason }
                } else {
                    AdminCommand::Suspend { name, secs, reason }
                }),
                _ if cmd == "mute" => Typed::Usage(USAGE_MUTE),
                _ => Typed::Usage(USAGE_SUSPEND),
            }
        }
        "ban" => match (name(0), reason(1), args.len()) {
            (Some(name), Some(reason), 2) => admin(AdminCommand::Ban { name, reason }),
            _ => Typed::Usage(USAGE_BAN),
        },
        "invite" => match (name(0), args.len()) {
            (Some(n), 1) => Typed::Social(Social::Invite(n)),
            _ => Typed::Usage(USAGE_INVITE),
        },
        "partykick" => match (name(0), args.len()) {
            (Some(n), 1) => Typed::Social(Social::PartyKick(n)),
            _ => Typed::Usage(USAGE_PARTY_KICK),
        },
        "lead" => match (name(0), args.len()) {
            (Some(n), 1) => Typed::Social(Social::PartyLead(n)),
            _ => Typed::Usage(USAGE_PARTY_LEAD),
        },
        "trade" => match (name(0), args.len()) {
            (Some(n), 1) => Typed::Social(Social::Trade(n)),
            _ => Typed::Usage(USAGE_TRADE),
        },
        "offer" => offer(&args).map_or(Typed::Usage(USAGE_OFFER), Typed::Social),
        "emote" | "e" => Typed::Emote((!args.is_empty()).then(|| args.join(" "))),
        "accept" if args.is_empty() => Typed::Social(Social::Accept),
        "decline" if args.is_empty() => Typed::Social(Social::Decline),
        "leave" if args.is_empty() => Typed::Social(Social::Leave),
        "confirm" if args.is_empty() => Typed::Social(Social::Confirm),
        "cancel" if args.is_empty() => Typed::Social(Social::Cancel),
        _ => Typed::Unknown(format!("/{cmd}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> CharacterName {
        CharacterName::try_from(s).unwrap()
    }

    #[test]
    fn social_commands() {
        assert_eq!(parse("/invite Ana"), Typed::Social(Social::Invite(n("Ana"))));
        assert_eq!(parse("/accept"), Typed::Social(Social::Accept));
        assert_eq!(parse("/invite"), Typed::Usage(USAGE_INVITE));
        assert_eq!(
            parse("/offer 0 3 1 1 coins 200"),
            Typed::Social(Social::Offer {
                items: vec![(0, 3), (1, 1)],
                coins: 200
            })
        );
        assert_eq!(
            parse("/offer coins 50"),
            Typed::Social(Social::Offer {
                items: vec![],
                coins: 50
            })
        );
        assert_eq!(parse("/offer 0"), Typed::Usage(USAGE_OFFER));
        assert_eq!(parse("/offer 0 x"), Typed::Usage(USAGE_OFFER));
    }

    #[test]
    fn emote_commands() {
        assert_eq!(parse("/emote wave"), Typed::Emote(Some("wave".into())));
        assert_eq!(parse("/e Big Wave"), Typed::Emote(Some("Big Wave".into())));
        assert_eq!(parse("/emote"), Typed::Emote(None));
    }

    #[test]
    fn durations() {
        assert_eq!(duration("30m"), Some(1800));
        assert_eq!(duration("1h"), Some(3600));
        assert_eq!(duration("3d"), Some(259_200));
        assert_eq!(duration("45"), Some(45));
        assert_eq!(duration("0h"), None);
        assert_eq!(duration("2y"), None);
        assert_eq!(duration("999d"), None);
        assert_eq!(duration(""), None);
    }

    #[test]
    fn lines_become_messages() {
        let chat = |t: &str| ClientMsg::Chat {
            channel: ChatChannel::Local,
            text: ChatText::try_from(t).unwrap(),
        };
        assert_eq!(parse("  olá, 世界 "), Typed::Send(chat("olá, 世界")));
        assert_eq!(
            parse("/w Ana  oi,  tudo bem?"),
            Typed::Send(ClientMsg::Chat {
                channel: ChatChannel::Whisper(n("Ana")),
                text: ChatText::try_from("oi,  tudo bem?").unwrap(),
            })
        );
        assert_eq!(parse("/w Ana"), Typed::Usage(USAGE_WHISPER));
        assert_eq!(
            parse("/MUTE Kai 1h spam"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Mute {
                name: n("Kai"),
                secs: 3600,
                reason: ReasonId::try_from("spam").unwrap(),
            }))
        );
        assert_eq!(parse("/mute Kai forever spam"), Typed::Usage(USAGE_MUTE));
        assert_eq!(parse("/suspend Kai 3d"), Typed::Usage(USAGE_SUSPEND));
        assert_eq!(
            parse("/ban Kai cheating"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Ban {
                name: n("Kai"),
                reason: ReasonId::try_from("cheating").unwrap(),
            }))
        );
        assert_eq!(
            parse("/replay"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Replay))
        );
        assert_eq!(
            parse("/who Kai"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Who { name: n("Kai") }))
        );
        assert_eq!(
            parse("/kick Kai"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Kick { name: n("Kai") }))
        );
        assert_eq!(parse("/kick K"), Typed::Usage(USAGE_KICK));
        assert_eq!(
            parse("/maintenance 10m"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Maintenance { secs: 600 }))
        );
        assert_eq!(
            parse("/maintenance cancel"),
            Typed::Send(ClientMsg::Admin(AdminCommand::Maintenance { secs: 0 }))
        );
        assert_eq!(parse("/maintenance"), Typed::Usage(USAGE_MAINTENANCE));
        assert_eq!(parse("/maintenance 2d"), Typed::Usage(USAGE_MAINTENANCE));
        assert_eq!(
            parse("/report Kai"),
            Typed::Send(ClientMsg::ReportChat { name: n("Kai") })
        );
        assert_eq!(parse("/dance"), Typed::Unknown("/dance".into()));
    }
}

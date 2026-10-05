//! Sanctions and audit rows written while the server runs: one poll per
//! batch save, and what each new row does to the connections.

use super::account::sanction_error;
use super::*;

impl Gateway {
    /// Reads sanctions and admin actions created or revoked since the last
    /// batch save, so a ban or rename reaches players who are already online
    /// without a restart ([moderação]).
    pub(super) fn poll_admin_changes(&mut self) {
        let since = self.last_poll;
        self.last_poll = login::now_unix();
        let (repo, done) = (self.repo.clone(), self.done_tx.clone());
        tokio::spawn(async move {
            let sanctions = match repo.sanctions_changed_since(since).await {
                Ok(v) => v,
                Err(e) => {
                    error!(error = %e, "cannot read the sanctions");
                    return;
                }
            };
            let admin = match repo.admin_log_since(since).await {
                Ok(v) => v,
                Err(e) => {
                    error!(error = %e, "cannot read the admin log");
                    return;
                }
            };
            if !sanctions.is_empty() || !admin.is_empty() {
                let _ = done.send(Done::Polled { sanctions, admin });
            }
        });
    }

    pub(super) fn on_polled(&mut self, sanctions: Vec<Sanction>, admin: Vec<AdminLogRow>) {
        let now = login::now_unix();
        // Mutes, created or revoked: read the account's longest one again.
        let muted: BTreeSet<AccountId> = sanctions
            .iter()
            .filter(|s| s.kind == SanctionKind::Mute)
            .filter_map(|s| s.account)
            .filter(|a| self.conns.values().any(|conn| conn.account == Some(*a)))
            .collect();
        for account in muted {
            let (repo, done) = (self.repo.clone(), self.done_tx.clone());
            tokio::spawn(async move {
                match repo.mute_until(account, now).await {
                    Ok(until) => drop(done.send(Done::Muted { account, until })),
                    Err(e) => error!(error = %e, "cannot read the mute"),
                }
            });
        }
        for sanction in sanctions {
            if !sanction.active_at(now) {
                continue;
            }
            // Every connection of the account or IP, in any phase: a player
            // waiting in the queue or choosing a character must not enter.
            let targets: Vec<ConnectionId> = self
                .conns
                .iter()
                .filter(|(_, conn)| match sanction.kind {
                    SanctionKind::IpBan => sanction.ip.as_deref() == Some(ip_key(conn.peer.ip()).as_str()),
                    SanctionKind::Suspend | SanctionKind::Ban => {
                        sanction.account.is_some() && sanction.account == conn.account
                    }
                    // Chat only: handled above.
                    SanctionKind::Mute => false,
                })
                .map(|(c, _)| *c)
                .collect();
            for c in targets {
                let (code, args) = sanction_error(&sanction, now, &self.keys);
                self.kick(c, code, args);
            }
        }
        for row in admin {
            if row.action == MAINTENANCE_ACTION {
                self.on_maintenance_row(&row);
                continue;
            }
            if row.action != "rename" {
                continue;
            }
            let Ok(id) = row.target.parse::<u32>() else {
                continue;
            };
            let character = CharacterId::new(id);
            let target = self
                .in_game
                .iter()
                .find(|(_, who)| who.character == character)
                .and_then(|(player, _)| self.players.get(player).copied());
            if let Some(c) = target {
                // No error: the client reconnects and sees the new name.
                self.net.disconnect(c);
            }
        }
    }
}

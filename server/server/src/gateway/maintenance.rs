//! Announced restart
//! and the hourly backup. `admin maintenance 10m` (or `/maintenance 10m` in
//! the chat) writes an `admin_log` row; the poll schedules it here. Every
//! connection gets `Maintenance { in_secs }` when it is scheduled and at
//! 10 min, 5 min, 1 min and 10 s; at zero the gateway stops and saves
//! everyone, like Ctrl+C.

use super::*;

/// Seconds before the restart when everyone is told again.
const ANNOUNCE_AT: [u32; 4] = [600, 300, 60, 10];
/// `admin_log` action written by `admin maintenance` and `/maintenance`.
pub const MAINTENANCE_ACTION: &str = "maintenance";
/// Polls without a checkpoint (an empty server still hears the admin).
const POLL_EVERY_SECS: u64 = 30;

/// A restart on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheduled {
    /// Unix second of the restart.
    pub at: u64,
    /// Index into `ANNOUNCE_AT` of the next announcement, if any is left.
    next: Option<usize>,
}

impl Scheduled {
    pub fn new(at: u64, now: u64) -> Scheduled {
        let left = at.saturating_sub(now);
        Scheduled {
            at,
            next: ANNOUNCE_AT.iter().position(|t| u64::from(*t) < left),
        }
    }

    /// Seconds left to announce now, if a threshold was crossed (several
    /// crossed at once make one announcement).
    pub fn due(&mut self, now: u64) -> Option<u32> {
        let left = self.at.saturating_sub(now);
        let mut announce = None;
        while let Some(i) = self.next {
            let at = ANNOUNCE_AT.get(i).copied().map_or(0, u64::from);
            if left > at {
                break;
            }
            announce = Some(u32::try_from(left).unwrap_or(u32::MAX));
            self.next = (i + 1 < ANNOUNCE_AT.len()).then_some(i + 1);
        }
        announce
    }
}

impl Gateway {
    /// From an `admin_log` row: `args` is the delay in seconds counted from
    /// `created_at` (the same row read twice gives the same restart), 0 to
    /// cancel.
    pub(super) fn on_maintenance_row(&mut self, row: &AdminLogRow) {
        let Ok(secs) = row.args.trim().parse::<u64>() else {
            warn!(args = %row.args, "maintenance row with bad seconds");
            return;
        };
        let now = login::now_unix();
        if secs == 0 {
            if self.maintenance.take().is_some() {
                info!(actor = %row.actor, "maintenance cancelled");
                self.broadcast(&ServerMsg::MaintenanceCancelled);
            }
            return;
        }
        let at = row.created_at.saturating_add(secs);
        if self.maintenance.is_some_and(|m| m.at == at) || at <= now {
            return;
        }
        info!(actor = %row.actor, in_secs = at - now, "maintenance scheduled");
        self.maintenance = Some(Scheduled::new(at, now));
        let in_secs = u32::try_from(at - now).unwrap_or(u32::MAX);
        self.broadcast(&ServerMsg::Maintenance { in_secs });
    }

    /// Once a second: announcements, a poll on an empty server, the hourly
    /// backup. `true` when the restart is due: the gateway stops.
    pub(super) fn maintenance_second(&mut self) -> bool {
        let now = login::now_unix();
        if now.saturating_sub(self.last_poll) >= POLL_EVERY_SECS {
            self.poll_admin_changes();
        }
        self.backup_if_due(now);
        self.heir_check_if_due(now);
        self.instances_second();
        let Some(m) = &mut self.maintenance else {
            return false;
        };
        if now >= m.at {
            info!("maintenance: stopping now");
            return true;
        }
        if let Some(in_secs) = m.due(now) {
            self.broadcast(&ServerMsg::Maintenance { in_secs });
        }
        false
    }

    /// Every connection that finished `Hello` (any screen, in the game or
    /// waiting in the queue).
    fn broadcast(&self, msg: &ServerMsg) {
        for (c, conn) in &self.conns {
            if !matches!(conn.phase, Phase::AwaitHello) {
                self.net.send_msg(*c, msg);
            }
        }
    }

    /// `VACUUM INTO` once per `backup_every`; one at a time.
    fn backup_if_due(&mut self, now: u64) {
        let Some(backup) = &mut self.backup else { return };
        if now < backup.next || backup.running.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        backup.next = now + backup.every;
        backup.running.store(true, std::sync::atomic::Ordering::Relaxed);
        let (repo, dir, running) = (self.repo.clone(), backup.dir.clone(), backup.running.clone());
        tokio::spawn(async move {
            match repo.backup(dir, now).await {
                Ok(done) => {
                    info!(file = %done.file.display(), removed = done.removed.len(), "backup written")
                }
                Err(e) => error!(error = %e, "backup failed"),
            }
            running.store(false, std::sync::atomic::Ordering::Relaxed);
        });
    }
}

/// Where and how often the gateway copies the database.
pub struct BackupPlan {
    pub dir: std::path::PathBuf,
    pub every: u64,
    next: u64,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl BackupPlan {
    /// The first copy one period after the start (a restart loop never
    /// fills the folder).
    pub fn new(dir: std::path::PathBuf, every: u64, now: u64) -> BackupPlan {
        BackupPlan {
            dir,
            every,
            next: now + every,
            running: Arc::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcements_come_at_each_threshold_once() {
        let mut m = Scheduled::new(1000 + 900, 1000);
        assert_eq!(m.due(1000), None, "the scheduling itself was announced");
        assert_eq!(m.due(1000 + 300), Some(600));
        assert_eq!(m.due(1000 + 301), None);
        assert_eq!(m.due(1000 + 600), Some(300));
        // Late tick: 1 min and 10 s crossed together, one announcement.
        assert_eq!(m.due(1000 + 895), Some(5));
        assert_eq!(m.due(1000 + 899), None);
        // Scheduled with 30 s left: only the 10 s announcement remains.
        let mut short = Scheduled::new(30, 0);
        assert_eq!(short.due(19), None);
        assert_eq!(short.due(20), Some(10));
        assert_eq!(short.due(25), None);
    }
}

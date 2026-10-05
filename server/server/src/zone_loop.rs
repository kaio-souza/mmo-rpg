//! Zone execution: ticks run as tasks in a rayon pool, started by
//! one clock thread. A late tick never causes a burst: the next one is due
//! at `max(planned, now)` (time dilation). Each tick runs in `catch_unwind`;
//! a zone that panics saves its replay, drops its players and restarts.

use crate::content::{ContentSlot, slot_get};
use crate::translation;
use crate::zone_link::{FromZone, ToZone, ZoneEnd};
use base::{PlayerId, TICKS_PER_SECOND, TilePos, ZoneId};
use protocol::ErrorCode;
use sim::{Command, Content, LoadLevel, TickInput, TickOutput, Zone};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub const TICK: Duration = Duration::from_millis(1000 / TICKS_PER_SECOND);

/// Overload thresholds on the mean tick time of the last second.
const N1_ABOVE: Duration = Duration::from_millis(35);
const N2_ABOVE: Duration = Duration::from_millis(50);
const N3_AFTER: Duration = Duration::from_secs(5);
/// Batch save interval: 30 s.
const CHECKPOINT_TICKS: u64 = 30 * TICKS_PER_SECOND;
/// Phase 10: a neighbor gets ghosts at least this often, even with nothing
/// new, so it knows this zone is up (`sim::GHOST_TIMEOUT_TICKS` is 20).
const GHOST_HEARTBEAT_TICKS: u64 = 5;

pub struct ZoneTask {
    name: String,
    zone_id: ZoneId,
    seed: u64,
    /// Where a reload finds the new content.
    slot: ContentSlot,
    zone: Zone,
    link: ZoneEnd,
    out: TickOutput,
    commands: Vec<Command>,
    /// A block of item ids was asked for and has not arrived.
    ids_requested: bool,
    players: BTreeSet<PlayerId>,
    last_pos: BTreeMap<PlayerId, TilePos>,
    durations: VecDeque<Duration>,
    load: LoadLevel,
    n2_since: Option<Instant>,
    replay_dir: PathBuf,
    /// Development aid: panic on this tick to exercise recovery.
    panic_at_tick: Option<u64>,
    stats: Stats,
    /// Coins by source since the last checkpoint (phase 7): one row per
    /// source every 30 s instead of a row per tick.
    coins: BTreeMap<sim::CoinFlow, u64>,
    /// Phase 10: the band picture last sent to each neighbor (ghosts go as
    /// differences from it).
    ghosts_sent: BTreeMap<ZoneId, Vec<sim::EntityState>>,
}

#[derive(Default)]
struct Stats {
    ticks: u32,
    total: Duration,
    /// Part of `total` spent inside `Zone::tick` (the rest: commands, encoding, sending).
    sim: Duration,
    worst: Duration,
}

pub struct ZoneSlot {
    task: Mutex<ZoneTask>,
    due: Mutex<Instant>,
    running: AtomicBool,
}

impl ZoneTask {
    pub fn new(
        slot: ContentSlot,
        zone_id: ZoneId,
        seed: u64,
        link: ZoneEnd,
        replay_dir: PathBuf,
        panic_at_tick: Option<u64>,
    ) -> Result<ZoneTask, sim::ZoneError> {
        let content = slot_get(&slot);
        let zone = Zone::new(content.clone(), zone_id, seed)?;
        let name = content
            .catalog
            .zone(zone_id)
            .map(|z| z.name.clone())
            .unwrap_or_default();
        Ok(ZoneTask {
            name,
            zone_id,
            seed,
            slot,
            zone,
            link,
            out: TickOutput::default(),
            commands: Vec::new(),
            ids_requested: false,
            players: BTreeSet::new(),
            last_pos: BTreeMap::new(),
            durations: VecDeque::new(),
            load: LoadLevel::N0,
            n2_since: None,
            replay_dir,
            panic_at_tick,
            stats: Stats::default(),
            coins: BTreeMap::new(),
            ghosts_sent: BTreeMap::new(),
        })
    }

    fn run_tick(&mut self) {
        let started = Instant::now();
        self.commands.clear();
        while let Ok(msg) = self.link.rx.try_recv() {
            let cmd = match msg {
                ToZone::Command(cmd) => cmd,
                ToZone::SaveReplay => {
                    self.save_replay();
                    continue;
                }
                ToZone::Checkpoint => {
                    // Answered even when empty: the gateway waits for it.
                    let _ = self.link.tx.send(FromZone::Checkpoint(self.zone.players()));
                    continue;
                }
                ToZone::Reload { content_hash } => {
                    self.reload(content_hash);
                    continue;
                }
            };
            if let Command::ItemIds { .. } = &cmd {
                self.ids_requested = false;
            }
            if let Command::Enter { player, .. } = &cmd
                && self.load == LoadLevel::N3
            {
                let _ = self.link.tx.send(FromZone::Refused {
                    player: *player,
                    code: ErrorCode::ZoneBusy,
                });
                continue;
            }
            self.commands.push(cmd);
        }
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let input = TickInput {
            commands: &self.commands,
            now_unix,
            load: self.load,
        };
        let panic_at = self.panic_at_tick;
        let zone = &mut self.zone;
        let out = &mut self.out;
        let result = catch_unwind(AssertUnwindSafe(|| {
            if panic_at == Some(zone.tick_count().0 + 1) {
                panic!("panic requested by --debug-panic-tick");
            }
            zone.tick(&input, out);
        }));
        let sim_took = started.elapsed();
        match result {
            Ok(()) => {
                self.publish();
                if self.zone.tick_count().0.is_multiple_of(CHECKPOINT_TICKS) {
                    self.checkpoint();
                }
            }
            Err(_) => self.recover(),
        }
        self.stats.sim += sim_took;
        self.measure(started.elapsed());
    }

    fn publish(&mut self) {
        let tx = &self.link.tx;
        let tick = self.out.tick.0 as u32;
        let mut fresh = BTreeSet::new();
        for &(player, entity, pos) in &self.out.entered {
            self.players.insert(player);
            self.last_pos.insert(player, pos);
            fresh.insert(player);
            let _ = tx.send(FromZone::Entered {
                player,
                entity,
                pos,
                tick,
            });
        }
        // Custody (rule 4) before the leaves of the same tick:
        // the saver writes in arrival order.
        for e in std::mem::take(&mut self.out.escrows) {
            let _ = tx.send(FromZone::Escrow(Box::new(e)));
        }
        for (ticket, from, parcel) in std::mem::take(&mut self.out.escrow_lost) {
            let _ = tx.send(FromZone::EscrowLost { ticket, from, parcel });
        }
        for d in std::mem::take(&mut self.out.delivered) {
            let _ = tx.send(FromZone::Delivered(Box::new(d)));
        }
        // Who stays goes with the first leaver: one transaction.
        let mut staying = if self.out.left.is_empty() {
            Vec::new()
        } else {
            self.zone.players()
        };
        for (player, sheet, pos) in &self.out.left {
            self.players.remove(player);
            self.last_pos.remove(player);
            let _ = tx.send(FromZone::Left {
                player: *player,
                sheet: Box::new(sheet.clone()),
                pos: *pos,
                staying: std::mem::take(&mut staying),
            });
        }
        // Phase 10: players across the seam go to the gateway, which hands
        // them to the next zone; ghosts go to each neighbor as differences.
        for (player, to, arrival) in std::mem::take(&mut self.out.handoffs) {
            self.players.remove(&player);
            self.last_pos.remove(&player);
            let _ = tx.send(FromZone::HandOff { player, to, arrival });
        }
        let beat = self.out.tick.0.is_multiple_of(GHOST_HEARTBEAT_TICKS);
        for (to, list) in std::mem::take(&mut self.out.border) {
            let old = self.ghosts_sent.entry(to).or_default();
            let diff = sim::GhostDiff::between(old, &list);
            if !diff.is_empty() || beat {
                let _ = tx.send(FromZone::Ghosts { to, diff });
            }
            *old = list;
        }
        for (flow, amount) in &self.out.coins {
            let total = self.coins.entry(*flow).or_insert(0);
            *total = total.saturating_add(*amount);
        }
        let mut frames = translation::frames(&self.out, &fresh);
        // After the views: a dialogue may name an NPC the player just saw.
        frames.extend(translation::dialogue_frames(&self.out));
        frames.extend(translation::player_frames(&self.out));
        let players: Vec<PlayerId> = self.players.iter().copied().collect();
        let zone = &self.zone;
        frames.extend(translation::combat_frames(&self.out, &players, &|p, e| {
            zone.knows(p, e)
        }));
        if !self.ids_requested && (self.out.need_item_ids || self.zone.item_ids_left() < sim::LOW_ITEM_IDS) {
            self.ids_requested = true;
            let _ = tx.send(FromZone::NeedItemIds);
        }
        for player in &self.out.dead {
            let secs = self.zone.content().catalog.formulas().progression.death_lock_secs;
            let hours = u32::try_from(secs.div_ceil(3600)).unwrap_or(u32::MAX);
            let _ = tx.send(FromZone::Dead {
                player: *player,
                hours,
            });
        }
        if !frames.is_empty() {
            let _ = tx.send(FromZone::Frames(frames));
        }
        if !self.out.tiles.is_empty() {
            let _ = tx.send(FromZone::Tiles(self.out.tiles.clone()));
        }
        let mut moved = Vec::new();
        for (player, last) in &mut self.last_pos {
            if let Some(pos) = self.zone.position_of(*player)
                && pos != *last
            {
                *last = pos;
                moved.push((*player, pos));
            }
        }
        if !moved.is_empty() {
            let _ = tx.send(FromZone::Moved(moved));
        }
    }

    fn checkpoint(&mut self) {
        let players = self.zone.players();
        if !players.is_empty() {
            let _ = self.link.tx.send(FromZone::Checkpoint(players));
        }
        if !self.coins.is_empty() {
            let rows: Vec<(sim::CoinFlow, u64)> = std::mem::take(&mut self.coins).into_iter().collect();
            let _ = self.link.tx.send(FromZone::Coins(rows));
        }
    }

    fn recover(&mut self) {
        error!(zone = %self.name, tick = self.zone.tick_count().0, "zone panicked: dropping players and restarting");
        self.save_replay();
        let players: Vec<PlayerId> = std::mem::take(&mut self.players).into_iter().collect();
        self.last_pos.clear();
        // The neighbors get the whole band again from the new zone.
        self.ghosts_sent.clear();
        let _ = self.link.tx.send(FromZone::Failure { players });
        match Zone::new(self.zone.content().clone(), self.zone_id, self.seed) {
            Ok(z) => self.zone = z,
            Err(e) => error!(error = %e, "zone cannot restart"),
        }
    }

    /// Switches to the content in the slot. The `Reloaded`
    /// notice goes before any frame that uses the new ids.
    fn reload(&mut self, content_hash: u64) {
        let content: Arc<Content> = slot_get(&self.slot);
        if content.catalog.content_hash() != content_hash {
            return; // a newer reload is on its way
        }
        let (old, new) = (&self.zone.content().catalog, &content.catalog);
        // Painted tiles apply now; a map that grew or moved needs a restart.
        let same_size = match (old.zone(self.zone_id), new.zone(self.zone_id)) {
            (Some(a), Some(b)) => (a.origin, a.width, a.height) == (b.origin, b.width, b.height),
            _ => false,
        };
        if !same_size {
            warn!(zone = %self.name, "the zone map changed size or place: it applies after a restart");
        }
        let mut out = TickOutput::default();
        match self.zone.reload(content, &mut out) {
            Ok(()) => {
                info!(zone = %self.name, hash = %format!("{content_hash:016x}"), "content reloaded");
                let _ = self.link.tx.send(FromZone::Reloaded { content_hash });
                let frames = translation::dialogue_frames(&out);
                if !frames.is_empty() {
                    let _ = self.link.tx.send(FromZone::Frames(frames));
                }
            }
            Err(e) => warn!(zone = %self.name, "content not reloaded: {e}"),
        }
    }

    fn save_replay(&self) {
        let log = self.zone.replay_log();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = self.replay_dir.join(format!("zone-{}-{stamp}.replay", self.name));
        let written = log.to_bytes().map_err(|e| e.to_string()).and_then(|b| {
            std::fs::create_dir_all(&self.replay_dir).map_err(|e| e.to_string())?;
            std::fs::write(&path, b).map_err(|e| e.to_string())
        });
        match written {
            Ok(()) => info!(path = %path.display(), ticks = log.len(), "replay saved"),
            Err(e) => error!(error = %e, "cannot save replay"),
        }
    }

    fn measure(&mut self, took: Duration) {
        self.durations.push_back(took);
        if self.durations.len() > TICKS_PER_SECOND as usize {
            self.durations.pop_front();
        }
        let mean = self.durations.iter().sum::<Duration>() / self.durations.len().max(1) as u32;
        let level = if mean > N2_ABOVE {
            let since = *self.n2_since.get_or_insert_with(Instant::now);
            if since.elapsed() > N3_AFTER {
                LoadLevel::N3
            } else {
                LoadLevel::N2
            }
        } else {
            self.n2_since = None;
            if mean > N1_ABOVE {
                LoadLevel::N1
            } else {
                LoadLevel::N0
            }
        };
        if level != self.load {
            warn!(zone = %self.name, ?level, mean_us = mean.as_micros() as u64, "zone load level changed");
            self.load = level;
            let _ = self.link.tx.send(FromZone::Load(level));
        }
        self.stats.ticks += 1;
        self.stats.total += took;
        self.stats.worst = self.stats.worst.max(took);
        if self.stats.ticks == 5 * TICKS_PER_SECOND as u32 {
            info!(
                zone = %self.name,
                players = self.zone.player_count(),
                mean_us = (self.stats.total / self.stats.ticks).as_micros() as u64,
                sim_us = (self.stats.sim / self.stats.ticks).as_micros() as u64,
                worst_us = self.stats.worst.as_micros() as u64,
                "tick stats (5 s)"
            );
            self.stats = Stats::default();
        }
    }
}

/// Starts the clock thread. Zone `i` of `n` is offset by `i * TICK / n`.
pub fn start(zones: Vec<ZoneTask>, pool: Arc<rayon::ThreadPool>) -> std::thread::JoinHandle<()> {
    let now = Instant::now();
    let n = zones.len().max(1) as u32;
    let slots: Vec<Arc<ZoneSlot>> = zones
        .into_iter()
        .enumerate()
        .map(|(i, task)| {
            Arc::new(ZoneSlot {
                task: Mutex::new(task),
                due: Mutex::new(now + TICK * i as u32 / n),
                running: AtomicBool::new(false),
            })
        })
        .collect();
    std::thread::Builder::new()
        .name("zone-clock".into())
        .spawn(move || clock(slots, pool))
        .unwrap_or_else(|e| panic!("cannot start the zone clock thread: {e}"))
}

fn clock(slots: Vec<Arc<ZoneSlot>>, pool: Arc<rayon::ThreadPool>) {
    loop {
        let now = Instant::now();
        let mut next_wake = now + TICK;
        for slot in &slots {
            let due = *slot.due.lock().unwrap_or_else(PoisonError::into_inner);
            if due <= now {
                if slot
                    .running
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    let slot = slot.clone();
                    pool.spawn(move || {
                        slot.task
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .run_tick();
                        let mut due = slot.due.lock().unwrap_or_else(PoisonError::into_inner);
                        *due = (*due + TICK).max(Instant::now());
                        slot.running.store(false, Ordering::Release);
                    });
                }
            } else {
                next_wake = next_wake.min(due);
            }
        }
        let sleep = next_wake.saturating_duration_since(Instant::now());
        std::thread::sleep(sleep.max(Duration::from_micros(500)));
    }
}

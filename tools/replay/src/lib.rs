//! `replay` as a library: opens a zone's replay log against the content and
//! runs it tick by tick. The `replay` binary checks the state hashes; the
//! editor's Replays screen also reads every entity after each tick to draw
//! the zone.

use sim::{EntityKind, EntityLabel, EntityState, ReplayLog, TickInput, TickOutput, TickRecord, Zone};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What a replay file says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub zone: String,
    pub seed: u64,
    pub content_hash: u64,
    /// The tick of the base state (the first record is the one after it).
    pub first_tick: u64,
    pub records: usize,
}

/// Reads only the header of a replay file (no content needed).
pub fn info(bytes: &[u8]) -> Result<Info, String> {
    let log = ReplayLog::from_bytes(bytes).map_err(|e| format!("not a replay file: {e}"))?;
    Ok(info_of(&log))
}

fn info_of(log: &ReplayLog) -> Info {
    Info {
        zone: log.zone.clone(),
        seed: log.seed,
        content_hash: log.content_hash,
        first_tick: log.base().map_or(0, |b| b.tick.0),
        records: log.len(),
    }
}

/// One tick run again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub tick: u64,
    pub took: Duration,
    /// `Some((got, recorded))` when the state hash differs: the simulation
    /// diverged here and the rest of the log means nothing.
    pub diverged: Option<(u64, u64)>,
}

/// A zone being replayed.
pub struct Replay {
    zone: Zone,
    records: Vec<TickRecord>,
    next: usize,
    out: TickOutput,
    pub info: Info,
}

impl Replay {
    /// Loads and validates `content`, checks it is the version the replay
    /// was recorded with, and rebuilds the zone from the base state.
    pub fn open(bytes: &[u8], content: &Path) -> Result<Replay, String> {
        let log = ReplayLog::from_bytes(bytes).map_err(|e| format!("not a replay file: {e}"))?;
        let catalog = defs::load(content)
            .map_err(|e| e.to_string())?
            .validate()
            .map_err(|p| format!("content has {} problem(s); run validate", p.len()))?;
        if catalog.content_hash() != log.content_hash {
            return Err(format!(
                "content hash {:016x} differs from the replay's {:016x}: use the same content version",
                catalog.content_hash(),
                log.content_hash
            ));
        }
        let (zone_id, _) = catalog
            .zone_by_name(&log.zone)
            .ok_or_else(|| format!("zone `{}` not in the content", log.zone))?;
        let base = log.base().ok_or("replay has no base state")?;
        let zone = Zone::from_snapshot(Arc::new(sim::Content::new(catalog)), zone_id, log.seed, base)
            .map_err(|e| e.to_string())?;
        Ok(Replay {
            zone,
            records: log.records().cloned().collect(),
            next: 0,
            out: TickOutput::default(),
            info: info_of(&log),
        })
    }

    pub fn zone(&self) -> &Zone {
        &self.zone
    }

    /// The commands of the next tick to run, if any.
    pub fn next_record(&self) -> Option<&TickRecord> {
        self.records.get(self.next)
    }

    /// Runs the next record; `None` at the end of the log.
    pub fn step(&mut self) -> Option<Step> {
        let rec = self.records.get(self.next)?;
        self.next += 1;
        let input = TickInput {
            commands: &rec.commands,
            now_unix: rec.now_unix,
            load: rec.load,
        };
        let start = Instant::now();
        self.zone.tick(&input, &mut self.out);
        let took = start.elapsed();
        let hash = self.zone.state_hash();
        Some(Step {
            tick: rec.tick.0,
            took,
            diverged: (hash != rec.state_hash).then_some((hash, rec.state_hash)),
        })
    }

    /// Every entity right now, for a viewer.
    pub fn entities(&self) -> Vec<EntityState> {
        self.zone.entity_states()
    }

    /// The text key (or the player name) an entity is shown by.
    pub fn label(&self, e: &EntityState) -> String {
        let keys = self.zone.content().catalog.keys();
        match &e.label {
            EntityLabel::Player(name) => name.to_string(),
            EntityLabel::Npc(t) | EntityLabel::Monster(t) | EntityLabel::Node(t) => {
                keys.key(*t).unwrap_or("?").to_owned()
            }
        }
    }
}

/// A short name for a command: its variant (`Move`, `Attack`...).
pub fn command_name(c: &sim::Command) -> String {
    let debug = format!("{c:?}");
    debug
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .next()
        .unwrap_or("?")
        .to_owned()
}

/// `player`, `npc`, `monster`, `node` or `companion`.
pub fn kind_name(k: EntityKind) -> &'static str {
    match k {
        EntityKind::Player => "player",
        EntityKind::Npc => "npc",
        EntityKind::Monster => "monster",
        EntityKind::Node => "node",
        EntityKind::Companion => "companion",
    }
}

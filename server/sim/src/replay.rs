//! Replay log: every tick's input, plus a snapshot of the state
//! at the start of the kept records. Keeps between 5 and 10 minutes.

use crate::craft::NodeComp;
use crate::fight::{Fighter, MonsterComp};
use crate::shop::{ShopComp, ShopOpen};
use crate::types::{Command, LoadLevel};
use crate::zone::{Body, DialogueComp, NpcComp, PlayerComp, Walk};
use base::Tick;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Records between checkpoints: 5 minutes at 20 Hz.
pub const SEGMENT_TICKS: usize = 6000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickRecord {
    pub tick: Tick,
    pub now_unix: u64,
    pub load: LoadLevel,
    pub commands: Vec<Command>,
    /// `Zone::state_hash` after this tick.
    pub state_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub(crate) body: Body,
    pub(crate) walk: Walk,
    pub(crate) player: Option<PlayerComp>,
    pub(crate) npc: Option<NpcComp>,
    /// Dialogue open by a player (`None` when closed).
    pub(crate) dialogue: Option<DialogueComp>,
    pub(crate) fighter: Option<Fighter>,
    pub(crate) monster: Option<MonsterComp>,
    /// `Gear` as postcard bytes: items have no `Clone`, the
    /// snapshot keeps their record.
    pub(crate) gear: Option<Vec<u8>>,
    /// Phase 7: the NPC's shop and the shop a player has open.
    pub(crate) shop: Option<ShopComp>,
    pub(crate) shop_open: Option<ShopOpen>,
    /// Phase 8: a resource node and who emptied it.
    pub(crate) node: Option<NodeComp>,
    /// A player's companion.
    #[serde(default)]
    pub(crate) companion: Option<crate::companion::CompanionComp>,
}

/// Complete zone state, entities sorted by id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneState {
    pub tick: Tick,
    pub(crate) next_id: u32,
    pub(crate) entities: Vec<EntitySnapshot>,
    pub(crate) rng: u64,
    pub(crate) respawns: Vec<(u64, u16)>,
    /// `(next, end)` of the item id block.
    pub(crate) item_ids: Option<(u64, u64)>,
    /// Phase 9: groups, invites and trades.
    pub(crate) social: crate::social::Social,
    /// Phase 10: the neighbors' ghosts.
    #[serde(default)]
    pub(crate) ghosts: crate::seam::Ghosts,
    /// Alpha: the last tick's unix second, so a replay from here sees the
    /// same cooldown ends ([`crate::Zone::tick`]).
    #[serde(default)]
    pub(crate) now_unix: u64,
    /// Rule 4: deposits waiting for the database, as `postcard`
    /// bytes (the pieces are not `Clone`).
    #[serde(default)]
    pub(crate) escrows: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayLog {
    pub zone: String,
    pub content_hash: u64,
    pub seed: u64,
    /// State before `records[0]`.
    base: Option<ZoneState>,
    records: VecDeque<TickRecord>,
    /// State before `records[SEGMENT_TICKS]`, taken when the first segment fills.
    checkpoint: Option<ZoneState>,
}

impl ReplayLog {
    pub(crate) fn new(zone: String, content_hash: u64, seed: u64) -> Self {
        Self {
            zone,
            content_hash,
            seed,
            base: None,
            records: VecDeque::new(),
            checkpoint: None,
        }
    }

    pub(crate) fn start(&mut self, state: ZoneState) {
        self.base = Some(state);
        self.records.clear();
        self.checkpoint = None;
    }

    /// Adds a record. `true`: the caller must hand the current state to
    /// `set_checkpoint`.
    pub(crate) fn push(&mut self, r: TickRecord) -> bool {
        self.records.push_back(r);
        if self.records.len() == 2 * SEGMENT_TICKS
            && let Some(cp) = self.checkpoint.take()
        {
            self.base = Some(cp);
            self.records.drain(..SEGMENT_TICKS);
        }
        self.records.len() == SEGMENT_TICKS
    }

    pub(crate) fn set_checkpoint(&mut self, state: ZoneState) {
        self.checkpoint = Some(state);
    }

    pub fn base(&self) -> Option<&ZoneState> {
        self.base.as_ref()
    }

    pub fn records(&self) -> impl Iterator<Item = &TickRecord> {
        self.records.iter()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_stdvec(self)
    }

    pub fn from_bytes(b: &[u8]) -> Result<ReplayLog, postcard::Error> {
        postcard::from_bytes(b)
    }
}

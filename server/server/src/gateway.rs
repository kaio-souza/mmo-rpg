//! Gateway: connection state, handshake, accounts and sessions, entry
//! queue, chunk streaming and saving. Turns client messages into zone
//! commands and forwards zone frames. No game rules.
//!
//! Phase 10: many zones, each on its own link (in this process
//! or another). Each player is routed to the zone that owns it; a player
//! crossing the seam is carried from one zone to the next (`HandOff`), and
//! ghosts go from a zone to its neighbors through here.
//!
//! Slow work (database, argon2) runs in tasks that report back through
//! `Done`; the connection stays `Busy` meanwhile.

mod account;
mod admin;
mod chat;
mod chunks;
mod dungeon;
mod escrow;
mod feats;
mod goals;
mod instance;
mod maintenance;
mod play;
mod session;
mod unique;

use chunks::{encode_chunks, world_words};
pub use instance::InstanceSpawner;
pub(crate) use instance::instance_rules;
pub use maintenance::{BackupPlan, MAINTENANCE_ACTION};
pub(crate) use unique::unique_keys;

use crate::content::{ContentFrames, ContentSlot, Reloaded};
use crate::login::{self, Attempts, CHARACTERS_PER_ACCOUNT, PasswordPool, Registrations, Shield};
use crate::queue::EntryQueue;
use crate::saving::SaveJob;
use crate::translation;
use crate::zone_link::{FromZone, ToZone, ZoneEvent};
use base::{
    AccountId, AccountName, Arg, CharacterId, CharacterName, Fnv64, Locale, Look, PlayerId, TextRef, TilePos,
    ZoneId,
};
use bytes::Bytes;
use i18n::KeyTable;
use persistence::{
    AdminLogRow, CharacterRow, CharacterSave, LoadedCharacter, RepoError, Repository, Role, Sanction,
    SanctionKind, SavedSheet, ip_key,
};
use protocol::{
    CharacterSummary, ChunkTiles, ClientMsg, ErrorCode, PROTOCOL_VERSION, Password, Preference, ServerMsg,
    SessionToken,
};
use server_net::{ConnectionId, Net, NetEvent};
use sim::{CharacterSheet, Command, LoadLevel};
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use protocol::CHUNK_MARGIN;
/// A seam crossing slower than this is logged as a warning.
const CROSSING_SLOW_MS: u128 = 150;
/// Item ids reserved per request of a zone.
const ITEM_ID_BLOCK: u64 = 1024;
/// How long shutdown waits for the zone's last checkpoint.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

pub struct GatewayConfig {
    pub dev: bool,
    /// The largest view radius of the zones (chunk streaming, local chat).
    pub view_radius: u16,
    /// Where new characters enter, and characters whose zone is gone.
    pub start_zone: ZoneId,
    pub max_players: usize,
    /// What clients get about the running content: hash, text packs
    /// `Definitions`, and the text keys (reason keys of
    /// sanctions become translated text on the client).
    pub content: ContentFrames,
    /// Hands reloaded content to the zone.
    pub slot: ContentSlot,
    /// Hourly copy of the database (`None`: off, as in tests).
    pub backup: Option<BackupPlan>,
    /// Wrong passwords per minute, server wide, that turn on the proof of
    /// work; 0: always on.
    pub shield_per_min: usize,
    /// Makes instance copies in this process (`None`: no instances).
    pub spawn_instance: Option<InstanceSpawner>,
}

enum Phase {
    AwaitHello,
    AwaitLogin,
    /// A task is working for this connection; other messages are ignored.
    Busy,
    Authenticated,
    Queued,
    Entering(PlayerId),
    InZone(PlayerId),
}

struct Conn {
    peer: SocketAddr,
    phase: Phase,
    locale: Locale,
    account: Option<AccountId>,
    /// A temporary password: playing waits for `ChangePassword`.
    must_change: bool,
    /// Loaded character waiting in the queue or entering.
    pending: Option<Pending>,
    pos: TilePos,
    sent_chunks: BTreeSet<(i32, i32)>,
    queue_sent: Option<(u32, u32)>,
    /// Text packs already sent: each locale once per connection and content.
    packs_sent: BTreeSet<Locale>,
    /// From the account row at login (phase 5): admin commands, audit name.
    role: Role,
    account_name: String,
    /// End of the account's mute (unix seconds), read when entering.
    muted_until: Option<u64>,
    chat: chat::Limiter,
    /// The last `Challenge` sent: seed and bits.
    challenge: ([u8; 16], u8),
}

struct Pending {
    character: CharacterId,
    sheet: CharacterSheet,
    /// The zone that owns the saved tile (the start zone without one).
    zone: ZoneId,
    pos: Option<TilePos>,
    /// `None` for the `dev` account, which may have many characters online.
    account: Option<AccountId>,
}

/// Who a player in the zone is.
struct InGame {
    character: CharacterId,
    account: Option<AccountId>,
    name: CharacterName,
    /// The zone that has it now: its commands go there (phase 10).
    zone: ZoneId,
    /// When the seam handed it over, until the next zone takes it: the
    /// wait is logged (alpha: a hitch was felt when crossing).
    crossing: Option<Instant>,
}

/// One linked zone (phase 10).
struct ZoneConn {
    tx: mpsc::UnboundedSender<ToZone>,
    load: LoadLevel,
}

/// Results of tasks, back on the gateway task.
pub enum Done {
    Session {
        c: ConnectionId,
        account: AccountId,
        role: Role,
        account_name: String,
        token: SessionToken,
        characters: Vec<CharacterRow>,
        /// Attempt keys to clear or give back, the device token and the
        /// wrong passwords since the last login.
        after: account::AfterLogin,
        registered: bool,
        /// A temporary password is still in use.
        must_change: bool,
    },
    /// `ChangePassword` with a wrong current password (the try was
    /// already counted).
    ChangeFailed {
        c: ConnectionId,
        account: String,
        ip: String,
    },
    LoginFailed {
        c: ConnectionId,
        account: String,
        ip: String,
    },
    /// Error answer; the connection goes back to `back` (or closes).
    Failed {
        c: ConnectionId,
        code: ErrorCode,
        args: Vec<Arg>,
        back: Back,
    },
    Characters {
        c: ConnectionId,
        characters: Vec<CharacterRow>,
    },
    Loaded {
        c: ConnectionId,
        character: Box<LoadedCharacter>,
        dev: bool,
        muted_until: Option<u64>,
    },
    /// A character's last save is on disk: it may enter again.
    Released {
        account: Option<AccountId>,
        character: CharacterId,
    },
    /// Sanctions and admin actions created or revoked since the last batch.
    Polled {
        sanctions: Vec<Sanction>,
        admin: Vec<AdminLogRow>,
    },
    /// The account's mute changed (created or revoked).
    Muted { account: AccountId, until: Option<u64> },
    /// An in-game action finished: a System line for the player; `poll`
    /// applies new sanctions now.
    Reply {
        c: ConnectionId,
        key: &'static str,
        args: Vec<Arg>,
        poll: bool,
    },
    /// An in-game action failed: an error, the connection stays as it is.
    Error { c: ConnectionId, code: ErrorCode },
    /// Rule 4: a parcel taken out of custody for a character that
    /// just entered; the zone puts it in the backpack.
    EscrowTaken {
        player: PlayerId,
        character: CharacterId,
        escrow: u64,
        parcel: persistence::EscrowParcel,
    },
    /// A character is the first of the server to earn this title.
    FirstFeat { name: CharacterName, title: String },
    /// Every ranking, built from the saved sheets at `at`.
    Ranked { lists: feats::RankingLists, at: u64 },
    /// A unique title or class claimed: `holder` has it now.
    Claimed {
        player: PlayerId,
        character: CharacterId,
        key: String,
        holder: CharacterId,
    },
    /// A party is the first of the server to clear a dungeon.
    DungeonFirst { party: String, name: base::TextId },
    /// Unique holders after the heir check.
    Holders(BTreeMap<String, CharacterId>),
    /// `/firsts`: the server firsts for one connection.
    Firsts {
        c: ConnectionId,
        rows: Vec<(String, CharacterName, u64)>,
    },
}

#[derive(Clone, Copy)]
pub enum Back {
    Close,
    Login,
    Authenticated,
    /// `Enter` failed: also release the account reserved for it.
    AuthenticatedRelease(AccountId),
}

pub struct Gateway {
    cfg: GatewayConfig,
    net: Net,
    /// Linked zones (phase 10); `zone_rx` hears all of them.
    zones: BTreeMap<ZoneId, ZoneConn>,
    zone_rx: mpsc::UnboundedReceiver<ZoneEvent>,
    repo: Arc<Repository>,
    passwords: PasswordPool,
    /// Sanction reason keys → `TextId` (shared with the tasks).
    keys: Arc<KeyTable>,
    /// Reloaded content waiting for the zones' `Reloaded`.
    pending_content: Option<ContentFrames>,
    /// Zones that have not confirmed the pending content yet.
    pending_reload: BTreeSet<ZoneId>,
    /// Unix second of the last sanction/audit poll (batch save).
    last_poll: u64,
    saver: Option<mpsc::UnboundedSender<SaveJob>>,
    saver_task: JoinHandle<()>,
    done_tx: mpsc::UnboundedSender<Done>,
    done_rx: mpsc::UnboundedReceiver<Done>,
    /// Encoded `Chunk` frame per chunk of the world (every zone's map).
    chunks: BTreeMap<(i32, i32), Bytes>,
    /// The words of those chunks, patched when a tile is switched.
    chunk_words: BTreeMap<(i32, i32), Vec<u16>>,
    conns: BTreeMap<ConnectionId, Conn>,
    players: BTreeMap<PlayerId, ConnectionId>,
    /// Every player the zone has (or is about to have); the queue limit.
    in_game: BTreeMap<PlayerId, InGame>,
    online_accounts: BTreeSet<AccountId>,
    online_characters: BTreeSet<CharacterId>,
    queue: EntryQueue,
    attempts: Attempts,
    shield: Shield,
    registrations: Registrations,
    next_player: u32,
    chat_history: chat::History,
    /// Public rankings, built on demand from the saved sheets.
    rankings: feats::RankingCache,
    /// Who holds each unique title and class (`title:<id>`, `class:<id>`).
    holders: BTreeMap<String, CharacterId>,
    /// How far the server got in each goal.
    goals: goals::Goals,
    /// Instance copies in use.
    instances: instance::Instances,
    /// Dungeon lockouts running: `(character, dungeon)` → unix second.
    lockouts: BTreeMap<(CharacterId, String), u64>,
    /// Unix second of the next heir check.
    next_heir_check: u64,
    /// An announced restart.
    maintenance: Option<maintenance::Scheduled>,
    backup: Option<BackupPlan>,
    /// Parcels handed to a zone (`Command::Deliver`), by escrow id: whose
    /// they are if the zone hands them back.
    delivering: BTreeMap<u64, CharacterId>,
}

impl Gateway {
    pub fn new(
        cfg: GatewayConfig,
        net: Net,
        zone_rx: mpsc::UnboundedReceiver<ZoneEvent>,
        repo: Arc<Repository>,
        passwords: PasswordPool,
    ) -> Gateway {
        let chunk_words = world_words(&crate::content::slot_get(&cfg.slot).catalog);
        let chunks = encode_chunks(&chunk_words);
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        let (saver, saver_task) = crate::saving::start(repo.clone(), done_tx.clone());
        let keys = Arc::new(cfg.content.keys.clone());
        let mut cfg = cfg;
        let backup = cfg.backup.take();
        let shield = Shield::new(cfg.shield_per_min);
        Gateway {
            cfg,
            net,
            zones: BTreeMap::new(),
            zone_rx,
            repo,
            passwords,
            keys,
            pending_content: None,
            pending_reload: BTreeSet::new(),
            last_poll: login::now_unix(),
            saver: Some(saver),
            saver_task,
            done_tx,
            done_rx,
            chunks,
            chunk_words,
            conns: BTreeMap::new(),
            players: BTreeMap::new(),
            in_game: BTreeMap::new(),
            online_accounts: BTreeSet::new(),
            online_characters: BTreeSet::new(),
            queue: EntryQueue::default(),
            attempts: Attempts::default(),
            shield,
            registrations: Registrations::default(),
            next_player: 1,
            chat_history: chat::History::default(),
            rankings: feats::RankingCache::default(),
            holders: BTreeMap::new(),
            goals: goals::Goals::default(),
            instances: instance::Instances::default(),
            lockouts: BTreeMap::new(),
            next_heir_check: 0,
            maintenance: None,
            backup,
            delivering: BTreeMap::new(),
        }
    }

    pub async fn run(
        mut self,
        mut events: mpsc::Receiver<NetEvent>,
        mut reloads: mpsc::UnboundedReceiver<Reloaded>,
    ) {
        // The waits after wrong passwords survive a restart.
        match self.repo.login_blocks().await {
            Ok(blocks) => self.attempts = Attempts::load(blocks),
            Err(e) => warn!(error = %e, "cannot read the saved login waits"),
        }
        self.load_holders().await;
        self.load_goals().await;
        self.load_lockouts().await;
        let mut second = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                ev = events.recv() => match ev {
                    Some(ev) => self.on_net(ev),
                    None => break,
                },
                Some(ev) = self.zone_rx.recv() => self.on_zone_event(ev),
                Some(done) = self.done_rx.recv() => self.on_done(done),
                Some(r) = reloads.recv() => self.on_reload(r),
                _ = second.tick() => if self.every_second() {
                    info!("stopping for the announced restart: saving every player");
                    break;
                },
                _ = tokio::signal::ctrl_c() => {
                    info!("stopping: saving every player");
                    break;
                }
            }
        }
        self.save_login_blocks();
        self.save_goals();
        self.shutdown().await;
    }

    /// Saves the waits after wrong passwords, if they changed.
    fn save_login_blocks(&mut self) {
        if let Some(blocks) = self.attempts.take_changed() {
            let repo = self.repo.clone();
            tokio::spawn(async move {
                if let Err(e) = repo.save_login_blocks(blocks).await {
                    warn!(error = %e, "cannot save the login waits");
                }
            });
        }
    }

    /// A new seed for the connection's next `Login` or `Register`, with
    /// the bits the shield asks for now.
    fn send_challenge(&mut self, c: ConnectionId) {
        let bits = self.shield.bits(login::now_unix());
        let seed = login::new_seed();
        if let Some(conn) = self.conns.get_mut(&c) {
            conn.challenge = (seed, bits);
            self.net.send_msg(c, &ServerMsg::Challenge { seed, bits });
        }
    }

    /// Saves every player of every zone and waits until it is on disk.
    async fn shutdown(mut self) {
        let mut waiting: BTreeSet<ZoneId> = self.zones.keys().copied().collect();
        for z in self.zones.values() {
            let _ = z.tx.send(ToZone::Checkpoint);
        }
        let deadline = tokio::time::Instant::now() + SHUTDOWN_WAIT;
        while !waiting.is_empty() {
            match tokio::time::timeout_at(deadline, self.zone_rx.recv()).await {
                Ok(Some(ZoneEvent::Msg(zone, FromZone::Checkpoint(list)))) => {
                    self.save_checkpoint(list);
                    waiting.remove(&zone);
                }
                Ok(Some(ZoneEvent::Msg(
                    _,
                    FromZone::Left {
                        player,
                        sheet,
                        pos,
                        staying,
                    },
                ))) => self.on_left(player, sheet, pos, staying),
                // A player in transit is saved from its arrival sheet.
                Ok(Some(ZoneEvent::Msg(_, FromZone::HandOff { player, arrival, .. }))) => {
                    self.on_left(player, Box::new(arrival.sheet), arrival.pos, Vec::new())
                }
                Ok(Some(ZoneEvent::Down(zone))) => {
                    waiting.remove(&zone);
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    warn!(
                        zones = waiting.len(),
                        "no checkpoint from every zone: their players keep their last save"
                    );
                    break;
                }
            }
        }
        for z in self.zones.values() {
            let _ = z.tx.send(ToZone::SaveReplay);
        }
        self.saver = None; // the saver ends after the queued jobs
        if let Err(e) = self.saver_task.await {
            error!(error = %e, "saver task failed");
        }
        tokio::time::sleep(Duration::from_millis(200)).await; // replay write
        info!("stopped");
    }

    /// A command for the zone that has this player now.
    fn to_player(&self, player: PlayerId, cmd: Command) {
        if let Some(z) = self.in_game.get(&player).and_then(|w| self.zones.get(&w.zone)) {
            let _ = z.tx.send(ToZone::Command(cmd));
        }
    }

    fn to_zone(&self, zone: ZoneId, msg: ToZone) {
        if let Some(z) = self.zones.get(&zone) {
            let _ = z.tx.send(msg);
        }
    }

    /// The worst load among the zones (the entry queue waits on N3).
    fn load(&self) -> LoadLevel {
        self.zones.values().map(|z| z.load).max().unwrap_or(LoadLevel::N0)
    }

    /// The zone name for saves and logs.
    fn zone_name(&self, zone: ZoneId) -> String {
        if let Some(t) = self.template_of(zone) {
            return format!(
                "{}#{}",
                self.zone_name(t),
                zone.get().saturating_sub(instance::INSTANCE_IDS)
            );
        }
        crate::content::slot_get(&self.cfg.slot)
            .catalog
            .zone(zone)
            .map(|z| z.name.clone())
            .unwrap_or_default()
    }

    fn save(&self, job: SaveJob) {
        if let Some(saver) = &self.saver {
            let _ = saver.send(job);
        }
    }

    fn on_net(&mut self, ev: NetEvent) {
        match ev {
            NetEvent::Connected(c, peer) => {
                self.conns.insert(
                    c,
                    Conn {
                        peer,
                        phase: Phase::AwaitHello,
                        locale: Locale::SOURCE,
                        account: None,
                        must_change: false,
                        pending: None,
                        pos: TilePos::default(),
                        sent_chunks: BTreeSet::new(),
                        queue_sent: None,
                        packs_sent: BTreeSet::new(),
                        role: Role::Player,
                        account_name: String::new(),
                        muted_until: None,
                        chat: chat::Limiter::default(),
                        challenge: ([0; 16], 0),
                    },
                );
            }
            NetEvent::Disconnected(c) => self.drop_conn(c),
            NetEvent::Message(c, msg) => self.on_msg(c, msg),
        }
    }

    fn drop_conn(&mut self, c: ConnectionId) {
        let Some(conn) = self.conns.remove(&c) else { return };
        match conn.phase {
            // The zone answers with `Left`; the save then releases the character.
            Phase::Entering(player) | Phase::InZone(player) => {
                self.to_player(player, Command::Leave { player });
                self.players.remove(&player);
            }
            Phase::Queued => {
                self.queue.remove(c);
                if let Some(p) = conn.pending {
                    self.release(p.account, p.character);
                }
            }
            // A task still running for it releases what it holds when it ends.
            _ => {}
        }
    }

    fn fail(&mut self, c: ConnectionId, code: ErrorCode) {
        self.net.send_error_and_close(c, code);
        self.drop_conn(c);
    }

    fn reply_error(&self, c: ConnectionId, code: ErrorCode, args: Vec<Arg>) {
        self.net.send_msg(c, &ServerMsg::Error { code, args });
    }

    /// Sends the error and closes after it is written. The zone learns of the
    /// leave through `NetEvent::Disconnected`, so the character is saved.
    fn kick(&self, c: ConnectionId, code: ErrorCode, args: Vec<Arg>) {
        self.net.send_msg(c, &ServerMsg::Error { code, args });
        self.net.disconnect(c);
    }

    fn release(&mut self, account: Option<AccountId>, character: CharacterId) {
        if let Some(a) = account {
            self.online_accounts.remove(&a);
        }
        self.online_characters.remove(&character);
    }

    /// Dispatch: each message goes to the module that owns its part of the
    /// connection's life. `Gateway` itself only sorts them.
    fn on_msg(&mut self, c: ConnectionId, msg: ClientMsg) {
        let Some(conn) = self.conns.get(&c) else {
            return;
        };
        if let ClientMsg::Ping { n } = msg {
            self.net.send_msg(c, &ServerMsg::Pong { n });
            return;
        }
        // A task is running, or the zone has not confirmed the entry yet:
        // nothing but a ping is answered. The text pack is the exception, so
        // a client waiting in the queue can already fetch it.
        if matches!(conn.phase, Phase::Busy | Phase::Queued | Phase::Entering(_)) {
            if let ClientMsg::RequestTextPack { locale } = msg
                && !matches!(conn.phase, Phase::Busy)
            {
                self.send_text_pack(c, locale);
            }
            return;
        }
        match msg {
            ClientMsg::Hello { .. }
            | ClientMsg::DevLogin { .. }
            | ClientMsg::Register { .. }
            | ClientMsg::Login { .. }
            | ClientMsg::Resume { .. } => self.on_login_msg(c, msg),
            ClientMsg::CreateCharacter { .. }
            | ClientMsg::DeleteCharacter { .. }
            | ClientMsg::Enter { .. }
            | ClientMsg::SetPreferences(_)
            | ClientMsg::ChangePassword { .. } => self.on_account_msg(c, msg),
            ClientMsg::Chat { .. }
            | ClientMsg::ReportChat { .. }
            | ClientMsg::Admin(_)
            | ClientMsg::Ranking { .. }
            | ClientMsg::ServerFirsts => self.on_chat_msg(c, msg),
            ClientMsg::RequestTextPack { locale } => {
                if matches!(conn.phase, Phase::Authenticated | Phase::InZone(_)) {
                    self.send_text_pack(c, locale);
                } else {
                    self.fail(c, ErrorCode::InvalidMessage);
                }
            }
            _ => self.on_play_msg(c, msg),
        }
    }
    /// The world's clock (phase 10): one for every zone, from this
    /// process's wall clock; the day length is content.
    fn world_time(&self) -> ServerMsg {
        let content = crate::content::slot_get(&self.cfg.slot);
        let f = &content.catalog.formulas().progression;
        ServerMsg::WorldTime {
            secs_of_day: f.time_of_day(login::now_unix()),
            day_secs: f.day_secs.max(1),
        }
    }

    /// `true` when an announced restart is due.
    fn every_second(&mut self) -> bool {
        // Phase 10: clients correct their clock once a minute.
        if login::now_unix().is_multiple_of(60) {
            let msg = self.world_time();
            for (c, conn) in &self.conns {
                if matches!(conn.phase, Phase::InZone(_)) {
                    self.net.send_msg(*c, &msg);
                }
            }
        }
        let now = Instant::now();
        let unix = login::now_unix();
        self.attempts.forget_old(unix);
        // Saved within a second of a change: a crash loses little.
        self.save_login_blocks();
        self.save_goals();
        self.registrations.forget_old(now);
        self.pump_queue();
        self.send_queue_positions();
        self.maintenance_second()
    }
}

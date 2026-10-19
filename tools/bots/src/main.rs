//! `bots`: N connections with simple behaviors, for load tests. Measures
//! round trip time (Ping/Pong) and messages received.
//!
//! ```text
//! bots --server 127.0.0.1:7000 --count 1000 --behavior walk --seconds 60
//! bots --count 20 --behavior trade --seconds 600    # economy, phase 7
//! bots --count 200 --behavior seam --seconds 300     # two zones, phase 10
//! ```
//!
//!

use base::{CharacterName, EntityId, Locale, TilePos};
use clap::{Parser, ValueEnum};
use client_net::{Connection, ConnectionState};
use protocol::{ClientMsg, PROTOCOL_VERSION, ServerMsg};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Behavior {
    /// Stand still, only ping.
    Idle,
    /// Walk to a random tile nearby every 1–3 s.
    Walk,
    /// Phase 7: hunt what is near, then sell the loot and repair at the
    /// first NPC with a shop. Drives the economy telemetry
    /// (`admin report economy`): coins come in from monsters and go out in
    /// purchases and repairs.
    Trade,
    /// Phase 10: walk the `--route` waypoints, then go back and forth over
    /// its last leg, which crosses the seam between two zones. Counts the
    /// crossings, any second `Welcome` (a crossing must not be a new entry)
    /// and bots that stop moving.
    Seam,
}

#[derive(Parser, Debug)]
#[command(about = "Load test bots")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:7000")]
    server: String,
    #[arg(long, default_value_t = 100)]
    count: u32,
    #[arg(long, value_enum, default_value = "walk")]
    behavior: Behavior,
    /// Stop after this many seconds (0: run until killed).
    #[arg(long, default_value_t = 30)]
    seconds: u64,
    /// Delay between connections, in milliseconds.
    #[arg(long, default_value_t = 2)]
    ramp_ms: u64,
    /// Name prefix (names must be unique on the server).
    #[arg(long, default_value = "bot")]
    prefix: String,
    /// `seam` behavior: waypoints `x,y;x,y;...`. The default leads from
    /// Aurelia's circle down the road and over the seam into the field.
    #[arg(long, default_value = "31,-30;31,-5;31,8")]
    route: String,
}

fn parse_route(s: &str) -> Option<Vec<TilePos>> {
    let route: Option<Vec<TilePos>> = s
        .split(';')
        .map(|p| {
            let (x, y) = p.trim().split_once(',')?;
            Some(TilePos::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect();
    route.filter(|r| r.len() >= 2)
}

#[derive(Default)]
struct Stats {
    entered: AtomicU64,
    failed: AtomicU64,
    disconnected: AtomicU64,
    messages: AtomicU64,
    rtts_us: Mutex<Vec<u64>>,
    /// `seam`: legs of the crossing walked, second `Welcome`s, stalls.
    crossings: AtomicU64,
    rewelcomes: AtomicU64,
    stuck: AtomicU64,
    /// Bots that fell (a monster on the way) and gave up to stand again.
    falls: AtomicU64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let Some(route) = parse_route(&args.route) else {
        eprintln!("--route needs at least two waypoints `x,y;x,y`");
        return ExitCode::FAILURE;
    };
    let route = Arc::new(route);
    let stats = Arc::new(Stats::default());
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let mut handles = Vec::new();
    for i in 0..args.count {
        let (bot_stats, bot_stop) = (stats.clone(), stop.clone());
        let (server, prefix, behavior) = (args.server.clone(), args.prefix.clone(), args.behavior);
        let route = route.clone();
        let h = std::thread::Builder::new()
            .name(format!("bot-{i}"))
            .stack_size(256 * 1024)
            .spawn(move || bot(i, &server, &prefix, behavior, &route, &bot_stats, &bot_stop));
        match h {
            Ok(h) => handles.push(h),
            Err(e) => {
                eprintln!("cannot start bot {i}: {e}");
                stats.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        std::thread::sleep(Duration::from_millis(args.ramp_ms));
    }

    let mut last_msgs = 0;
    let mut last = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let msgs = stats.messages.load(Ordering::Relaxed);
        let secs = last.elapsed().as_secs_f64();
        let mut rtts = std::mem::take(&mut *stats.rtts_us.lock().unwrap_or_else(PoisonError::into_inner));
        rtts.sort_unstable();
        let pct = |p: usize| {
            rtts.get(rtts.len().saturating_sub(1) * p / 100)
                .copied()
                .unwrap_or(0) as f64
                / 1000.0
        };
        println!(
            "[{:>4}s] in zone {:>5}  failed {}  dropped {}  msgs/s {:>8.0}  rtt ms p50 {:.1} p99 {:.1} max {:.1}",
            start.elapsed().as_secs(),
            stats.entered.load(Ordering::Relaxed),
            stats.failed.load(Ordering::Relaxed),
            stats.disconnected.load(Ordering::Relaxed),
            (msgs - last_msgs) as f64 / secs,
            pct(50),
            pct(99),
            pct(100),
        );
        if args.behavior == Behavior::Seam {
            println!(
                "        crossings {}  second welcomes {}  stalls {}  falls {}",
                stats.crossings.load(Ordering::Relaxed),
                stats.rewelcomes.load(Ordering::Relaxed),
                stats.stuck.load(Ordering::Relaxed),
                stats.falls.load(Ordering::Relaxed),
            );
        }
        last_msgs = msgs;
        last = Instant::now();
        if args.seconds > 0 && start.elapsed() >= Duration::from_secs(args.seconds) {
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }
    let failed = stats.failed.load(Ordering::Relaxed)
        + stats.disconnected.load(Ordering::Relaxed)
        + stats.rewelcomes.load(Ordering::Relaxed);
    if failed > 0 {
        eprintln!("{failed} bot(s) failed or were dropped");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// Files an entity the bot saw under NPCs or monsters.
fn see(npcs: &mut Vec<(EntityId, TilePos)>, monsters: &mut Vec<(EntityId, TilePos)>, e: &protocol::Entity) {
    let list = match e.kind {
        protocol::EntityKind::Npc => npcs,
        protocol::EntityKind::Monster => monsters,
        _ => return,
    };
    match list.iter_mut().find(|(id, _)| *id == e.id) {
        Some(x) => x.1 = e.pos,
        None => list.push((e.id, e.pos)),
    }
}

fn bot(
    i: u32,
    server: &str,
    prefix: &str,
    behavior: Behavior,
    route: &[TilePos],
    stats: &Stats,
    stop: &AtomicBool,
) {
    let fail = |why: &str| {
        eprintln!("bot {i}: {why}");
        stats.failed.fetch_add(1, Ordering::Relaxed);
    };
    let conn = match Connection::connect(server, None) {
        Ok(c) => c,
        Err(e) => return fail(&format!("connect: {e}")),
    };
    let Ok(name) = CharacterName::try_from(format!("{prefix}{i}")) else {
        return fail("invalid name (3-16 chars of [A-Za-z0-9_])");
    };
    conn.send(&ClientMsg::Hello {
        version: PROTOCOL_VERSION,
        locale: Locale::EN,
    });
    conn.send(&ClientMsg::DevLogin { name });
    let deadline = Instant::now() + Duration::from_secs(15);
    let (me, mut pos): (EntityId, TilePos) = loop {
        match conn.receive_timeout(Duration::from_millis(200)) {
            Some(ServerMsg::Welcome { your_id, pos, .. }) => break (your_id, pos),
            Some(ServerMsg::Error { code, .. }) => return fail(&format!("refused: {code:?}")),
            _ if Instant::now() > deadline => return fail("no Welcome in 15 s"),
            _ => {}
        }
    };
    stats.entered.fetch_add(1, Ordering::Relaxed);

    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (u64::from(i) + 1).wrapping_mul(0x2545_f491_4f6c_dd1d));
    let epoch = Instant::now();
    let mut next_move = epoch + Duration::from_millis(rng.next(2000));
    let mut next_ping = epoch + Duration::from_millis(rng.next(1000));
    let mut ping_n = 0u32;
    let mut sent_at: Vec<(u32, Instant)> = Vec::new();
    // What the trading bot keeps: who is around, what it carries and
    // whether a shop is open.
    let mut npcs: Vec<(EntityId, TilePos)> = Vec::new();
    let mut monsters: Vec<(EntityId, TilePos)> = Vec::new();
    let mut coins = 0u64;
    let mut slots: Vec<Option<(base::ItemId, u32)>> = Vec::new();
    let mut shop: Option<(EntityId, usize)> = None;
    let mut shop_npcs: Vec<EntityId> = Vec::new();
    let mut next_trade = epoch + Duration::from_secs(5);
    // The seam walker: the waypoint it heads for, spread a little so the
    // bots do not all want the same tile, and when it last got anywhere.
    let lane = rng.next(4) as i32 - 1;
    let mut leg = 0usize;
    let mut last_arrival = epoch;
    let mut bounced = false;
    let mut next_order = epoch;
    while !stop.load(Ordering::Relaxed) {
        while let Some(msg) = conn.receive() {
            stats.messages.fetch_add(1, Ordering::Relaxed);
            match msg {
                ServerMsg::Welcome { .. } => {
                    stats.rewelcomes.fetch_add(1, Ordering::Relaxed);
                }
                ServerMsg::Pong { n } => {
                    if let Some(k) = sent_at.iter().position(|(m, _)| *m == n) {
                        let (_, t) = sent_at.swap_remove(k);
                        stats
                            .rtts_us
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(t.elapsed().as_micros() as u64);
                    }
                }
                ServerMsg::Diff(d) => {
                    if let Some((_, p, _)) = d.moved.iter().find(|(id, _, _)| *id == me) {
                        pos = *p;
                    }
                    for (id, p, _) in &d.moved {
                        for list in [&mut npcs, &mut monsters] {
                            if let Some(e) = list.iter_mut().find(|(x, _)| x == id) {
                                e.1 = *p;
                            }
                        }
                    }
                    for e in &d.entered {
                        if e.id == me {
                            pos = e.pos;
                        }
                        see(&mut npcs, &mut monsters, e);
                    }
                    for id in &d.left {
                        npcs.retain(|(x, _)| x != id);
                        monsters.retain(|(x, _)| x != id);
                    }
                }
                ServerMsg::Snapshot { entities } => {
                    if let Some(e) = entities.iter().find(|e| e.id == me) {
                        pos = e.pos;
                    }
                    for e in &entities {
                        see(&mut npcs, &mut monsters, e);
                    }
                }
                ServerMsg::Inventory {
                    coins: c, slots: s, ..
                } => {
                    coins = c;
                    slots = s.iter().map(|x| x.map(|x| (x.item, x.qty))).collect();
                }
                ServerMsg::Shops { npcs } => shop_npcs = npcs,
                ServerMsg::Shop(view) => shop = Some((view.npc, view.stock.len())),
                ServerMsg::ShopEnd => shop = None,
                // Fallen on the way (a monster; orders are ignored while
                // down): stand up at the spawn and go on.
                ServerMsg::Stats(st) if st.fallen && behavior == Behavior::Seam => {
                    stats.falls.fetch_add(1, Ordering::Relaxed);
                    conn.send(&ClientMsg::GiveUp);
                    leg = 0;
                }
                ServerMsg::Error { code, .. } => {
                    eprintln!("bot {i}: server error {code:?}");
                }
                _ => {}
            }
        }
        if conn.state() != ConnectionState::Open {
            stats.disconnected.fetch_add(1, Ordering::Relaxed);
            stats.entered.fetch_sub(1, Ordering::Relaxed);
            return;
        }
        let now = Instant::now();
        if now >= next_ping {
            ping_n += 1;
            sent_at.push((ping_n, now));
            sent_at.retain(|(_, t)| t.elapsed() < Duration::from_secs(10));
            conn.send(&ClientMsg::Ping { n: ping_n });
            next_ping = now + Duration::from_secs(1);
        }
        if behavior == Behavior::Trade && now >= next_trade {
            next_trade = now + Duration::from_millis(600);
            // A monster in reach is worth more than a trip to the shop.
            let near = monsters
                .iter()
                .filter(|(_, p)| p.distance(pos) <= 10)
                .min_by_key(|(_, p)| p.distance(pos));
            match (shop, near) {
                (None, Some((target, p))) => {
                    if p.distance(pos) > 1 {
                        conn.send(&ClientMsg::Move { destination: *p });
                    }
                    conn.send(&ClientMsg::Attack { target: *target });
                }
                // Nothing to fight: carry the loot to a shop, or go hunting.
                (None, None) => {
                    let keeper = npcs
                        .iter()
                        .filter(|(id, _)| shop_npcs.contains(id))
                        .min_by_key(|(_, p)| p.distance(pos));
                    let carrying = slots.iter().any(|s| s.is_some());
                    match keeper {
                        Some((npc, p)) if carrying || coins > 60 => {
                            if p.distance(pos) <= 2 {
                                conn.send(&ClientMsg::OpenShop { target: *npc });
                            } else {
                                conn.send(&ClientMsg::Move { destination: *p });
                            }
                        }
                        // Monsters live away from the town: roam to find them.
                        _ => {
                            let dx = rng.next(41) as i32 - 20;
                            let dy = rng.next(41) as i32 - 20;
                            conn.send(&ClientMsg::Move {
                                destination: TilePos::new(pos.x + dx, pos.y + dy),
                            });
                        }
                    }
                }
                (Some((_, lines)), _) => {
                    // One action per turn: sell a stack, repair, or buy.
                    if let Some(slot) = slots.iter().position(|s| s.is_some()) {
                        let qty = slots.get(slot).and_then(|s| *s).map_or(1, |(_, q)| q);
                        conn.send(&ClientMsg::Sell {
                            slot: slot as u16,
                            qty,
                        });
                        conn.send(&ClientMsg::Repair { slot: slot as u16 });
                    } else if coins > 60 && lines > 0 {
                        conn.send(&ClientMsg::Buy {
                            index: (rng.next(lines as u64) as u16).min(lines as u16 - 1),
                            qty: 1,
                        });
                    } else {
                        conn.send(&ClientMsg::CloseShop);
                    }
                }
            }
        }
        if behavior == Behavior::Seam {
            let Some(goal) = route.get(leg).map(|p| TilePos::new(p.x + lane, p.y)) else {
                return;
            };
            if pos.distance(goal) <= 1 {
                // From the last waypoint on, every arrival is one crossing.
                if leg + 1 == route.len() || bounced {
                    stats.crossings.fetch_add(1, Ordering::Relaxed);
                    bounced = true;
                }
                leg = if leg + 1 < route.len() {
                    leg + 1
                } else {
                    route.len() - 2
                };
                last_arrival = now;
                next_order = now;
            } else if now >= next_order {
                // Ordered again now and then: a closed seam (neighbor
                // silent) or a crowd stops the walk without a word.
                conn.send(&ClientMsg::Move { destination: goal });
                next_order = now + Duration::from_secs(3);
            }
            if now.duration_since(last_arrival) > Duration::from_secs(60) {
                stats.stuck.fetch_add(1, Ordering::Relaxed);
                eprintln!("bot {i}: no progress in 60 s at {pos:?}, heading for {goal:?}");
                last_arrival = now;
            }
        }
        if behavior == Behavior::Walk && now >= next_move {
            let dx = rng.next(21) as i32 - 10;
            let dy = rng.next(21) as i32 - 10;
            conn.send(&ClientMsg::Move {
                destination: TilePos::new(pos.x + dx, pos.y + dy),
            });
            next_move = now + Duration::from_millis(1000 + rng.next(2000));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

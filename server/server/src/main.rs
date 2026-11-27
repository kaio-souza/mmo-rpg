//! Server binary: reads config, does IO and wires the crates together.
//! No game logic lives here.
//!
//! Modes (phase 10): `all` runs the gateway and the zones in one
//! process; `gateway` runs the gateway and waits for zone processes on
//! `--link-listen`; `zone` runs zones that connect to `--gateway`. Phase 2:
//! accounts, TLS and saving; `--dev` keeps plain TCP and `DevLogin` on
//! localhost.

mod content;
mod gateway;
mod login;
mod queue;
mod saving;
mod translation;
mod zone_link;
mod zone_loop;

use clap::{Parser, ValueEnum};
use persistence::Repository;
use server_net::{Net, NetConfig, TlsFiles};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Mode {
    /// Gateway and zones in one process.
    All,
    /// Gateway only (plus `--zones`, if any); zone processes connect to `--link-listen`.
    Gateway,
    /// Zones only (`--zones`), connected to `--gateway`.
    Zone,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum LinkKind {
    /// Channels in memory.
    Memory,
    /// TCP on loopback: same as separate processes.
    Tcp,
}

#[derive(Parser, Debug)]
#[command(version, about = "Game server")]
struct Args {
    /// Development mode: plain TCP, DevLogin, localhost only.
    #[arg(long)]
    dev: bool,
    #[arg(long, default_value = "127.0.0.1:7000")]
    address: SocketAddr,
    /// Also listen for browser clients (WebSocket, same TLS) here, such as
    /// 0.0.0.0:7001.
    #[arg(long)]
    ws_address: Option<SocketAddr>,
    /// A page allowed to open the WebSocket (its origin, such as
    /// https://play.example.com); repeat for more. None: any page.
    #[arg(long = "ws-origin")]
    ws_origins: Vec<String>,
    /// Database file and generated certificate live here.
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
    /// Database (default: sqlite:<data-dir>/game.db).
    #[arg(long)]
    db: Option<String>,
    /// TLS certificate chain, PEM (default: <data-dir>/tls/cert.pem, generated if missing).
    #[arg(long, requires = "tls_key")]
    tls_cert: Option<PathBuf>,
    /// TLS private key, PEM.
    #[arg(long, requires = "tls_cert")]
    tls_key: Option<PathBuf>,
    /// Players in the game at once; more wait in the entry queue (one
    /// world holds 2000).
    #[arg(long, default_value_t = 2000)]
    max_players: usize,
    /// Open connections per IP (IPv6: per /64); 0: no limit. Loopback is
    /// never limited.
    #[arg(long, default_value_t = 8)]
    max_conns_per_ip: usize,
    /// Wrong passwords per minute, server wide, that turn on the proof of
    /// work for 10 minutes; 0: always on.
    #[arg(long, default_value_t = 30)]
    shield_per_min: usize,
    #[arg(long, default_value = "content")]
    content: PathBuf,
    /// Reload content when its files change, without a restart (default: on
    /// with --dev). `--watch-content false` turns it off.
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    watch_content: Option<bool>,
    /// A text missing in a locale shows the English one instead of
    /// stopping the load. For content being written while the
    /// translations are still coming; the CI (`validate content/`) is what
    /// keeps an untranslated build from reaching players.
    #[arg(long)]
    allow_missing_text: bool,
    #[arg(long, value_enum, default_value = "all")]
    mode: Mode,
    /// Only this zone, which is also the start zone (mode `all`; for tests
    /// and a quick local run).
    #[arg(long, conflicts_with = "zones")]
    zone: Option<String>,
    /// Zones this process runs, comma separated (default in mode `all`:
    /// every zone of the content; required in mode `zone`).
    #[arg(long, value_delimiter = ',')]
    zones: Vec<String>,
    /// Where new characters enter (default: the zone with `starting: true`).
    #[arg(long)]
    start_zone: Option<String>,
    /// Mode `gateway` (or `all`, optional): where zone processes connect.
    #[arg(long)]
    link_listen: Option<SocketAddr>,
    /// Mode `zone`: the gateway's `--link-listen` address (fixed).
    #[arg(long, default_value = "127.0.0.1:7100")]
    gateway: SocketAddr,
    /// Link of the zones in this process: in memory, or TCP on loopback.
    #[arg(long, value_enum, default_value = "memory")]
    zone_link: LinkKind,
    /// Database copies (`VACUUM INTO`) go here (default: <data-dir>/backup).
    #[arg(long)]
    backup_dir: Option<PathBuf>,
    /// Seconds between database copies (default: 3600; off with --dev
    /// unless given). 0 turns them off.
    #[arg(long)]
    backup_every_secs: Option<u64>,
    /// Where a panicking zone writes its replay.
    #[arg(long, default_value = "replays")]
    replay_dir: PathBuf,
    /// Threads of the zone pool (default: one per core).
    #[arg(long)]
    threads: Option<usize>,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Development aid: make the zone panic on this tick.
    #[arg(long, hide = true)]
    debug_panic_tick: Option<u64>,
}

fn main() -> ExitCode {
    // No colors outside a terminal: journald and fail2ban read plain lines
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Default address where a gateway waits for zone processes.
const LINK_LISTEN: &str = "127.0.0.1:7100";

fn run(args: Args) -> Result<(), String> {
    game_folder(&args.content)?;
    if args.mode == Mode::Zone {
        return run_zones(args);
    }
    if args.dev && !args.address.ip().is_loopback() {
        return Err(format!("--dev only listens on localhost, not {}", args.address));
    }
    if let Some(ws) = args.ws_address
        && args.dev
        && !ws.ip().is_loopback()
    {
        return Err(format!("--dev only listens on localhost, not {ws}"));
    }
    let tls = if args.dev {
        None
    } else {
        let files = TlsFiles {
            cert: args
                .tls_cert
                .clone()
                .unwrap_or_else(|| args.data_dir.join("tls/cert.pem")),
            key: args
                .tls_key
                .clone()
                .unwrap_or_else(|| args.data_dir.join("tls/key.pem")),
        };
        let mut names = vec!["localhost".to_owned(), "127.0.0.1".to_owned()];
        if !args.address.ip().is_unspecified() && !args.address.ip().is_loopback() {
            names.push(args.address.ip().to_string());
        }
        Some(files.load_or_generate(&names).map_err(|e| e.to_string())?)
    };
    let db_url = args
        .db
        .clone()
        .unwrap_or_else(|| format!("sqlite:{}", args.data_dir.join("game.db").display()));
    let repo = Arc::new(Repository::open(&db_url).map_err(|e| format!("{db_url}: {e}"))?);
    info!(db = %db_url, "database open");
    let content = Arc::new(content::load(&args.content, args.allow_missing_text)?);
    let frames = content::frames(&content)?;
    let catalog = &content.catalog;
    let local = local_zones(&args, catalog)?;
    let start_name = args
        .start_zone
        .clone()
        .or_else(|| args.zone.clone())
        .or_else(|| {
            catalog
                .zones()
                .find(|(_, z)| z.starting)
                .map(|(_, z)| z.name.clone())
        })
        .or_else(|| {
            catalog
                .zones()
                .find(|(_, z)| z.instance.is_none())
                .map(|(_, z)| z.name.clone())
        })
        .ok_or("the content has no zone")?;
    let (start_zone, _) = catalog
        .zone_by_name(&start_name)
        .ok_or_else(|| format!("start zone `{start_name}` not found"))?;
    let view_radius = catalog.zones().map(|(_, z)| z.view_radius).max().unwrap_or(1);
    let link_listen = match (args.mode, args.link_listen) {
        (_, Some(a)) => Some(a),
        (Mode::Gateway, None) => Some(LINK_LISTEN.parse().map_err(|_| "bad default address")?),
        _ => None,
    };
    info!(zones = ?local.iter().map(|z| catalog.zone(*z).map(|d| d.name.clone()).unwrap_or_default()).collect::<Vec<_>>(), start = %start_name, hash = %format!("{:016x}", catalog.content_hash()), "content loaded");

    // Hourly copies on a public server; off in dev unless asked.
    let every = args.backup_every_secs.unwrap_or(if args.dev { 0 } else { 3600 });
    let backup = (every > 0).then(|| {
        let dir = args
            .backup_dir
            .clone()
            .unwrap_or_else(|| args.data_dir.join("backup"));
        info!(dir = %dir.display(), every_secs = every, "hourly backup on");
        gateway::BackupPlan::new(dir, every, login::now_unix())
    });
    let pool = zone_pool(args.threads)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("net")
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let slot: content::ContentSlot = Arc::new(Mutex::new(content.clone()));
        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut tasks = Vec::new();
        for zone_id in &local {
            let (gateway_end, zone_end) = match args.zone_link {
                LinkKind::Memory => zone_link::memory(),
                LinkKind::Tcp => zone_link::tcp_loopback()
                    .await
                    .map_err(|e| format!("zone link: {e}"))?,
            };
            tasks.push(
                zone_loop::ZoneTask::new(
                    slot.clone(),
                    *zone_id,
                    zone_seed(args.seed, *zone_id),
                    zone_end,
                    args.replay_dir.clone(),
                    args.debug_panic_tick,
                )
                .map_err(|e| e.to_string())?,
            );
            zone_link::attach(*zone_id, gateway_end, &events_tx);
        }
        // Ticks run on the pool's threads, so the sim's `par_iter` uses this pool too.
        let clock = zone_loop::start(tasks, pool);
        // Instance copies run here, on the same clock, linked in memory.
        let spawn_instance: gateway::InstanceSpawner = {
            let (slot, events, replay_dir) = (slot.clone(), events_tx.clone(), args.replay_dir.clone());
            Arc::new(move |template, runtime, seed| {
                let (gateway_end, zone_end) = zone_link::memory();
                let name = content::slot_get(&slot)
                    .catalog
                    .zone(template)
                    .map(|z| format!("{}#{}", z.name, runtime.get()))
                    .unwrap_or_default();
                let task = zone_loop::ZoneTask::new(slot.clone(), template, seed, zone_end, replay_dir.clone(), None)
                    .map_err(|e| e.to_string())?
                    .named(name);
                clock.add(task);
                zone_link::attach(runtime, gateway_end, &events);
                Ok(())
            })
        };
        if let Some(addr) = link_listen {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| format!("zone link listen {addr}: {e}"))?;
            info!(%addr, "waiting for zone processes");
            let slot = slot.clone();
            let resolve = move |name: &str, hash: u64| {
                let c = content::slot_get(&slot);
                if c.catalog.content_hash() != hash {
                    return None;
                }
                c.catalog.zone_by_name(name).map(|(id, _)| id)
            };
            tokio::spawn(zone_link::listen(listener, resolve, events_tx.clone()));
        }
        drop(events_tx);

        let login_threads = std::thread::available_parallelism().map_or(2, |n| n.get());
        let passwords = login::PasswordPool::new(login_threads).await?;
        let mut net_cfg = NetConfig::new(args.address);
        net_cfg.tls = tls;
        net_cfg.max_conns_per_ip = args.max_conns_per_ip;
        net_cfg.ws_address = args.ws_address;
        net_cfg.ws_origins = args.ws_origins.clone();
        let (net, events) = Net::start(net_cfg).await.map_err(|e| e.to_string())?;
        let mode = if args.dev { "dev mode, plain TCP" } else { "TLS" };
        info!(address = %net.local_addr(), mode = ?args.mode, link = ?args.zone_link, max_players = args.max_players, "server ready ({mode})");
        let (reload_tx, reload_rx) = tokio::sync::mpsc::unbounded_channel();
        if args.watch_content.unwrap_or(args.dev) {
            content::watch(args.content.clone(), args.allow_missing_text, reload_tx);
        }
        let cfg = gateway::GatewayConfig {
            dev: args.dev,
            view_radius,
            start_zone,
            max_players: args.max_players,
            content: frames,
            slot,
            backup,
            shield_per_min: args.shield_per_min,
            spawn_instance: Some(spawn_instance),
        };
        gateway::Gateway::new(cfg, net, events_rx, repo, passwords)
            .run(events, reload_rx)
            .await;
        Ok::<(), String>(())
    })
}

/// The content folder must exist before anything is written (certificate,
/// database): run from a folder that is not a game and nothing is created.
fn game_folder(content: &std::path::Path) -> Result<(), String> {
    if content.is_dir() {
        return Ok(());
    }
    Err(format!(
        "no content folder at `{}`: run the server from your game's folder (the one with content/ and assets/), \
         or pass --content PATH",
        content.display()
    ))
}

/// Mode `zone`: the zones of `--zones`, each on its own TCP
/// link to the gateway. No database and no clients: the gateway saves.
/// The process stops when the gateway closes a link.
fn run_zones(args: Args) -> Result<(), String> {
    if args.zones.is_empty() {
        return Err("mode zone needs --zones".into());
    }
    game_folder(&args.content)?;
    let content = Arc::new(content::load(&args.content, args.allow_missing_text)?);
    let local = local_zones(&args, &content.catalog)?;
    let hash = content.catalog.content_hash();
    let pool = zone_pool(args.threads)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("link")
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let slot: content::ContentSlot = Arc::new(Mutex::new(content.clone()));
        let mut tasks = Vec::new();
        let mut watchers = Vec::new();
        for zone_id in &local {
            let name = content
                .catalog
                .zone(*zone_id)
                .map(|z| z.name.clone())
                .unwrap_or_default();
            let (end, closed) = zone_link::connect(args.gateway, &name, hash).await;
            info!(zone = %name, gateway = %args.gateway, "linked to the gateway");
            let end = reload_from_disk(end, slot.clone(), args.content.clone(), args.allow_missing_text);
            tasks.push(
                zone_loop::ZoneTask::new(
                    slot.clone(),
                    *zone_id,
                    zone_seed(args.seed, *zone_id),
                    end,
                    args.replay_dir.clone(),
                    args.debug_panic_tick,
                )
                .map_err(|e| e.to_string())?,
            );
            watchers.push(closed);
        }
        let _clock = zone_loop::start(tasks, pool);
        tokio::select! {
            _ = futures_any(watchers) => Err("the gateway closed the link: stopping".to_owned()),
            _ = tokio::signal::ctrl_c() => {
                info!("stopping: the gateway keeps the last saves");
                Ok(())
            }
        }
    })
}

/// Ends when the first of the tasks ends.
async fn futures_any(handles: Vec<tokio::task::JoinHandle<()>>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    for h in handles {
        let tx = tx.clone();
        tokio::spawn(async move {
            let _ = h.await;
            let _ = tx.send(()).await;
        });
    }
    drop(tx);
    let _ = rx.recv().await;
}

/// A zone process gets `Reload` from the gateway: it reads the same files
/// from its own disk first, so the zone finds the new content in its slot.
fn reload_from_disk(
    end: zone_link::ZoneEnd,
    slot: content::ContentSlot,
    dir: PathBuf,
    allow_missing_text: bool,
) -> zone_link::ZoneEnd {
    let zone_link::ZoneEnd { mut rx, tx } = end;
    let (inner_tx, inner_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let zone_link::ToZone::Reload { content_hash } = &msg
                && content::slot_get(&slot).catalog.content_hash() != *content_hash
            {
                let (dir, wanted, slot) = (dir.clone(), *content_hash, slot.clone());
                let loaded =
                    tokio::task::spawn_blocking(move || content::load(&dir, allow_missing_text)).await;
                match loaded {
                    Ok(Ok(c)) if c.catalog.content_hash() == wanted => {
                        *slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(c);
                    }
                    Ok(Ok(_)) => tracing::warn!("reload: the files here differ from the gateway's"),
                    Ok(Err(e)) => tracing::warn!("reload: {e}"),
                    Err(e) => tracing::warn!("reload: {e}"),
                }
            }
            if inner_tx.send(msg).is_err() {
                return;
            }
        }
    });
    zone_link::ZoneEnd { rx: inner_rx, tx }
}

/// The zones this process runs.
fn local_zones(args: &Args, catalog: &defs::ValidatedCatalog) -> Result<Vec<base::ZoneId>, String> {
    let names: Vec<String> = match (&args.zone, args.mode) {
        (Some(z), _) => vec![z.clone()],
        (None, Mode::All) if args.zones.is_empty() => catalog.zones().map(|(_, z)| z.name.clone()).collect(),
        (None, _) => args.zones.clone(),
    };
    names
        .iter()
        .map(|n| {
            catalog
                .zone_by_name(n)
                .map(|(id, _)| id)
                .ok_or_else(|| format!("zone `{n}` not found in {}", args.content.display()))
        })
        // Instances never run as one shared zone: the gateway makes copies.
        .filter(|id| !matches!(id, Ok(id) if catalog.zone(*id).is_some_and(|z| z.instance.is_some())))
        .collect()
}

/// Each zone its own seed, the same on every run.
fn zone_seed(seed: u64, zone: base::ZoneId) -> u64 {
    seed.wrapping_add(u64::from(zone.get()).wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

fn zone_pool(threads: Option<usize>) -> Result<Arc<rayon::ThreadPool>, String> {
    let mut pool = rayon::ThreadPoolBuilder::new().thread_name(|i| format!("zone-pool-{i}"));
    if let Some(n) = threads {
        pool = pool.num_threads(n);
    }
    Ok(Arc::new(pool.build().map_err(|e| e.to_string())?))
}

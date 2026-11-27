//! `ZoneLink`: how the gateway and a zone talk. The game code
//! only sees two channel ends; the link is either in memory (a zone in the
//! gateway's process) or a TCP connection (a zone in its own process, phase
//! 10,; loopback in tests and CI). A zone process opens one
//! connection per zone and says which zone it runs with `FromZone::Hello`.

use base::{EntityId, PlayerId, TilePos, ZoneId};
use bytes::Bytes;
use protocol::ErrorCode;
use serde::{Deserialize, Serialize};
use sim::{CharacterSheet, Command, LoadLevel};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToZone {
    Command(Command),
    /// Administration: write the replay log to disk now.
    SaveReplay,
    /// Send a `Checkpoint` now (server shutting down).
    Checkpoint,
    /// Switch to the content with this hash (from the process's content
    /// slot).
    Reload {
        content_hash: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FromZone {
    Entered {
        player: PlayerId,
        entity: EntityId,
        pos: TilePos,
        tick: u32,
    },
    /// The player is out of the zone; sheet and last position to save.
    /// `staying`: every player still in the zone at the end of the same
    /// tick (only on the first `Left` of a tick), saved in the same
    /// transaction, so an item just handed over is never refused on one
    /// side and dropped on the other.
    Left {
        player: PlayerId,
        sheet: Box<CharacterSheet>,
        pos: TilePos,
        staying: Vec<(PlayerId, CharacterSheet, TilePos)>,
    },
    /// Every player of the zone at the end of the same tick: one batch save.
    Checkpoint(Vec<(PlayerId, CharacterSheet, TilePos)>),
    /// Encoded frames, ready to write.
    Frames(Vec<(PlayerId, Bytes)>),
    /// Players whose own tile changed this tick (chunk streaming).
    Moved(Vec<(PlayerId, TilePos)>),
    /// Phase 11: tiles a player switched, with their new word.
    Tiles(Vec<(TilePos, u16)>),
    Load(LoadLevel),
    Refused {
        player: PlayerId,
        code: ErrorCode,
    },
    /// The zone panicked and restarted; these players were dropped.
    Failure {
        players: Vec<PlayerId>,
    },
    /// The zone runs the content with this hash now; frames after this use
    /// its ids.
    Reloaded {
        content_hash: u64,
    },
    /// The zone needs a new block of item ids.
    NeedItemIds,
    /// The character died for good: it leaves and rests.
    Dead {
        player: PlayerId,
        hours: u32,
    },
    /// Coins created and destroyed since the last checkpoint, summed by
    /// source (phase 7): the economy telemetry
    Coins(Vec<(sim::CoinFlow, u64)>),
    /// Phase 10: first message of a zone process on its connection: which
    /// zone this link runs, on which content (same hash as the gateway).
    Hello {
        zone: String,
        content_hash: u64,
    },
    /// Phase 10: a player crossed the seam; the gateway hands it to `to`
    /// (`Command::Arrive`) and routes its commands there from now on.
    HandOff {
        player: PlayerId,
        to: ZoneId,
        arrival: Box<sim::Arrival>,
    },
    /// Phase 10: ghosts for the neighbor `to` (`Command::Ghosts`).
    Ghosts {
        to: ZoneId,
        diff: sim::GhostDiff,
    },
    /// Rule 4: a deposit to write (sheet and parcel together);
    /// the zone waits for `Command::EscrowDone`.
    Escrow(Box<sim::EscrowOut>),
    /// A refused deposit the zone could not give back: into custody for
    /// its sender.
    EscrowLost {
        ticket: u64,
        from: base::CharacterId,
        parcel: sim::EscrowParcel,
    },
    /// Result of a `Command::Deliver`.
    Delivered(Box<sim::DeliveredOut>),
    /// Titles earned that the server may hear of, by title id.
    Feats(Vec<(PlayerId, String, defs::Announce)>),
    /// Unique titles and classes players just got (`title:<id>`,
    /// `class:<id>`): the gateway claims them.
    Claims(Vec<(PlayerId, String)>),
    /// Dungeon runs cleared.
    Clears(Vec<sim::ClearOut>),
    /// Unique items that left the game (`item:<id>`).
    Released(Vec<String>),
}

pub struct GatewayEnd {
    pub tx: mpsc::UnboundedSender<ToZone>,
    pub rx: mpsc::UnboundedReceiver<FromZone>,
}

pub struct ZoneEnd {
    pub rx: mpsc::UnboundedReceiver<ToZone>,
    pub tx: mpsc::UnboundedSender<FromZone>,
}

/// What the gateway hears from all its zones, on one channel (phase 10).
pub enum ZoneEvent {
    /// A zone is linked: commands for it go to `tx`.
    Up {
        zone: ZoneId,
        tx: mpsc::UnboundedSender<ToZone>,
    },
    Msg(ZoneId, FromZone),
    /// The link closed (process gone, or the gateway dropped it).
    Down(ZoneId),
}

/// Hands one link to the gateway: `Up`, then every message tagged with the
/// zone, then `Down` when it closes.
pub fn attach(zone: ZoneId, end: GatewayEnd, events: &mpsc::UnboundedSender<ZoneEvent>) {
    let GatewayEnd { tx, mut rx } = end;
    if events.send(ZoneEvent::Up { zone, tx }).is_err() {
        return;
    }
    let events = events.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if events.send(ZoneEvent::Msg(zone, msg)).is_err() {
                return;
            }
        }
        let _ = events.send(ZoneEvent::Down(zone));
    });
}

/// The gateway's door for zone processes: every connection
/// starts with `FromZone::Hello`; `resolve` turns its zone name and content
/// hash into the zone id, or refuses (unknown zone, other content).
pub async fn listen(
    listener: TcpListener,
    resolve: impl Fn(&str, u64) -> Option<ZoneId> + Send + Sync + 'static,
    events: mpsc::UnboundedSender<ZoneEvent>,
) {
    let resolve = std::sync::Arc::new(resolve);
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                warn!(error = %e, "zone link accept");
                continue;
            }
        };
        let (resolve, events) = (resolve.clone(), events.clone());
        tokio::spawn(async move {
            let mut end = gateway_end(stream);
            let hello = tokio::time::timeout(std::time::Duration::from_secs(5), end.rx.recv()).await;
            let Ok(Some(FromZone::Hello { zone, content_hash })) = hello else {
                warn!(%peer, "zone link without hello: closed");
                return;
            };
            match resolve(&zone, content_hash) {
                Some(id) => {
                    tracing::info!(%peer, %zone, "zone process linked");
                    attach(id, end, &events);
                }
                None => {
                    warn!(%peer, %zone, hash = %format!("{content_hash:016x}"), "zone refused: unknown zone or other content")
                }
            }
        });
    }
}

pub fn memory() -> (GatewayEnd, ZoneEnd) {
    let (to_zone_tx, to_zone_rx) = mpsc::unbounded_channel();
    let (from_zone_tx, from_zone_rx) = mpsc::unbounded_channel();
    (
        GatewayEnd {
            tx: to_zone_tx,
            rx: from_zone_rx,
        },
        ZoneEnd {
            rx: to_zone_rx,
            tx: from_zone_tx,
        },
    )
}

/// The same two ends, but every message crosses a TCP connection on
/// loopback (postcard frames). Proves the zone can live in another process.
pub async fn tcp_loopback() -> std::io::Result<(GatewayEnd, ZoneEnd)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (zone_side, gateway_side) = tokio::join!(TcpStream::connect(addr), listener.accept());
    let (gateway_side, _) = gateway_side?;
    Ok((gateway_end(gateway_side), zone_end(zone_side?).0))
}

/// The gateway's end of a TCP link: what the gateway sends goes to the
/// socket, what arrives from the socket goes to the gateway.
pub fn gateway_end(stream: TcpStream) -> GatewayEnd {
    let _ = stream.set_nodelay(true);
    let (tx, from_gateway) = mpsc::unbounded_channel::<ToZone>();
    let (to_gateway, rx) = mpsc::unbounded_channel::<FromZone>();
    let (r, w) = stream.into_split();
    tokio::spawn(pump_out(from_gateway, w));
    tokio::spawn(pump_in(r, to_gateway));
    GatewayEnd { tx, rx }
}

/// The zone's end of a TCP link, and a task that ends when the connection
/// does (a zone process stops when its gateway is gone).
pub fn zone_end(stream: TcpStream) -> (ZoneEnd, tokio::task::JoinHandle<()>) {
    let _ = stream.set_nodelay(true);
    let (r, w) = stream.into_split();
    let (to_zone, rx) = mpsc::unbounded_channel::<ToZone>();
    let (tx, from_zone) = mpsc::unbounded_channel::<FromZone>();
    let closed = tokio::spawn(pump_in(r, to_zone));
    tokio::spawn(pump_out(from_zone, w));
    (ZoneEnd { rx, tx }, closed)
}

/// A zone process connecting to its gateway (fixed address):
/// tries every second until the gateway answers, then says hello.
pub async fn connect(
    gateway: std::net::SocketAddr,
    zone: &str,
    content_hash: u64,
) -> (ZoneEnd, tokio::task::JoinHandle<()>) {
    loop {
        match TcpStream::connect(gateway).await {
            Ok(stream) => {
                let (end, closed) = zone_end(stream);
                let _ = end.tx.send(FromZone::Hello {
                    zone: zone.to_owned(),
                    content_hash,
                });
                return (end, closed);
            }
            Err(e) => {
                warn!(%gateway, error = %e, "gateway not reachable: trying again in 1 s");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

async fn pump_out<T: Serialize>(mut rx: mpsc::UnboundedReceiver<T>, mut w: tokio::net::tcp::OwnedWriteHalf) {
    let mut buf = Vec::new();
    while let Some(msg) = rx.recv().await {
        buf.clear();
        if let Err(e) = protocol::encode(&msg, &mut buf) {
            warn!(error = %e, "zone link encode");
            continue;
        }
        if w.write_all(&buf).await.is_err() {
            return;
        }
    }
}

async fn pump_in<T: serde::de::DeserializeOwned>(
    mut r: tokio::net::tcp::OwnedReadHalf,
    tx: mpsc::UnboundedSender<T>,
) {
    /// Zone links are internal; allow big frames (a full tick of output).
    const MAX: usize = 64 * 1024 * 1024;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = match r.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
        let mut used = 0;
        loop {
            match protocol::decode_frame::<T>(buf.get(used..).unwrap_or_default(), MAX) {
                Ok(Some((msg, len))) => {
                    used += len;
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    warn!(error = %e, "zone link decode: closing");
                    return;
                }
            }
        }
        buf.drain(..used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base::PlayerId;

    async fn round_trip(mut g: GatewayEnd, mut z: ZoneEnd) {
        let cmd = ToZone::Command(Command::Leave {
            player: PlayerId::new(7),
        });
        g.tx.send(cmd.clone()).unwrap();
        g.tx.send(ToZone::SaveReplay).unwrap();
        assert_eq!(z.rx.recv().await, Some(cmd));
        assert_eq!(z.rx.recv().await, Some(ToZone::SaveReplay));
        let frames = FromZone::Frames(vec![(PlayerId::new(7), Bytes::from_static(b"abc"))]);
        z.tx.send(frames.clone()).unwrap();
        z.tx.send(FromZone::Load(LoadLevel::N2)).unwrap();
        assert_eq!(g.rx.recv().await, Some(frames));
        assert_eq!(g.rx.recv().await, Some(FromZone::Load(LoadLevel::N2)));
    }

    #[tokio::test]
    async fn memory_link_delivers_in_order() {
        let (g, z) = memory();
        round_trip(g, z).await;
    }

    #[tokio::test]
    async fn tcp_loopback_link_delivers_in_order() {
        let (g, z) = tcp_loopback().await.unwrap();
        round_trip(g, z).await;
    }

    #[tokio::test]
    async fn a_zone_process_says_hello_first() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (zone, accepted) = tokio::join!(connect(addr, "dev_field", 7), listener.accept());
        let mut g = gateway_end(accepted.unwrap().0);
        assert_eq!(
            g.rx.recv().await,
            Some(FromZone::Hello {
                zone: "dev_field".into(),
                content_hash: 7
            })
        );
        drop(g);
        // The gateway is gone: the zone's watcher ends.
        let (_end, closed) = zone;
        tokio::time::timeout(std::time::Duration::from_secs(2), closed)
            .await
            .unwrap()
            .unwrap();
    }
}

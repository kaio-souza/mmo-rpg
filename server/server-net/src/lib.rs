//! Server connections: one task per connection reads frames, decodes them
//! with `protocol` and queues them; writing uses shared, already encoded
//! `Bytes`. Limits: frame size, messages per second, output queue (a client
//! too slow to read is dropped instead of stalling the server).
//!
//! TLS with rustls (ring);
//! plain TCP only in dev mode (localhost, checked by the binary).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod tls;
mod ws;

pub use tls::{TlsFiles, server_config};

use bytes::Bytes;
use protocol::{ClientMsg, ErrorCode, MAX_CLIENT_FRAME, ProtocolError, ServerMsg};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(pub u64);

/// Time a client has to finish the TLS handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct NetConfig {
    pub address: SocketAddr,
    /// `None`: plain TCP (dev mode only).
    pub tls: Option<Arc<rustls::ServerConfig>>,
    pub max_msgs_per_sec: u32,
    /// Frames waiting to be written per connection before it is dropped.
    pub max_queued_frames: usize,
    /// Open connections per IP (IPv6: per /64) before new ones are closed
    /// on accept; 0: no limit. Loopback is never limited (bots).
    pub max_conns_per_ip: usize,
    /// Second listener for browser clients: WebSocket carrying the same
    /// frames, over the same TLS (`wss://`) or plain in dev mode (`ws://`).
    pub ws_address: Option<SocketAddr>,
    /// Pages allowed to open a WebSocket (`Origin` header, such as
    /// `https://play.example.com`); empty: any page.
    pub ws_origins: Vec<String>,
}

impl NetConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            tls: None,
            max_msgs_per_sec: protocol::MAX_CLIENT_MSGS_PER_SEC,
            max_queued_frames: 512,
            max_conns_per_ip: 8,
            ws_address: None,
            ws_origins: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum NetEvent {
    Connected(ConnectionId, SocketAddr),
    Message(ConnectionId, ClientMsg),
    Disconnected(ConnectionId),
}

#[derive(Debug)]
pub enum NetError {
    Bind(std::io::Error),
    Tls(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Bind(e) if e.kind() == std::io::ErrorKind::AddrInUse => write!(
                f,
                "cannot listen: {e}; another server is probably running on this port: stop it (Ctrl+C) or use --address"
            ),
            NetError::Bind(e) => write!(f, "cannot listen: {e}"),
            NetError::Tls(e) => write!(f, "TLS: {e}"),
        }
    }
}

impl std::error::Error for NetError {}

/// Handle to the connections. Cheap to clone.
#[derive(Clone)]
pub struct Net {
    writers: Arc<Mutex<BTreeMap<ConnectionId, mpsc::Sender<Bytes>>>>,
    /// Open connections per IP group (`ip_group`).
    per_ip: Arc<Mutex<BTreeMap<IpAddr, usize>>>,
    local_addr: SocketAddr,
    ws_local_addr: Option<SocketAddr>,
}

/// IPv4 as is, IPv6 by its /64: one host often holds a whole /64.
fn ip_group(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            let mut o = v6.octets();
            for b in &mut o[8..] {
                *b = 0;
            }
            IpAddr::V6(Ipv6Addr::from(o))
        }
    }
}

/// One counted connection of an IP group; gives the place back on drop.
struct IpSlot {
    map: Arc<Mutex<BTreeMap<IpAddr, usize>>>,
    group: Option<IpAddr>,
}

impl IpSlot {
    /// `None` when the group already has `max` connections open.
    fn take(map: &Arc<Mutex<BTreeMap<IpAddr, usize>>>, ip: IpAddr, max: usize) -> Option<IpSlot> {
        if max == 0 || ip.is_loopback() {
            return Some(IpSlot {
                map: map.clone(),
                group: None,
            });
        }
        let group = ip_group(ip);
        let mut m = map.lock().unwrap_or_else(PoisonError::into_inner);
        let n = m.entry(group).or_insert(0);
        if *n >= max {
            return None;
        }
        *n += 1;
        Some(IpSlot {
            map: map.clone(),
            group: Some(group),
        })
    }
}

impl Drop for IpSlot {
    fn drop(&mut self) {
        let Some(group) = self.group else { return };
        let mut m = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(n) = m.get_mut(&group) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&group);
            }
        }
    }
}

impl Net {
    /// Starts listening. Events arrive on the returned receiver.
    pub async fn start(cfg: NetConfig) -> Result<(Net, mpsc::Receiver<NetEvent>), NetError> {
        let listener = TcpListener::bind(cfg.address).await.map_err(NetError::Bind)?;
        let local_addr = listener.local_addr().map_err(NetError::Bind)?;
        let ws_listener = match cfg.ws_address {
            Some(a) => Some(TcpListener::bind(a).await.map_err(NetError::Bind)?),
            None => None,
        };
        let ws_local_addr = match &ws_listener {
            Some(l) => Some(l.local_addr().map_err(NetError::Bind)?),
            None => None,
        };
        let (events, rx) = mpsc::channel(4096);
        let net = Net {
            writers: Arc::default(),
            per_ip: Arc::default(),
            local_addr,
            ws_local_addr,
        };
        info!(%local_addr, "listening");
        let ids = Arc::new(AtomicU64::new(1));
        if let Some(l) = ws_listener {
            info!(address = %l.local_addr().map_err(NetError::Bind)?, "listening for WebSocket");
            tokio::spawn(accept_loop(
                l,
                true,
                ids.clone(),
                net.clone(),
                cfg.clone(),
                events.clone(),
            ));
        }
        tokio::spawn(accept_loop(listener, false, ids, net.clone(), cfg, events));
        Ok((net, rx))
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Where the WebSocket listener is, if there is one.
    pub fn ws_local_addr(&self) -> Option<SocketAddr> {
        self.ws_local_addr
    }

    /// Queues one already encoded frame. A full queue drops the connection.
    pub fn send(&self, c: ConnectionId, frame: Bytes) {
        let mut writers = self.writers.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(w) = writers.get(&c)
            && w.try_send(frame).is_err()
        {
            warn!(conn = c.0, "output queue full or closed: dropping connection");
            writers.remove(&c);
        }
    }

    /// Encodes and sends one message.
    pub fn send_msg(&self, c: ConnectionId, msg: &ServerMsg) {
        let mut buf = Vec::new();
        match protocol::encode(msg, &mut buf) {
            Ok(()) => self.send(c, Bytes::from(buf)),
            Err(e) => warn!(conn = c.0, error = %e, "encode failed"),
        }
    }

    /// Sends `Error(code)` and closes after it is written.
    pub fn send_error_and_close(&self, c: ConnectionId, code: ErrorCode) {
        self.send_msg(
            c,
            &ServerMsg::Error {
                code,
                args: Vec::new(),
            },
        );
        self.disconnect(c);
    }

    /// Closes after the queued frames are written.
    pub fn disconnect(&self, c: ConnectionId) {
        self.writers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&c);
    }

    pub fn connection_count(&self) -> usize {
        self.writers.lock().unwrap_or_else(PoisonError::into_inner).len()
    }
}

async fn accept_loop(
    listener: TcpListener,
    ws: bool,
    ids: Arc<AtomicU64>,
    net: Net,
    cfg: NetConfig,
    events: mpsc::Sender<NetEvent>,
) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "accept failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let Some(slot) = IpSlot::take(&net.per_ip, peer.ip(), cfg.max_conns_per_ip) else {
            debug!(%peer, "too many connections from this IP");
            drop(stream);
            continue;
        };
        let _ = stream.set_nodelay(true);
        let id = ConnectionId(ids.fetch_add(1, Ordering::Relaxed));
        let acceptor = cfg.tls.clone().map(TlsAcceptor::from);
        // The handshake runs in the connection's own task, so a slow client
        // never holds up the accept loop.
        let (net, cfg, events) = (net.clone(), cfg.clone(), events.clone());
        tokio::spawn(async move {
            serve(id, stream, peer, acceptor, ws, net, cfg, events).await;
            drop(slot);
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    id: ConnectionId,
    stream: TcpStream,
    peer: SocketAddr,
    acceptor: Option<TlsAcceptor>,
    ws: bool,
    net: Net,
    cfg: NetConfig,
    events: mpsc::Sender<NetEvent>,
) {
    match acceptor {
        None => serve_plain(id, stream, peer, ws, net, cfg, events).await,
        Some(acceptor) => match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
            Ok(Ok(tls)) => serve_plain(id, tls, peer, ws, net, cfg, events).await,
            Ok(Err(e)) => debug!(%peer, error = %e, "TLS handshake failed"),
            Err(_) => debug!(%peer, "TLS handshake timed out"),
        },
    }
}

/// After TLS: either the frames themselves, or a WebSocket carrying them
/// (bridged to an in-memory pipe, so the rest is the same code).
async fn serve_plain<S>(
    id: ConnectionId,
    stream: S,
    peer: SocketAddr,
    ws: bool,
    net: Net,
    cfg: NetConfig,
    events: mpsc::Sender<NetEvent>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    if !ws {
        return run_connection(id, stream, peer, net, cfg, events).await;
    }
    let accepted = tokio::time::timeout(HANDSHAKE_TIMEOUT, ws::accept(stream, &cfg.ws_origins)).await;
    match accepted {
        Ok(Ok(socket)) => {
            let (inner, outer) = tokio::io::duplex(2 * MAX_CLIENT_FRAME);
            tokio::spawn(ws::bridge(socket, outer));
            run_connection(id, inner, peer, net, cfg, events).await;
        }
        Ok(Err(e)) => debug!(%peer, error = %e, "WebSocket handshake failed"),
        Err(_) => debug!(%peer, "WebSocket handshake timed out"),
    }
}

async fn run_connection<S>(
    id: ConnectionId,
    stream: S,
    peer: SocketAddr,
    net: Net,
    cfg: NetConfig,
    events: mpsc::Sender<NetEvent>,
) where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (tx, rx) = mpsc::channel(cfg.max_queued_frames);
    net.writers
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id, tx);
    if events.send(NetEvent::Connected(id, peer)).await.is_err() {
        return;
    }
    let (read, write) = tokio::io::split(stream);
    tokio::spawn(write_loop(write, rx));
    read_loop(id, read, net, cfg.max_msgs_per_sec, events).await;
}

async fn write_loop<W: AsyncWrite + Unpin>(mut w: W, mut rx: mpsc::Receiver<Bytes>) {
    while let Some(frame) = rx.recv().await {
        if w.write_all(&frame).await.is_err() {
            return;
        }
    }
    let _ = w.shutdown().await;
}

async fn read_loop<R: AsyncRead + Unpin>(
    id: ConnectionId,
    mut r: R,
    net: Net,
    max_per_sec: u32,
    events: mpsc::Sender<NetEvent>,
) {
    let reason = read_frames(id, &mut r, &net, max_per_sec, &events).await;
    debug!(conn = id.0, reason, "connection closed");
    net.disconnect(id);
    let _ = events.send(NetEvent::Disconnected(id)).await;
}

async fn read_frames<R: AsyncRead + Unpin>(
    id: ConnectionId,
    r: &mut R,
    net: &Net,
    max_per_sec: u32,
    events: &mpsc::Sender<NetEvent>,
) -> &'static str {
    let mut buf: Vec<u8> = Vec::with_capacity(2 * MAX_CLIENT_FRAME);
    let mut chunk = [0u8; 4096];
    let mut window = tokio::time::Instant::now();
    let mut count = 0u32;
    let mut warned = false;
    loop {
        let n = match r.read(&mut chunk).await {
            Ok(0) | Err(_) => return "closed by peer",
            Ok(n) => n,
        };
        buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
        let mut used = 0;
        loop {
            let rest = buf.get(used..).unwrap_or_default();
            match protocol::decode_frame::<ClientMsg>(rest, MAX_CLIENT_FRAME) {
                Ok(None) => break,
                Ok(Some((msg, len))) => {
                    used += len;
                    if window.elapsed() >= Duration::from_secs(1) {
                        window = tokio::time::Instant::now();
                        count = 0;
                        warned = false;
                    }
                    count += 1;
                    if count > max_per_sec.saturating_mul(2) {
                        net.send_error_and_close(id, ErrorCode::TooFast);
                        return "flood";
                    }
                    if count > max_per_sec {
                        if !warned {
                            net.send_msg(
                                id,
                                &ServerMsg::Error {
                                    code: ErrorCode::TooFast,
                                    args: Vec::new(),
                                },
                            );
                            warned = true;
                        }
                        continue;
                    }
                    if events.send(NetEvent::Message(id, msg)).await.is_err() {
                        return "server stopping";
                    }
                }
                Err(e) => {
                    let reason = match e {
                        ProtocolError::TooLarge { .. } => "frame too large",
                        _ => "invalid message",
                    };
                    net.send_error_and_close(id, ErrorCode::InvalidMessage);
                    return reason;
                }
            }
        }
        buf.drain(..used);
    }
}

#[cfg(test)]
mod ip_tests {
    use super::*;

    #[test]
    fn connections_per_ip_group_are_limited_and_given_back() {
        let map = Arc::default();
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let s1 = IpSlot::take(&map, a, 2).unwrap();
        let _s2 = IpSlot::take(&map, a, 2).unwrap();
        assert!(IpSlot::take(&map, a, 2).is_none());
        assert!(IpSlot::take(&map, "10.0.0.2".parse().unwrap(), 2).is_some());
        drop(s1);
        assert!(IpSlot::take(&map, a, 2).is_some());
        // One /64 is one group; loopback and 0 are never limited.
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        let _g = IpSlot::take(&map, v6, 1).unwrap();
        assert!(IpSlot::take(&map, "2001:db8::ffff".parse().unwrap(), 1).is_none());
        for _ in 0..5 {
            std::mem::forget(IpSlot::take(&map, "127.0.0.1".parse().unwrap(), 1).unwrap());
            std::mem::forget(IpSlot::take(&map, a, 0).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    async fn connect(net: &Net) -> TcpStream {
        TcpStream::connect(net.local_addr()).await.unwrap()
    }

    async fn start() -> (Net, mpsc::Receiver<NetEvent>) {
        let mut cfg = NetConfig::new("127.0.0.1:0".parse().unwrap());
        cfg.max_msgs_per_sec = 5;
        Net::start(cfg).await.unwrap()
    }

    fn frame(m: &ClientMsg) -> Vec<u8> {
        let mut b = Vec::new();
        protocol::encode(m, &mut b).unwrap();
        b
    }

    async fn read_server_msg(s: &mut TcpStream) -> Option<ServerMsg> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            if let Ok(Some((m, _))) = protocol::decode_frame::<ServerMsg>(&buf, protocol::MAX_SERVER_FRAME) {
                return Some(m);
            }
            let n = s.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    async fn start_ws(origins: Vec<String>) -> (Net, mpsc::Receiver<NetEvent>) {
        let mut cfg = NetConfig::new("127.0.0.1:0".parse().unwrap());
        cfg.ws_address = Some("127.0.0.1:0".parse().unwrap());
        cfg.ws_origins = origins;
        Net::start(cfg).await.unwrap()
    }

    #[tokio::test]
    async fn websocket_carries_the_same_frames() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let (net, mut rx) = start_ws(Vec::new()).await;
        let addr = net.ws_local_addr().unwrap();
        let tcp = TcpStream::connect(addr).await.unwrap();
        let (mut ws, _) = tokio_tungstenite::client_async(format!("ws://{addr}/"), tcp)
            .await
            .unwrap();
        let NetEvent::Connected(id, _) = rx.recv().await.unwrap() else {
            panic!()
        };
        // One frame split over two messages arrives whole.
        let f = frame(&ClientMsg::Ping { n: 9 });
        let (a, b) = f.split_at(3);
        ws.send(Message::Binary(a.to_vec().into())).await.unwrap();
        ws.send(Message::Binary(b.to_vec().into())).await.unwrap();
        let NetEvent::Message(_, ClientMsg::Ping { n: 9 }) = rx.recv().await.unwrap() else {
            panic!()
        };
        net.send_msg(id, &ServerMsg::Pong { n: 9 });
        let Some(Ok(Message::Binary(bytes))) = ws.next().await else {
            panic!()
        };
        let (m, _) = protocol::decode_frame::<ServerMsg>(&bytes, protocol::MAX_SERVER_FRAME)
            .unwrap()
            .unwrap();
        assert_eq!(m, ServerMsg::Pong { n: 9 });
    }

    #[tokio::test]
    async fn websocket_from_an_unlisted_page_is_refused() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let (net, _rx) = start_ws(vec!["https://play.example.com".into()]).await;
        let addr = net.ws_local_addr().unwrap();
        for (origin, ok) in [
            ("https://evil.example.com", false),
            ("https://play.example.com", true),
        ] {
            let tcp = TcpStream::connect(addr).await.unwrap();
            let mut req = format!("ws://{addr}/").into_client_request().unwrap();
            req.headers_mut().insert("origin", origin.parse().unwrap());
            assert_eq!(
                tokio_tungstenite::client_async(req, tcp).await.is_ok(),
                ok,
                "{origin}"
            );
        }
    }

    #[tokio::test]
    async fn messages_and_reply() {
        let (net, mut rx) = start().await;
        let mut s = connect(&net).await;
        let NetEvent::Connected(id, _) = rx.recv().await.unwrap() else {
            panic!()
        };
        s.write_all(&frame(&ClientMsg::Ping { n: 7 })).await.unwrap();
        let NetEvent::Message(_, ClientMsg::Ping { n: 7 }) = rx.recv().await.unwrap() else {
            panic!()
        };
        net.send_msg(id, &ServerMsg::Pong { n: 7 });
        assert_eq!(read_server_msg(&mut s).await, Some(ServerMsg::Pong { n: 7 }));
    }

    #[tokio::test]
    async fn garbage_closes_with_error() {
        let (net, mut rx) = start().await;
        let mut s = connect(&net).await;
        let _ = rx.recv().await;
        s.write_all(&[3, 0, 0, 0, 0xff, 0xff, 0xff]).await.unwrap();
        assert!(matches!(
            read_server_msg(&mut s).await,
            Some(ServerMsg::Error {
                code: ErrorCode::InvalidMessage,
                ..
            })
        ));
        assert!(matches!(rx.recv().await, Some(NetEvent::Disconnected(_))));
    }

    #[tokio::test]
    async fn flood_is_cut() {
        let (net, mut rx) = start().await;
        let mut s = connect(&net).await;
        let _ = rx.recv().await;
        let mut burst = Vec::new();
        for n in 0..20 {
            burst.extend(frame(&ClientMsg::Ping { n }));
        }
        s.write_all(&burst).await.unwrap();
        let mut delivered = 0;
        loop {
            match rx.recv().await.unwrap() {
                NetEvent::Message(..) => delivered += 1,
                NetEvent::Disconnected(_) => break,
                NetEvent::Connected(..) => {}
            }
        }
        assert_eq!(delivered, 5);
        assert!(matches!(
            read_server_msg(&mut s).await,
            Some(ServerMsg::Error {
                code: ErrorCode::TooFast,
                ..
            })
        ));
    }

    fn client_config(trusted: Option<&std::path::Path>) -> Arc<rustls::ClientConfig> {
        use rustls::pki_types::CertificateDer;
        use rustls::pki_types::pem::PemObject;
        let mut roots = rustls::RootCertStore::empty();
        if let Some(path) = trusted {
            for cert in CertificateDer::pem_file_iter(path).unwrap() {
                roots.add(cert.unwrap()).unwrap();
            }
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Arc::new(
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    }

    #[tokio::test]
    async fn tls_with_generated_certificate() {
        let dir = std::env::temp_dir().join(format!("server-net-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = TlsFiles {
            cert: dir.join("cert.pem"),
            key: dir.join("key.pem"),
        };
        let names = vec!["localhost".to_owned(), "127.0.0.1".to_owned()];
        let tls = files.load_or_generate(&names).unwrap();
        // Second start reuses the files instead of generating new ones.
        let cert_before = std::fs::read(&files.cert).unwrap();
        files.load_or_generate(&names).unwrap();
        assert_eq!(std::fs::read(&files.cert).unwrap(), cert_before);

        let mut cfg = NetConfig::new("127.0.0.1:0".parse().unwrap());
        cfg.tls = Some(tls);
        let (net, mut rx) = Net::start(cfg).await.unwrap();
        let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();

        // A client that trusts the certificate gets through.
        let connector = tokio_rustls::TlsConnector::from(client_config(Some(&files.cert)));
        let tcp = TcpStream::connect(net.local_addr()).await.unwrap();
        let mut s = connector.connect(server_name.clone(), tcp).await.unwrap();
        let NetEvent::Connected(id, _) = rx.recv().await.unwrap() else {
            panic!()
        };
        s.write_all(&frame(&ClientMsg::Ping { n: 3 })).await.unwrap();
        s.flush().await.unwrap();
        let NetEvent::Message(_, ClientMsg::Ping { n: 3 }) = rx.recv().await.unwrap() else {
            panic!()
        };
        net.send_msg(id, &ServerMsg::Pong { n: 3 });
        let mut buf = vec![0u8; 64];
        let n = s.read(&mut buf).await.unwrap();
        let (msg, _) = protocol::decode_frame::<ServerMsg>(&buf[..n], protocol::MAX_SERVER_FRAME)
            .unwrap()
            .unwrap();
        assert_eq!(msg, ServerMsg::Pong { n: 3 });

        // A client that does not trust it fails the handshake; plain TCP
        // bytes never reach the game either.
        let connector = tokio_rustls::TlsConnector::from(client_config(None));
        let tcp = TcpStream::connect(net.local_addr()).await.unwrap();
        assert!(connector.connect(server_name, tcp).await.is_err());
        let mut plain = TcpStream::connect(net.local_addr()).await.unwrap();
        plain.write_all(&frame(&ClientMsg::Ping { n: 1 })).await.unwrap();
        let mut sink = [0u8; 16];
        let _ = plain.read(&mut sink).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(200), rx.recv())
                .await
                .is_err(),
            "no event from failed handshakes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn huge_prefix_rejected() {
        let (net, mut rx) = start().await;
        let mut s = connect(&net).await;
        let _ = rx.recv().await;
        s.write_all(&[0xff, 0xff, 0xff, 0x00]).await.unwrap();
        assert!(matches!(rx.recv().await, Some(NetEvent::Disconnected(_))));
    }
}

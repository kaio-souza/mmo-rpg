//! Client connection: a reader thread with blocking IO and channels, so the
//! frame loop never waits on the network. No tokio: one connection does not
//! need it, and the binary stays small and the same on Windows and Linux.
//!
//! TLS uses rustls (ring) directly: the reader thread feeds received bytes
//! into the TLS state, `send` writes through it; a mutex guards both.
//!
//!

use protocol::{ClientMsg, MAX_SERVER_FRAME, ServerMsg};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

const READER_STACK: usize = 256 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Open,
    /// Closed by the server or by a network error.
    Closed,
    /// The server sent bytes that do not decode (or a TLS error).
    ProtocolError,
}

#[derive(Debug)]
pub enum ConnectionError {
    Io(std::io::Error),
    Tls(String),
    /// The server's certificate is not trusted (self-signed without
    /// `--trust-cert`, wrong file, or a name that does not match).
    UntrustedCertificate(String),
    /// The server answered the TLS hello with something else: it is
    /// probably a `--dev` server (plain TCP).
    NotTls,
    HandshakeTimeout,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Io(e) => write!(f, "{e}"),
            ConnectionError::Tls(e) => write!(f, "TLS: {e}"),
            ConnectionError::UntrustedCertificate(e) => write!(f, "server certificate not trusted: {e}"),
            ConnectionError::NotTls => f.write_str("the server does not speak TLS"),
            ConnectionError::HandshakeTimeout => f.write_str("TLS handshake timed out"),
        }
    }
}

impl std::error::Error for ConnectionError {}

/// Which certificates the client trusts: Mozilla's roots, plus optionally
/// one more (a self-signed server, `--trust-cert`). There is no way to turn
/// verification off.
#[derive(Clone)]
pub struct TlsConfig {
    config: Arc<ClientConfig>,
}

impl TlsConfig {
    pub fn new(extra_cert_pem: Option<&[u8]>) -> Result<TlsConfig, ConnectionError> {
        let mut roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        if let Some(pem) = extra_cert_pem {
            let mut added = 0;
            for cert in CertificateDer::pem_slice_iter(pem) {
                let cert = cert.map_err(|e| ConnectionError::Tls(format!("certificate file: {e}")))?;
                roots
                    .add(cert)
                    .map_err(|e| ConnectionError::Tls(format!("certificate file: {e}")))?;
                added += 1;
            }
            if added == 0 {
                return Err(ConnectionError::Tls("certificate file has no certificate".into()));
            }
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| ConnectionError::Tls(e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(TlsConfig {
            config: Arc::new(config),
        })
    }
}

/// Write side: the socket and, with TLS, the session state shared with the
/// reader thread.
struct Link {
    sock: TcpStream,
    tls: Option<ClientConnection>,
}

impl Link {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match &mut self.tls {
            None => self.sock.write_all(bytes),
            Some(tls) => {
                tls.writer().write_all(bytes)?;
                flush_tls(tls, &mut self.sock)
            }
        }
    }
}

fn flush_tls(tls: &mut ClientConnection, sock: &mut TcpStream) -> std::io::Result<()> {
    while tls.wants_write() {
        tls.write_tls(sock)?;
    }
    Ok(())
}

pub struct Connection {
    link: Arc<Mutex<Link>>,
    incoming: Receiver<ServerMsg>,
    state: Arc<AtomicU8>,
    buf: Mutex<Vec<u8>>,
}

impl Connection {
    /// `address` is `host:port`; with TLS, `host` is the name checked in
    /// the certificate (an IP works too). `None`: plain TCP (dev servers).
    pub fn connect(address: &str, tls: Option<&TlsConfig>) -> Result<Connection, ConnectionError> {
        let stream = TcpStream::connect(address).map_err(ConnectionError::Io)?;
        stream.set_nodelay(true).map_err(ConnectionError::Io)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(ConnectionError::Io)?;
        let reader = stream.try_clone().map_err(ConnectionError::Io)?;
        let tls = match tls {
            None => None,
            Some(cfg) => {
                let name = ServerName::try_from(host_of(address).to_owned())
                    .map_err(|e| ConnectionError::Tls(format!("server name: {e}")))?;
                Some(
                    ClientConnection::new(cfg.config.clone(), name)
                        .map_err(|e| ConnectionError::Tls(e.to_string()))?,
                )
            }
        };
        let mut link = Link { sock: stream, tls };
        if let Some(tls) = &mut link.tls {
            // Handshake here, so a bad certificate or a non-TLS server is a
            // clear error from `connect` instead of a silent close later.
            link.sock
                .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
                .map_err(ConnectionError::Io)?;
            while tls.is_handshaking() {
                tls.complete_io(&mut link.sock).map_err(handshake_error)?;
            }
            link.sock.set_read_timeout(None).map_err(ConnectionError::Io)?;
        }
        let link = Arc::new(Mutex::new(link));
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(AtomicU8::new(ConnectionState::Open as u8));
        let (st, lk) = (state.clone(), link.clone());
        std::thread::Builder::new()
            .name("net-reader".into())
            .stack_size(READER_STACK)
            .spawn(move || read_loop(reader, lk, tx, st))
            .map_err(ConnectionError::Io)?;
        Ok(Connection {
            link,
            incoming: rx,
            state,
            buf: Mutex::new(Vec::with_capacity(256)),
        })
    }

    /// Sends at once (small frames). A failure closes the connection.
    pub fn send(&self, msg: &ClientMsg) {
        let mut buf = self.buf.lock().unwrap_or_else(PoisonError::into_inner);
        buf.clear();
        if protocol::encode(msg, &mut buf).is_err() {
            return;
        }
        let mut link = self.link.lock().unwrap_or_else(PoisonError::into_inner);
        if link.write(&buf).is_err() {
            self.set_state(ConnectionState::Closed);
            let _ = link.sock.shutdown(Shutdown::Both);
        }
    }

    /// Next message, without blocking.
    pub fn receive(&self) -> Option<ServerMsg> {
        self.incoming.try_recv().ok()
    }

    /// Next message, waiting up to `timeout` (tools and tests).
    pub fn receive_timeout(&self, timeout: Duration) -> Option<ServerMsg> {
        self.incoming.recv_timeout(timeout).ok()
    }

    pub fn state(&self) -> ConnectionState {
        match self.state.load(Ordering::Acquire) {
            0 => ConnectionState::Open,
            1 => ConnectionState::Closed,
            _ => ConnectionState::ProtocolError,
        }
    }

    fn set_state(&self, s: ConnectionState) {
        self.state.store(s as u8, Ordering::Release);
    }

    pub fn close(&self) {
        let mut link = self.link.lock().unwrap_or_else(PoisonError::into_inner);
        let Link { sock, tls } = &mut *link;
        if let Some(tls) = tls {
            tls.send_close_notify();
            let _ = flush_tls(tls, sock);
        }
        let _ = sock.shutdown(Shutdown::Both);
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

fn handshake_error(e: std::io::Error) -> ConnectionError {
    use std::io::ErrorKind as K;
    match e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        Some(rustls::Error::InvalidCertificate(why)) => {
            ConnectionError::UntrustedCertificate(format!("{why:?}"))
        }
        Some(rustls::Error::InvalidMessage(_)) => ConnectionError::NotTls,
        Some(other) => ConnectionError::Tls(other.to_string()),
        None => match e.kind() {
            K::WouldBlock | K::TimedOut => ConnectionError::HandshakeTimeout,
            K::UnexpectedEof | K::ConnectionReset | K::ConnectionAborted => ConnectionError::NotTls,
            _ => ConnectionError::Io(e),
        },
    }
}

/// `host` of `host:port`, without the brackets of an IPv6 address.
fn host_of(address: &str) -> &str {
    let host = address.rsplit_once(':').map_or(address, |(h, _)| h);
    host.trim_start_matches('[').trim_end_matches(']')
}

fn read_loop(mut sock: TcpStream, link: Arc<Mutex<Link>>, tx: mpsc::Sender<ServerMsg>, state: Arc<AtomicU8>) {
    let end = |s: ConnectionState| state.store(s as u8, Ordering::Release);
    let mut frames: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut chunk = vec![0u8; 64 * 1024];
    let mut plain = vec![0u8; 64 * 1024];
    let tls = link.lock().unwrap_or_else(PoisonError::into_inner).tls.is_some();
    loop {
        let n = match sock.read(&mut chunk) {
            Ok(0) | Err(_) => return end(ConnectionState::Closed),
            Ok(n) => n,
        };
        let received = chunk.get(..n).unwrap_or_default();
        if !tls {
            frames.extend_from_slice(received);
        } else {
            let mut guard = link.lock().unwrap_or_else(PoisonError::into_inner);
            let Link { sock: wsock, tls } = &mut *guard;
            let Some(tls) = tls else { return };
            let mut rest = received;
            while !rest.is_empty() {
                if tls.read_tls(&mut rest).is_err() || tls.process_new_packets().is_err() {
                    let _ = flush_tls(tls, wsock); // the alert, if any
                    return end(ConnectionState::ProtocolError);
                }
                loop {
                    match tls.reader().read(&mut plain) {
                        Ok(0) => return end(ConnectionState::Closed), // close_notify
                        Ok(m) => frames.extend_from_slice(plain.get(..m).unwrap_or_default()),
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => return end(ConnectionState::Closed),
                    }
                }
            }
            if flush_tls(tls, wsock).is_err() {
                return end(ConnectionState::Closed);
            }
        }
        let mut used = 0;
        loop {
            match protocol::decode_frame::<ServerMsg>(
                frames.get(used..).unwrap_or_default(),
                MAX_SERVER_FRAME,
            ) {
                Ok(Some((msg, len))) => {
                    used += len;
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => return end(ConnectionState::ProtocolError),
            }
        }
        frames.drain(..used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_of_address() {
        assert_eq!(host_of("example.com:7000"), "example.com");
        assert_eq!(host_of("127.0.0.1:7000"), "127.0.0.1");
        assert_eq!(host_of("[::1]:7000"), "::1");
    }

    #[test]
    fn trust_file_must_hold_a_certificate() {
        assert!(TlsConfig::new(None).is_ok());
        assert!(TlsConfig::new(Some(b"not a pem")).is_err());
    }
}

//! Client connection to the server: the same API on every platform.
//!
//! - Desktop ([`native`]): TCP, with TLS by rustls, read by a thread.
//! - Browser (`web`): a WebSocket (`wss://`, TLS by the browser) carrying
//!   the same frames; the page's events fill the queue.
//!
//! `send` never blocks and `receive` never waits, so the frame loop is the
//! same on both.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{Connection, TlsConfig};
#[cfg(target_arch = "wasm32")]
pub use web::{Connection, TlsConfig};

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

/// The WebSocket URL a browser client opens for a server address: a full
/// `ws://` or `wss://` URL is kept, `host:port` gets `wss://` (or `ws://`
/// for a `--dev` server, without TLS).
pub fn websocket_url(address: &str, tls: bool) -> String {
    let address = address.trim();
    if address.starts_with("ws://") || address.starts_with("wss://") {
        return address.to_owned();
    }
    let scheme = if tls { "wss" } else { "ws" };
    format!("{scheme}://{address}/")
}

#[cfg(test)]
mod tests {
    use super::websocket_url;

    #[test]
    fn websocket_urls() {
        assert_eq!(
            websocket_url("play.example.com:7001", true),
            "wss://play.example.com:7001/"
        );
        assert_eq!(websocket_url("127.0.0.1:7001", false), "ws://127.0.0.1:7001/");
        assert_eq!(websocket_url(" wss://x.org/game ", false), "wss://x.org/game");
    }
}

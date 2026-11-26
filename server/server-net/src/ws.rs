//! WebSocket for browser clients. Each binary message carries bytes of the
//! same length-prefixed frame stream as TCP (a message may hold part of a
//! frame or several), so after the bridge the connection code is the same.

use futures_util::{SinkExt, StreamExt};
use protocol::MAX_CLIENT_FRAME;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error, Message};

/// The HTTP upgrade. A page whose `Origin` is not listed is refused (an
/// empty list allows any page).
#[allow(clippy::result_large_err)] // the refusal type is tungstenite's own
pub(crate) async fn accept<S>(stream: S, origins: &[String]) -> Result<WebSocketStream<S>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        if origins.is_empty() {
            return Ok(resp);
        }
        let origin = req.headers().get("origin").and_then(|o| o.to_str().ok());
        if origin.is_some_and(|o| origins.iter().any(|a| a == o)) {
            Ok(resp)
        } else {
            let mut refused = ErrorResponse::new(None);
            *refused.status_mut() = StatusCode::FORBIDDEN;
            Err(refused)
        }
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(2 * MAX_CLIENT_FRAME))
        .max_frame_size(Some(2 * MAX_CLIENT_FRAME));
    tokio_tungstenite::accept_hdr_async_with_config(stream, check, Some(config)).await
}

/// Copies between the socket and the pipe the connection reads and writes,
/// until either side ends.
pub(crate) async fn bridge<S>(socket: WebSocketStream<S>, pipe: DuplexStream)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut ws_out, mut ws_in) = socket.split();
    let (mut pipe_in, mut pipe_out) = tokio::io::split(pipe);
    let up = async {
        while let Some(Ok(msg)) = ws_in.next().await {
            match msg {
                Message::Binary(bytes) => {
                    if pipe_out.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                // Text is not part of the protocol; pings are answered by
                // the library.
                _ => {}
            }
        }
        let _ = pipe_out.shutdown().await;
    };
    let down = async {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match pipe_in.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let bytes = buf.get(..n).unwrap_or_default().to_vec();
            if ws_out.send(Message::Binary(bytes.into())).await.is_err() {
                return;
            }
        }
        let _ = ws_out.close().await;
    };
    tokio::select! {
        () = up => {}
        () = down => {}
    }
}

//! Browser transport: a WebSocket. The browser does TLS (`wss://`); each
//! binary message carries bytes of the frame stream. The page's events
//! fill a queue that `receive` empties; messages sent before the socket
//! opens wait in an outbox.

use crate::{ConnectionError, ConnectionState, websocket_url};
use protocol::{ClientMsg, MAX_SERVER_FRAME, ServerMsg};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

/// TLS is the browser's: it trusts what the browser trusts, so an extra
/// certificate file means nothing here. `None` (dev servers) is `ws://`.
#[derive(Clone)]
pub struct TlsConfig;

impl TlsConfig {
    pub fn new(_extra_cert_pem: Option<&[u8]>) -> Result<TlsConfig, ConnectionError> {
        Ok(TlsConfig)
    }
}

#[derive(Default)]
struct Shared {
    open: bool,
    state: Option<ConnectionState>,
    frames: Vec<u8>,
    incoming: VecDeque<ServerMsg>,
    outbox: Vec<Vec<u8>>,
}

type Handler<T> = Closure<dyn FnMut(T)>;

pub struct Connection {
    ws: WebSocket,
    shared: Rc<RefCell<Shared>>,
    _handlers: (
        Handler<Event>,
        Handler<MessageEvent>,
        Handler<Event>,
        Handler<CloseEvent>,
    ),
}

impl Connection {
    /// Starts opening the socket and returns at once; a socket that never
    /// opens turns `Closed`.
    pub fn connect(address: &str, tls: Option<&TlsConfig>) -> Result<Connection, ConnectionError> {
        let url = websocket_url(address, tls.is_some());
        let ws = WebSocket::new(&url)
            .map_err(|e| ConnectionError::Io(std::io::Error::other(format!("{url}: {e:?}"))))?;
        ws.set_binary_type(BinaryType::Arraybuffer);
        let shared = Rc::new(RefCell::new(Shared::default()));

        let (s, w) = (shared.clone(), ws.clone());
        let on_open: Handler<Event> = Closure::new(move |_: Event| {
            let mut s = s.borrow_mut();
            s.open = true;
            for bytes in std::mem::take(&mut s.outbox) {
                if w.send_with_u8_array(&bytes).is_err() {
                    s.state = Some(ConnectionState::Closed);
                }
            }
        });
        let s = shared.clone();
        let on_message: Handler<MessageEvent> = Closure::new(move |e: MessageEvent| {
            let Ok(buf) = e.data().dyn_into::<js_sys::ArrayBuffer>() else {
                return; // text is not part of the protocol
            };
            let mut s = s.borrow_mut();
            if s.state.is_some() {
                return;
            }
            let bytes = js_sys::Uint8Array::new(&buf).to_vec();
            s.frames.extend_from_slice(&bytes);
            let mut used = 0;
            loop {
                match protocol::decode_frame::<ServerMsg>(
                    s.frames.get(used..).unwrap_or_default(),
                    MAX_SERVER_FRAME,
                ) {
                    Ok(Some((msg, len))) => {
                        used += len;
                        s.incoming.push_back(msg);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        s.state = Some(ConnectionState::ProtocolError);
                        return;
                    }
                }
            }
            s.frames.drain(..used);
        });
        let s = shared.clone();
        let on_error: Handler<Event> = Closure::new(move |_: Event| {
            s.borrow_mut().state.get_or_insert(ConnectionState::Closed);
        });
        let s = shared.clone();
        let on_close: Handler<CloseEvent> = Closure::new(move |_: CloseEvent| {
            s.borrow_mut().state.get_or_insert(ConnectionState::Closed);
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        Ok(Connection {
            ws,
            shared,
            _handlers: (on_open, on_message, on_error, on_close),
        })
    }

    /// Sends at once, or when the socket opens. A failure closes it.
    pub fn send(&self, msg: &ClientMsg) {
        let mut buf = Vec::with_capacity(256);
        if protocol::encode(msg, &mut buf).is_err() {
            return;
        }
        let mut s = self.shared.borrow_mut();
        if !s.open {
            s.outbox.push(buf);
        } else if self.ws.send_with_u8_array(&buf).is_err() {
            s.state = Some(ConnectionState::Closed);
        }
    }

    /// Next message, without blocking.
    pub fn receive(&self) -> Option<ServerMsg> {
        self.shared.borrow_mut().incoming.pop_front()
    }

    /// A page cannot wait: the same as `receive`.
    pub fn receive_timeout(&self, _timeout: Duration) -> Option<ServerMsg> {
        self.receive()
    }

    /// `Closed` only once the queued messages were read, like the desktop.
    pub fn state(&self) -> ConnectionState {
        let s = self.shared.borrow();
        match s.state {
            Some(st) if s.incoming.is_empty() => st,
            _ => ConnectionState::Open,
        }
    }

    pub fn close(&self) {
        let _ = self.ws.close();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        self.close();
    }
}

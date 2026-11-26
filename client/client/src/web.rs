//! The client in a browser page: logging to the console, the game's files
//! from one bundle, the arguments from the query string, the page's canvas
//! and the renderer built asynchronously (a page cannot block).

use client_render::{Render, RenderError};
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// What the page fetches before starting: `content/` and `assets/`, made
/// by `web-bundle` (see `docs/web.md`).
const BUNDLE: &str = "game.bundle";
/// The page's canvas and status line.
const CANVAS_ID: &str = "game";
const STATUS_ID: &str = "status";

pub fn init_logging() {
    console_error_panic_hook::set_once();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .without_time()
        .with_ansi(false)
        .with_writer(Console::default)
        .init();
}

/// One log line, written to the browser console when dropped.
#[derive(Default)]
struct Console(Vec<u8>);

impl std::io::Write for Console {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        let line = String::from_utf8_lossy(&self.0);
        let line = line.trim_end();
        if !line.is_empty() {
            web_sys::console::log_1(&line.into());
        }
    }
}

pub async fn load_game() -> Result<(), String> {
    status("…");
    let bytes = client_files::fetch(BUNDLE).await?;
    let files = client_files::bundle::decode(&bytes)?;
    tracing::info!(files = files.len(), kib = bytes.len() / 1024, "game files loaded");
    client_files::mount(files);
    status("");
    Ok(())
}

/// Shown on the page, since a page has no terminal.
pub fn fail(e: &str) {
    tracing::error!("{e}");
    status(e);
}

fn status(text: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(STATUS_ID))
    {
        el.set_text_content(Some(text));
    }
}

/// `?server=host:7001&dev&locale=pt-BR` as command-line words.
pub fn query_args() -> Vec<String> {
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    let mut words = Vec::new();
    for pair in search
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
    {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let decode = |s: &str| {
            js_sys::decode_uri_component(&s.replace('+', " "))
                .ok()
                .and_then(|j| j.as_string())
                .unwrap_or_default()
        };
        words.push(format!("--{}", decode(k)));
        if !v.is_empty() {
            words.push(decode(v));
        }
    }
    words
}

pub fn canvas() -> Option<web_sys::HtmlCanvasElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(CANVAS_ID)?
        .dyn_into()
        .ok()
}

/// Where the renderer lands once the GPU answers.
#[derive(Default)]
pub struct RenderSlot(Rc<RefCell<Option<Result<Render, String>>>>);

impl RenderSlot {
    pub fn build(&self, building: impl Future<Output = Result<Render, RenderError>> + 'static) {
        let slot = self.0.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let r = building.await.map_err(|e| e.to_string());
            *slot.borrow_mut() = Some(r);
        });
    }

    pub fn take(&self) -> Option<Result<Render, String>> {
        self.0.borrow_mut().take()
    }
}

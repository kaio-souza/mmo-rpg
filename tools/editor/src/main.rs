//! `editor`: local web editor of `content/` (phase 6).
//!
//! ```text
//! editor --content content/ [--port 7878]
//! ```
//!
//! Listens on 127.0.0.1 only and serves one page built into the binary.
//! Every save loads the whole content with the change through `defs`;
//! with an error nothing is written. No scripts: content is closed blocks
//! picked from lists. Design:
//! And.

mod art;
mod game;
mod langs;
mod launch;
mod looks;
mod moderation;
mod replays;
mod sounds;
mod store;
mod texts;
mod theme;
mod ui;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use store::{Report, Store};
use texts::{Entry, Full, TextFile};
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(about = "Local web editor of the game content")]
struct Args {
    /// Content folder.
    #[arg(long, default_value = "content")]
    content: PathBuf,
    /// Port on 127.0.0.1.
    #[arg(long, default_value_t = 7878)]
    port: u16,
    /// Assets folder: sprite and icon names offered in the forms, and
    /// warnings for names without a file.
    #[arg(long, default_value = "assets")]
    assets: PathBuf,
    /// Game database for the Economy and Moderation screens (same format
    /// as the server). Opened only when the file exists.
    #[arg(long, default_value = "sqlite:data/game.db")]
    db: String,
    /// Folder of the zone replays for the Replays screen.
    #[arg(long, default_value = "replays")]
    replays: PathBuf,
}

struct App {
    store: Store,
    /// Read on every request: new art shows up without a restart.
    assets: PathBuf,
    /// One save at a time: check and write see the same files.
    lock: Mutex<()>,
    content: PathBuf,
    db: moderation::Db,
    replays: PathBuf,
    /// The game's folder (the parent of `content/`), where Play runs.
    game: PathBuf,
    play: std::sync::Mutex<launch::Play>,
}

type Shared = Arc<App>;

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    if !args.content.is_dir() {
        eprintln!(
            "error: no content folder at `{}`: run the editor from your game's folder (the one with content/ and assets/), or pass --content PATH",
            args.content.display()
        );
        return std::process::ExitCode::FAILURE;
    }
    let app = Arc::new(App {
        store: Store::new(args.content.clone()),
        assets: args.assets.clone(),
        lock: Mutex::new(()),
        content: args.content.clone(),
        db: moderation::Db::open(&args.db),
        replays: args.replays.clone(),
        game: std::fs::canonicalize(&args.content)
            .ok()
            .and_then(|c| c.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(".")),
        play: std::sync::Mutex::default(),
    });
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, args.port));
    let result = rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        println!("editor: http://{addr} (content: {})", args.content.display());
        axum::serve(listener, router(app)).await
    });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn router(app: Shared) -> Router {
    Router::new()
        .route("/", get(|| async { Html(include_str!("../web/index.html")) }))
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/map.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/map.js"),
                )
            }),
        )
        .route(
            "/dialogue.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/dialogue.js"),
                )
            }),
        )
        .route(
            "/looks.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/looks.js"),
                )
            }),
        )
        .route(
            "/langs.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/langs.js"),
                )
            }),
        )
        .route(
            "/sounds.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/sounds.js"),
                )
            }),
        )
        .route(
            "/links.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/links.js"),
                )
            }),
        )
        .route(
            "/game.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/game.js"),
                )
            }),
        )
        .route(
            "/theme.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/theme.js"),
                )
            }),
        )
        .route(
            "/admin.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/admin.js"),
                )
            }),
        )
        .route(
            "/replays.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/replays.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../web/app.css"),
                )
            }),
        )
        .route("/api/meta", get(meta))
        .route("/api/ui/{locale}", get(ui_text))
        .route("/api/content", get(content))
        .route("/api/all", get(all))
        .route("/api/schema/{kind}", get(schema))
        .route("/api/file", get(file))
        .route("/api/refs", get(refs))
        .route("/api/texts", get(text_table))
        .route("/api/locales", get(locales))
        .route("/api/check", post(check))
        .route("/api/save", post(save))
        .route("/api/art/{kind}/{name}", get(art))
        .route("/api/art", get(art_list))
        .route("/api/art/{kind}/{folder}/{name}", post(art_upload))
        .route("/api/tiles", get(tiles).post(tiles_save))
        .route("/api/sprite-manifest", get(manifest).post(manifest_save))
        .route("/api/sounds", get(sound_overview).post(sound_save))
        .route("/api/sound-file", get(sound_file))
        .route("/api/sound-file/{folder}/{name}", post(sound_upload))
        .route("/api/game", get(game_read).post(game_names))
        .route(
            "/api/game/background",
            get(game_background).post(game_background_save),
        )
        .route("/api/game/icon", get(game_icon).post(game_icon_save))
        .route("/api/themes", get(theme_overview))
        .route("/api/theme/{id}", get(theme_read).post(theme_save))
        .route("/api/theme/{id}/copy", post(theme_copy))
        .route("/api/theme/{id}/use", post(theme_use))
        .route("/api/admin/status", get(admin_status))
        .route("/api/admin/economy", get(admin_economy))
        .route("/api/admin/suspects", get(admin_suspects))
        .route("/api/admin/failed_logins", get(admin_failed_logins))
        .route("/api/admin/account", get(admin_account))
        .route("/api/admin/reports", get(admin_reports))
        .route("/api/admin/log", get(admin_log))
        .route("/api/admin/reasons", get(admin_reasons))
        .route("/api/admin/action", post(admin_action))
        .route("/api/replays", get(replay_list))
        .route("/api/replays/run", get(replay_run))
        .route("/api/play", get(play_status).post(play_start))
        .route("/api/new-game", post(new_game))
        // One PNG per request; the default limit is far below an image.
        .layer(DefaultBodyLimit::max(art::MAX_PNG_BYTES))
        .with_state(app)
}

fn fail(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({ "error": msg.into() }))).into_response()
}

async fn meta(State(app): State<Shared>) -> Json<Value> {
    let files = app.store.files().unwrap_or_default();
    let kinds: Vec<Value> = defs::editor::KINDS
        .iter()
        .map(|k| json!({ "name": k.name, "path": k.path, "per_id": k.per_id(), "texts": k.texts }))
        .collect();
    Json(json!({
        "kinds": kinds,
        "locales": store::locales(&files),
        // The editor page's own languages (its labels),.
        "page_locales": ui::page_locales(),
        "ticks_per_second": defs::editor::TICKS_PER_SECOND,
        // What the key of a counter family names (`kills` -> `monster`).
        "counter_keys": defs::COUNTER_KEYS.iter().map(|(f, k)| (*f, *k)).collect::<BTreeMap<_, _>>(),
    }))
}

async fn ui_text(UrlPath(locale): UrlPath<String>) -> Json<BTreeMap<String, String>> {
    Json(ui::texts(&locale))
}

async fn schema(UrlPath(kind): UrlPath<String>) -> Response {
    match defs::editor::schema(&kind) {
        Some(s) => Json(s).into_response(),
        None => fail(StatusCode::NOT_FOUND, format!("`{kind}` has no form")),
    }
}

/// Every file with its kind and id, the ids per kind, and the state of
/// the content on disk (errors, missing translations).
async fn content(State(app): State<Shared>) -> Response {
    let files = match app.store.files() {
        Ok(f) => f,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let assets = defs::AssetNames::read(&app.assets);
    let report = tokio::task::spawn_blocking({
        let files = files.clone();
        let assets = assets.clone();
        move || store::check(&files, &assets)
    })
    .await
    .unwrap_or_default();
    let mut list = Vec::new();
    let mut ids: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    ids.insert("sprite", assets.sprites.into_iter().collect());
    ids.insert("icon", assets.icons.into_iter().collect());
    // The families `counters.ron` turns on are what a title may count.
    let families = files
        .iter()
        .find(|(p, _)| p == "counters.ron")
        .and_then(|(_, t)| ron::from_str::<Vec<String>>(t).ok())
        .unwrap_or_default();
    ids.insert("counter_family", families);
    // A file git does not have yet can still change its id (nobody has it).
    let tracked = app.store.tracked();
    for (path, text) in &files {
        match defs::editor::kind_of(path) {
            Some((k, id)) => {
                if k.per_id() {
                    ids.entry(k.name).or_default().push(id.clone());
                }
                let brief = brief(k, text);
                let new = tracked.as_ref().is_some_and(|t| !t.contains(path));
                list.push(json!({ "path": path, "kind": k.name, "id": id, "brief": brief, "new": new }));
            }
            None => list.push(json!({ "path": path, "kind": null, "id": null })),
        }
    }
    let keys: Vec<String> = texts::table(&files).into_iter().map(|r| r.key).collect();
    Json(json!({ "files": list, "ids": ids, "report": report, "text_keys": keys })).into_response()
}

/// The top-level plain values of a content file (`kind`, `rarity`,
/// `icon`, `level`...), so the list can show a picture and a tag without
/// opening every file. Which ones matter is the page's choice.
fn brief(kind: &defs::editor::Kind, text: &str) -> Value {
    let Ok(Value::Object(fields)) = defs::editor::to_json(kind.name, text) else {
        return Value::Null;
    };
    // An enum with a payload (`trigger: Counter(..)`) shows as its variant
    // name, so the list can filter by it.
    let plain = fields.into_iter().filter_map(|(k, v)| match v {
        Value::String(ref s) if !s.is_empty() => Some((k, v)),
        Value::Number(_) | Value::Bool(_) => Some((k, v)),
        Value::Object(o) if o.len() == 1 => o.into_iter().next().map(|(name, _)| (k, Value::String(name))),
        _ => None,
    });
    Value::Object(plain.collect())
}

/// Every content file as form JSON: the page follows references between
/// them (where an item comes from, who gives a quest, where an NPC
/// stands). Text files and files without a form are left out.
async fn all(State(app): State<Shared>) -> Response {
    let files = match app.store.files() {
        Ok(f) => f,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let out = tokio::task::spawn_blocking(move || {
        files
            .iter()
            .filter_map(|(path, text)| {
                let (k, id) = defs::editor::kind_of(path)?;
                let json = defs::editor::to_json(k.name, text).ok()?;
                Some(json!({ "path": path, "kind": k.name, "id": id, "json": json }))
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(out).into_response()
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

async fn file(State(app): State<Shared>, Query(q): Query<FileQuery>) -> Response {
    let text = match app.store.read(&q.path) {
        Ok(t) => t,
        Err(e) => return fail(StatusCode::BAD_REQUEST, e),
    };
    let kind = defs::editor::kind_of(&q.path);
    let json = match (&kind, &text) {
        (Some((k, _)), Some(t)) => match defs::editor::to_json(k.name, t) {
            Ok(v) => Some(v),
            Err(e) => return fail(StatusCode::UNPROCESSABLE_ENTITY, format!("{}: {e}", q.path)),
        },
        _ => None,
    };
    Json(json!({
        "path": q.path,
        "kind": kind.as_ref().map(|(k, _)| k.name),
        "id": kind.map(|(_, id)| id),
        "exists": text.is_some(),
        "text": text,
        "json": json,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct RefsQuery {
    id: String,
}

/// Content files that name `id` between quotes: where something is used.
/// A plain text match, so a very common word may list a file by chance;
/// text files are left out and the page drops the open file itself.
async fn refs(State(app): State<Shared>, Query(q): Query<RefsQuery>) -> Response {
    let files = match app.store.files() {
        Ok(f) => f,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let needle = format!("\"{}\"", q.id);
    let found: Vec<Value> = files
        .iter()
        .filter(|(path, text)| !path.starts_with("text/") && text.contains(&needle))
        .filter_map(|(path, _)| {
            let (k, id) = defs::editor::kind_of(path)?;
            Some(json!({ "path": path, "kind": k.name, "id": id }))
        })
        .collect();
    Json(found).into_response()
}

/// The languages screen.
async fn locales(State(app): State<Shared>) -> Response {
    match app.store.files() {
        Ok(files) => Json(langs::overview(&files, &app.assets)).into_response(),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn text_table(State(app): State<Shared>) -> Response {
    match app.store.files() {
        Ok(files) => Json(texts::table(&files)).into_response(),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// A change from the page: a content file (form JSON or raw text) and
/// text entries.
#[derive(Deserialize, Default)]
struct Changes {
    #[serde(default)]
    files: Vec<FileChange>,
    #[serde(default)]
    texts: Vec<TextChange>,
    /// Files to remove: only ones git does not have yet (changing the id
    /// of something new).
    #[serde(default)]
    remove: Vec<String>,
    /// Text keys to remove in every locale (the old name of a renamed id).
    #[serde(default)]
    remove_texts: Vec<String>,
}

#[derive(Deserialize)]
struct FileChange {
    path: String,
    #[serde(default)]
    json: Option<Value>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct TextChange {
    locale: String,
    key: String,
    /// Empty in a translation: removes it (back to "missing").
    text: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    max_chars: Option<u32>,
}

/// Turns the page's changes into new file texts.
fn render(store: &Store, files: &[(String, String)], c: Changes) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    let remove_texts = c.remove_texts;
    for f in c.files {
        store.resolve(&f.path)?;
        let text = match (f.json, f.text) {
            (Some(json), _) => {
                let (kind, _) =
                    defs::editor::kind_of(&f.path).ok_or_else(|| format!("{}: no form", f.path))?;
                let old = files
                    .iter()
                    .find(|(p, _)| *p == f.path)
                    .map(|(_, t)| t.as_str())
                    .unwrap_or("");
                let body = defs::editor::to_ron(kind.name, json).map_err(|e| format!("{}: {e}", f.path))?;
                format!("{}{body}", defs::editor::header(old))
            }
            (None, Some(t)) => t,
            (None, None) => return Err(format!("{}: nothing to save", f.path)),
        };
        out.push((f.path, text));
    }
    // Text files touched by several changes are edited in turn.
    let mut edited: BTreeMap<String, TextFile> = BTreeMap::new();
    // English texts that changed: their translations get `review: true`.
    let mut changed_en: Vec<(String, String)> = Vec::new();
    for t in c.texts {
        if !store::locales(files).contains(&t.locale) {
            return Err(format!("unknown locale `{}`", t.locale));
        }
        let table = texts::table(files);
        let name = table
            .iter()
            .find(|r| r.key == t.key)
            .map(|r| r.file.clone())
            .unwrap_or_else(|| texts::file_for(files, &t.key));
        let path = format!("text/{}/{name}", t.locale);
        if !edited.contains_key(&path) {
            let current = files.iter().find(|(p, _)| *p == path).map(|(_, t)| t.as_str());
            let tf = match current {
                Some(text) => TextFile::parse(text).map_err(|e| format!("{path}: {e}"))?,
                None => TextFile {
                    header: format!("// Texts ({}).\n", name.trim_end_matches(".ron")),
                    entries: Vec::new(),
                },
            };
            edited.insert(path.clone(), tf);
        }
        let Some(tf) = edited.get_mut(&path) else {
            continue;
        };
        let entry = if t.locale == "en" {
            let old = tf
                .entries
                .iter()
                .find(|(k, _)| *k == t.key)
                .map(|(_, e)| e.clone());
            if old.as_ref().is_some_and(|o| o.text() != t.text) {
                changed_en.push((t.key.clone(), name.clone()));
            }
            let mut full = match old {
                Some(Entry::Full(f)) => f,
                _ => Full::default(),
            };
            full.text = t.text;
            if t.note.is_some() {
                full.note = t.note.filter(|n| !n.is_empty());
            }
            if t.max_chars.is_some() {
                full.max_chars = t.max_chars.filter(|m| *m > 0);
            }
            Some(Entry::Full(full))
        } else if t.text.is_empty() {
            None
        } else {
            Some(Entry::Plain(t.text))
        };
        tf.set(&t.key, entry);
    }
    for (key, name) in changed_en {
        for locale in store::locales(files).into_iter().filter(|l| l != "en") {
            let path = format!("text/{locale}/{name}");
            if !edited.contains_key(&path) {
                let Some((_, text)) = files.iter().find(|(p, _)| *p == path) else {
                    continue;
                };
                edited.insert(
                    path.clone(),
                    TextFile::parse(text).map_err(|e| format!("{path}: {e}"))?,
                );
            }
            if let Some(tf) = edited.get_mut(&path) {
                let marked = tf
                    .entries
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, e)| e.to_review());
                if marked.is_some() {
                    tf.set(&key, marked);
                }
            }
        }
    }
    if !remove_texts.is_empty() {
        let table = texts::table(files);
        for key in &remove_texts {
            let Some(name) = table.iter().find(|r| r.key == *key).map(|r| r.file.clone()) else {
                continue;
            };
            for locale in store::locales(files) {
                let path = format!("text/{locale}/{name}");
                if !edited.contains_key(&path) {
                    let Some((_, text)) = files.iter().find(|(p, _)| *p == path) else {
                        continue;
                    };
                    edited.insert(
                        path.clone(),
                        TextFile::parse(text).map_err(|e| format!("{path}: {e}"))?,
                    );
                }
                if let Some(tf) = edited.get_mut(&path) {
                    tf.set(key, None);
                }
            }
        }
    }
    out.extend(edited.into_iter().map(|(p, tf)| (p, tf.render())));
    Ok(out)
}

async fn run(app: Shared, c: Changes, write: bool) -> Response {
    let _guard = app.lock.lock().await;
    let app2 = app.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(Report, Vec<String>), String> {
        let files = app2.store.files()?;
        let removed = c.remove.clone();
        if !removed.is_empty() {
            let tracked = app2.store.tracked().unwrap_or_default();
            if let Some(p) = removed.iter().find(|p| tracked.contains(*p)) {
                return Err(format!("{p}: already in git; a published id is never removed"));
            }
        }
        let changes = render(&app2.store, &files, c)?;
        let assets = defs::AssetNames::read(&app2.assets);
        let report = store::check(&store::with_changes(&files, &changes, &removed), &assets);
        let mut written = Vec::new();
        if write && report.ok() {
            for (p, t) in &changes {
                app2.store.write(p, t)?;
                written.push(p.clone());
            }
            for p in &removed {
                app2.store.remove_new(p)?;
                written.push(p.clone());
            }
        }
        Ok((report, written))
    })
    .await;
    match result {
        Ok(Ok((report, written))) => {
            Json(json!({ "ok": report.ok(), "report": report, "written": written })).into_response()
        }
        Ok(Err(e)) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn check(State(app): State<Shared>, Json(c): Json<Changes>) -> Response {
    run(app, c, false).await
}

async fn save(State(app): State<Shared>, Json(c): Json<Changes>) -> Response {
    run(app, c, true).await
}

/// Every sprite and icon with its size, and the folders a new one may go
/// into (the ones already in use).
async fn art_list(State(app): State<Shared>) -> Response {
    let used = match app.store.files() {
        Ok(files) => tokio::task::spawn_blocking(move || store::used_art(&files))
            .await
            .unwrap_or_default(),
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    // The floor sprites are named by `assets/tiles.ron`, not by content;
    // without them every tile would look unused. The client's own fixed
    // names (the player figure) are not visible here.
    let mut sprites = used.0;
    sprites.extend(
        read_tiles(&app.assets)
            .into_values()
            .flat_map(|t| std::iter::once(t.sprite).chain(t.day_sprite)),
    );
    sprites.retain(|s| !s.is_empty());
    sprites.sort();
    sprites.dedup();
    let manifest = looks::read_manifest(&app.assets).unwrap_or_default();
    let mut out = serde_json::Map::new();
    for (kind, used) in [("sprite", sprites), ("icon", used.1)] {
        let images = art::list(&app.assets, kind);
        // How a sprite strip splits (phase 11), for the thumbnails: the
        // first frame, facing the camera.
        // A strip asked for that the image does not split into is drawn
        // whole by the client, and a still image under a folder that says
        // "4 x 4" is cut into tiny frames: the page marks both (a frame
        // size unlike the rest of its folder).
        let split: Vec<(&art::Image, u32, u32, bool)> = images
            .iter()
            .filter(|_| kind == "sprite")
            .map(|i| {
                let base = manifest.folders.get(&i.folder).copied().unwrap_or_default();
                let own = manifest.sprites.get(&i.name).copied().unwrap_or_default();
                let frames = own.frames.or(base.frames).unwrap_or(1).max(1);
                let dirs = own.directions.or(base.directions).unwrap_or(1).max(1);
                let whole = !i.width.is_multiple_of(frames)
                    || !i.height.is_multiple_of(dirs)
                    || !matches!(dirs, 1 | 4);
                (i, frames, dirs, whole)
            })
            .collect();
        let mut sizes: BTreeMap<(&str, u32, u32), usize> = BTreeMap::new();
        for (i, f, d, whole) in &split {
            if !whole {
                *sizes
                    .entry((i.folder.as_str(), i.width / f, i.height / d))
                    .or_default() += 1;
            }
        }
        let usual = |folder: &str| {
            sizes
                .iter()
                .filter(|((f, _, _), _)| *f == folder)
                .max_by_key(|(_, n)| **n)
                .map(|((_, w, h), _)| (*w, *h))
        };
        let mut bad_strips = Vec::new();
        let mut strips: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for (i, f, d, whole) in &split {
            let odd = !whole && usual(&i.folder).is_some_and(|u| u != (i.width / f, i.height / d));
            if (*whole && (*f, *d) != (1, 1)) || odd {
                bad_strips.push(i.name.clone());
            }
            strips.insert(i.name.clone(), if *whole { (1, 1) } else { (*f, *d) });
        }
        let mut folders: Vec<String> = images.iter().map(|i| i.folder.clone()).collect();
        folders.sort();
        folders.dedup();
        out.insert(
            kind.to_owned(),
            json!({ "images": images, "folders": folders, "used": used, "strips": strips, "bad_strips": bad_strips }),
        );
    }
    Json(json!({ "kinds": out, "max_bytes": art::MAX_PNG_BYTES })).into_response()
}

#[derive(Deserialize)]
struct UploadQuery {
    /// Overwrite a name that is already there.
    #[serde(default)]
    replace: bool,
}

/// Writes one uploaded PNG into `assets/`. The body is the file itself,
/// so the page sends the `File` as it is.
async fn art_upload(
    State(app): State<Shared>,
    UrlPath((kind, folder, name)): UrlPath<(String, String, String)>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Response {
    let _guard = app.lock.lock().await;
    // `-` stands for the root of `sprites/` or `icons/`: an empty path
    // step would not reach this route.
    let folder = if folder == "-" { String::new() } else { folder };
    match art::save(&app.assets, &kind, &folder, &name, &body, q.replace) {
        Ok(saved) => Json(saved).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

/// The PNG of a sprite or icon by name, for the preview next to the field.
async fn art(State(app): State<Shared>, UrlPath((kind, name)): UrlPath<(String, String)>) -> Response {
    let folder = match kind.as_str() {
        "sprite" => "sprites",
        "icon" => "icons",
        _ => return fail(StatusCode::NOT_FOUND, kind),
    };
    let safe = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    let found = safe.then(|| find_png(&app.assets.join(folder), &name)).flatten();
    match found.and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => ([(header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        None => fail(StatusCode::NOT_FOUND, name),
    }
}

fn find_png(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    for path in std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()) {
        if path.is_dir() {
            if let Some(p) = find_png(&path, name) {
                return Some(p);
            }
        } else if path.extension().is_some_and(|e| e == "png") && path.file_stem().is_some_and(|s| s == name)
        {
            return Some(path);
        }
    }
    None
}

fn read_tiles(assets: &std::path::Path) -> BTreeMap<u16, looks::TileLook> {
    looks::read_tiles(assets).unwrap_or_default()
}

/// `assets/tiles.ron` as JSON: the colors of the map painter and the
/// "Floors and scenery" screen.
async fn tiles(State(app): State<Shared>) -> Response {
    match looks::read_tiles(&app.assets) {
        Ok(t) => Json(t).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

/// Writes the whole `assets/tiles.ron` (the client reloads it on its own).
async fn tiles_save(State(app): State<Shared>, Json(t): Json<BTreeMap<u16, looks::TileLook>>) -> Response {
    let _guard = app.lock.lock().await;
    let errs = looks::check_tiles(&t);
    if !errs.is_empty() {
        return fail(StatusCode::UNPROCESSABLE_ENTITY, errs.join(" · "));
    }
    match looks::write(&app.assets.join("tiles.ron"), &looks::tiles_ron(&t)) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// `assets/sprites/sprites.ron` as JSON: anchors, strips and lights.
async fn manifest(State(app): State<Shared>) -> Response {
    match looks::read_manifest(&app.assets) {
        Ok(m) => Json(m).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

async fn manifest_save(State(app): State<Shared>, Json(m): Json<looks::Manifest>) -> Response {
    let _guard = app.lock.lock().await;
    let errs = looks::check_manifest(&m);
    if !errs.is_empty() {
        return fail(StatusCode::UNPROCESSABLE_ENTITY, errs.join(" · "));
    }
    let path = app.assets.join("sprites").join("sprites.ron");
    match looks::write(&path, &looks::manifest_ron(&m)) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// The Sounds screen: bindings, events, files, licenses.
async fn sound_overview(State(app): State<Shared>) -> Response {
    let content = app.store.files().unwrap_or_default();
    match sounds::overview(&app.assets, &content) {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

/// Writes the whole `assets/sounds/sounds.ron` (the client reloads it).
async fn sound_save(State(app): State<Shared>, Json(c): Json<defs::SoundConfig>) -> Response {
    let _guard = app.lock.lock().await;
    match sounds::save(&app.assets, &c) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

#[derive(Deserialize)]
struct SoundFileQuery {
    path: String,
}

/// One `.ogg` for the page's player.
async fn sound_file(State(app): State<Shared>, Query(q): Query<SoundFileQuery>) -> Response {
    match sounds::read(&app.assets, &q.path) {
        Some(bytes) => ([(header::CONTENT_TYPE, "audio/ogg")], bytes).into_response(),
        None => fail(StatusCode::NOT_FOUND, q.path),
    }
}

/// Writes one uploaded `.ogg`; the body is the file itself.
async fn sound_upload(
    State(app): State<Shared>,
    UrlPath((folder, name)): UrlPath<(String, String)>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Response {
    let _guard = app.lock.lock().await;
    match sounds::upload(&app.assets, &folder, &name, &body, q.replace) {
        Ok(path) => Json(json!({ "path": path })).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

// ---------- Game screen ----------

async fn game_read(State(app): State<Shared>) -> Json<Value> {
    Json(game::read(&app.assets))
}

#[derive(Deserialize)]
struct GameNames {
    names: BTreeMap<String, String>,
}

async fn game_names(State(app): State<Shared>, Json(body): Json<GameNames>) -> Response {
    let _guard = app.lock.lock().await;
    match game::save_names(&app.assets, &body.names) {
        Ok(()) => Json(game::read(&app.assets)).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

async fn game_background(State(app): State<Shared>) -> Response {
    match std::fs::read(app.assets.join(game::BACKGROUND)) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        Err(_) => fail(StatusCode::NOT_FOUND, game::BACKGROUND),
    }
}

async fn game_background_save(State(app): State<Shared>, body: Bytes) -> Response {
    let _guard = app.lock.lock().await;
    match game::save_background(&app.assets, &body) {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

async fn game_icon(State(app): State<Shared>) -> Response {
    match std::fs::read(app.assets.join(game::ICON)) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        Err(_) => fail(StatusCode::NOT_FOUND, game::ICON),
    }
}

async fn game_icon_save(State(app): State<Shared>, body: Bytes) -> Response {
    let _guard = app.lock.lock().await;
    match game::save_icon(&app.assets, &body) {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

// ---------- Theme screen ----------

async fn theme_overview(State(app): State<Shared>) -> Json<Value> {
    Json(theme::overview(&app.assets))
}

async fn theme_read(State(app): State<Shared>, UrlPath(id): UrlPath<String>) -> Response {
    answer(theme::read(&app.assets, &id))
}

async fn theme_save(
    State(app): State<Shared>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let _guard = app.lock.lock().await;
    answer(theme::save(&app.assets, &id, body))
}

#[derive(Deserialize)]
struct ThemeCopy {
    from: String,
}

async fn theme_copy(
    State(app): State<Shared>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<ThemeCopy>,
) -> Response {
    let _guard = app.lock.lock().await;
    answer(theme::create(&app.assets, &body.from, &id).map(|()| theme::overview(&app.assets)))
}

async fn theme_use(State(app): State<Shared>, UrlPath(id): UrlPath<String>) -> Response {
    let _guard = app.lock.lock().await;
    answer(theme::use_in_game(&app.assets, &id).map(|()| theme::overview(&app.assets)))
}

// ---------- Economy and Moderation screens ----------

fn answer(r: Result<Value, String>) -> Response {
    match r {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

async fn admin_status(State(app): State<Shared>) -> Json<Value> {
    Json(app.db.status())
}

async fn admin_economy(State(app): State<Shared>) -> Response {
    answer(moderation::economy(&app.db).await)
}

async fn admin_suspects(State(app): State<Shared>, Query(q): Query<moderation::SuspectQuery>) -> Response {
    answer(moderation::suspects(&app.db, q).await)
}

async fn admin_failed_logins(State(app): State<Shared>) -> Response {
    answer(moderation::failed_logins(&app.db).await)
}

#[derive(Deserialize)]
struct NameQuery {
    #[serde(default)]
    name: String,
}

async fn admin_account(State(app): State<Shared>, Query(q): Query<NameQuery>) -> Response {
    answer(moderation::account(&app.db, &q.name).await)
}

#[derive(Deserialize)]
struct SinceQuery {
    #[serde(default = "week")]
    since: String,
    #[serde(default)]
    account: Option<String>,
}

fn week() -> String {
    "7d".into()
}

async fn admin_reports(State(app): State<Shared>, Query(q): Query<SinceQuery>) -> Response {
    answer(moderation::chat_reports(&app.db, &q.since).await)
}

async fn admin_log(State(app): State<Shared>, Query(q): Query<SinceQuery>) -> Response {
    answer(moderation::log(&app.db, q.account.as_deref(), &q.since).await)
}

/// The moderation reasons with their English text.
async fn admin_reasons(State(app): State<Shared>) -> Response {
    let content = app.content.clone();
    let keys = match tokio::task::spawn_blocking(move || admin::reasons(&content)).await {
        Ok(Ok(k)) => k,
        Ok(Err(e)) => return fail(StatusCode::UNPROCESSABLE_ENTITY, e),
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let english = english_texts(&app);
    let list: Vec<Value> = keys
        .iter()
        .map(|k| json!({ "key": k, "text": english.get(k).cloned().unwrap_or_else(|| k.clone()) }))
        .collect();
    Json(json!(list)).into_response()
}

async fn admin_action(State(app): State<Shared>, Json(body): Json<moderation::ActionBody>) -> Response {
    match moderation::act(&app.db, &app.content, body).await {
        Ok(said) => Json(json!({ "ok": true, "message": said })).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

// ---------- Replays screen ----------

/// Key -> English text of the content.
fn english_texts(app: &App) -> BTreeMap<String, String> {
    let files = app.store.files().unwrap_or_default();
    texts::table(&files)
        .into_iter()
        .filter_map(|r| r.texts.get("en").cloned().map(|t| (r.key, t)))
        .collect()
}

async fn replay_list(State(app): State<Shared>) -> Response {
    let content = app.content.clone();
    let dir = app.replays.clone();
    match tokio::task::spawn_blocking(move || {
        let hash = defs::load(&content)
            .ok()
            .and_then(|c| c.validate().ok())
            .map(|c| c.content_hash());
        replays::list(&dir, hash)
    })
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct RunQuery {
    file: String,
    #[serde(default = "two")]
    every: u32,
}

fn two() -> u32 {
    2
}

async fn replay_run(State(app): State<Shared>, Query(q): Query<RunQuery>) -> Response {
    let Some(file) = replays::file_in(&app.replays, &q.file) else {
        return fail(
            StatusCode::BAD_REQUEST,
            format!("`{}` is not a replay file name", q.file),
        );
    };
    let english = english_texts(&app);
    let content = app.content.clone();
    match tokio::task::spawn_blocking(move || replays::run(&file, &content, q.every, &english)).await {
        Ok(r) => answer(r),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn play_status(State(app): State<Shared>) -> Json<Value> {
    let running = app.play.lock().is_ok_and(|mut p| p.running());
    Json(json!({ "running": running, "folder": app.game.display().to_string() }))
}

/// Starts the server and the client on this game (`mmo play`).
async fn play_start(State(app): State<Shared>) -> Response {
    let result = tokio::task::spawn_blocking(move || match app.play.lock() {
        Ok(mut p) => p.start(&app.game),
        Err(_) => Err("the play state is broken: restart the editor".into()),
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match result {
        Ok(()) => Json(json!({ "running": true })).into_response(),
        Err(e) => fail(StatusCode::CONFLICT, e),
    }
}

#[derive(Deserialize)]
struct NewGame {
    name: String,
}

/// Copies the template beside this game and opens an editor on it.
async fn new_game(State(app): State<Shared>, Json(body): Json<NewGame>) -> Response {
    let dir = match launch::new_game_dir(&app.game, body.name.trim()) {
        Ok(d) => d,
        Err(e) => return fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let shown = dir.display().to_string();
    match tokio::task::spawn_blocking(move || launch::new_game(&dir))
        .await
        .unwrap_or_else(|e| Err(e.to_string()))
    {
        Ok(url) => Json(json!({ "url": url, "folder": shown })).into_response(),
        Err(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

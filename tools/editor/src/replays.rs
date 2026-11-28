//! Replays screen: the files a zone recorded (`replays/`), and one of them
//! run again through the `replay` library to draw it. The page gets the
//! zone's ground once and, per frame, only the entities that changed, so a
//! 10-minute log stays a few megabytes.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every `.replay` file of the folder, newest first, with its header.
pub fn list(dir: &Path, content_hash: Option<u64>) -> Value {
    let mut out: Vec<(u64, Value)> = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return json!({ "dir": dir.display().to_string(), "files": [] });
    };
    for e in rd.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "replay") {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size = e.metadata().map_or(0, |m| m.len());
        // `zone-<name>-<unix>.replay`: the stamp sorts and dates it.
        let stamp: u64 = name
            .trim_end_matches(".replay")
            .rsplit('-')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let info = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| replay::info(&b));
        let v = match info {
            Ok(i) => json!({
                "file": name,
                "size": size,
                "at": stamp,
                "zone": i.zone,
                "records": i.records,
                "first_tick": i.first_tick,
                "same_content": content_hash.map(|h| h == i.content_hash),
            }),
            Err(err) => json!({ "file": name, "size": size, "at": stamp, "error": err }),
        };
        out.push((stamp, v));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    json!({ "dir": dir.display().to_string(), "files": out.into_iter().map(|(_, v)| v).collect::<Vec<_>>() })
}

/// A file name of the folder (no path tricks), or `None`.
pub fn file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    let ok = name.ends_with(".replay")
        && !name.contains(['/', '\\'])
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    ok.then(|| dir.join(name))
}

/// Runs the replay and returns what the page draws: the ground, every
/// entity's name, and one frame every `every` ticks with what changed.
/// Stops at the first tick whose state hash differs.
pub fn run(
    file: &Path,
    content: &Path,
    every: u32,
    english: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut r = replay::Replay::open(&bytes, content)?;
    let every = u64::from(every.clamp(1, 20));

    let map = r.zone().map();
    let origin = map.origin();
    let (w, h) = (map.width(), map.height());
    let mut floors = Vec::with_capacity(usize::try_from(w * h).unwrap_or(0));
    let mut blocked = String::with_capacity(floors.capacity());
    for y in 0..h {
        for x in 0..w {
            let p = base::TilePos::new(origin.x + x, origin.y + y);
            let t = map.tile(p);
            floors.push(t.map_or(0, |t| t.floor));
            blocked.push(if t.is_none_or(|t| t.blocks_movement) {
                '1'
            } else {
                '0'
            });
        }
    }

    let mut names: BTreeMap<u64, Value> = BTreeMap::new();
    let mut last: BTreeMap<u64, (i32, i32, u8, u8)> = BTreeMap::new();
    let mut frames: Vec<Value> = Vec::new();
    let mut commands: BTreeMap<String, u64> = BTreeMap::new();
    let mut tick_cmds: BTreeMap<String, u64> = BTreeMap::new();
    let (mut total_us, mut worst_us, mut ticks) = (0u128, 0u128, 0u64);
    let mut diverged = Value::Null;
    let mut frame = |r: &replay::Replay, tick: u64, cmds: &mut BTreeMap<String, u64>| {
        let now = r.entities();
        let mut set = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for e in &now {
            let id = u64::from(e.id.get());
            seen.insert(id);
            let state = (e.pos.x, e.pos.y, e.health, strip_row(e.direction));
            names.entry(id).or_insert_with(|| {
                let label = r.label(e);
                let shown = english.get(&label).cloned().unwrap_or(label);
                json!({ "kind": replay::kind_name(e.kind), "name": shown, "level": e.level, "layers": layers(r, replay::kind_name(e.kind) == "player", e.appearance, e.look) })
            });
            if last.get(&id) != Some(&state) {
                set.push(json!([id, state.0, state.1, state.2, state.3]));
                last.insert(id, state);
            }
        }
        let gone: Vec<u64> = last.keys().filter(|id| !seen.contains(id)).copied().collect();
        for id in &gone {
            last.remove(id);
        }
        let c: Vec<Value> = std::mem::take(cmds)
            .into_iter()
            .map(|(k, n)| json!([k, n]))
            .collect();
        frames.push(json!({ "tick": tick, "set": set, "gone": gone, "cmds": c }));
    };
    frame(&r, r.info.first_tick, &mut tick_cmds);
    while let Some(rec) = r.next_record() {
        for c in &rec.commands {
            let n = replay::command_name(c);
            *commands.entry(n.clone()).or_default() += 1;
            *tick_cmds.entry(n).or_default() += 1;
        }
        let Some(step) = r.step() else { break };
        let us = step.took.as_micros();
        total_us += us;
        worst_us = worst_us.max(us);
        ticks += 1;
        if let Some((got, recorded)) = step.diverged {
            frame(&r, step.tick, &mut tick_cmds);
            diverged = json!({ "tick": step.tick, "got": format!("{got:016x}"), "recorded": format!("{recorded:016x}") });
            break;
        }
        if ticks % every == 0 {
            frame(&r, step.tick, &mut tick_cmds);
        }
    }
    Ok(json!({
        "zone": r.info.zone,
        "seed": r.info.seed.to_string(),
        "first_tick": r.info.first_tick,
        "records": r.info.records,
        "every": every,
        "map": { "x": origin.x, "y": origin.y, "w": w, "h": h, "floors": floors, "blocked": blocked },
        "names": names,
        "frames": frames,
        "commands": commands,
        "ticks": ticks,
        "mean_us": total_us / u128::from(ticks.max(1)),
        "worst_us": worst_us,
        "diverged": diverged,
    }))
}

/// Row of a four-direction strip (S, W, N, E), as the client draws it.
fn strip_row(d: base::Direction) -> u8 {
    use base::Direction as D;
    match d {
        D::S | D::SE => 0,
        D::W | D::SW => 1,
        D::N | D::NW => 2,
        D::E | D::NE => 3,
    }
}

/// What an entity is drawn with, bottom first: `[sprite, tint]` pairs
/// (`tint`: `#rrggbb` or null). A player is body, clothes and hair tinted
/// by skin and hair colour, as in the client; the others one sprite.
fn layers(r: &replay::Replay, player: bool, appearance: u16, look: base::Look) -> Value {
    let cat = &r.zone().content().catalog;
    let hex = |c: u32| format!("#{c:06x}");
    if !player {
        let sprite = usize::from(appearance)
            .checked_sub(1)
            .and_then(|i| cat.sprites().get(i));
        return match sprite {
            Some(s) => json!([[s, null]]),
            None => json!([]),
        };
    }
    let lists = &cat.appearance().lists;
    let pick = |i: usize, at: u8| {
        lists
            .get(i)
            .and_then(|l| l.get(usize::from(at)).or_else(|| l.first()))
    };
    let skin = pick(1, look.skin).map(|o| hex(o.color));
    let hair = pick(3, look.hair_color).map(|o| hex(o.color));
    let body = pick(0, look.body);
    let mut out = Vec::new();
    match body.filter(|b| !b.sprite.is_empty()) {
        Some(b) => out.push(json!([b.sprite, skin])),
        None => out.push(json!(["player", skin])),
    }
    if let Some(c) = body.map(|b| &b.clothes).filter(|c| !c.is_empty()) {
        out.push(json!([c, null]));
    }
    if let Some(h) = pick(2, look.hair_style).filter(|o| !o.sprite.is_empty()) {
        out.push(json!([h.sprite, hair]));
    }
    Value::Array(out)
}

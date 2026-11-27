//! Zone definitions: `content/zones/<name>/zone.ron` and `map.ron`.
//!
//! ```ron
//! // zone.ron
//! (origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 8, y: 8), spawn_radius: 0)
//! // map.ron
//! (
//!     legend: {
//!         '.': (floor: 1),
//!         '#': (floor: 3, blocks_movement: true, blocks_sight: true),
//!     },
//!     rows: ["#####", "#...#", "#####"],
//! )
//! ```
//!
//! Map characters are layout, not game text. Width and height come from the rows.

use crate::craft::NodeSpawn;
use crate::npc::{NpcSpawn, parse_spawns, valid_id};
use base::TilePos;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Floors fit in 13 bits (on the wire the 16th bit marks "blocks movement",
/// the 15th "blocks sight",, and the 14th "can be switched",
///).
pub const MAX_FLOOR: u16 = 0x1fff;
pub const MAX_VIEW_RADIUS: u16 = 64;
/// Largest side of a zone map.
pub const MAX_ZONE_SIDE: i32 = 4096;
/// Width of the seam band along a neighbor (`view_radius + 4`,
///): ghosts cover it, and bosses, dense
/// spawners, cities, regions and the spawn stay out of it.
pub const SEAM_BAND: i32 = 24;
/// A player passes to the neighbor zone after walking this many tiles
/// past the edge (hysteresis: coming back needs the same on the other side).
pub const HANDOFF_DEPTH: i32 = 2;
/// A spawn entry with this many monsters or more is dense: it must stay
/// out of the seam band.
pub const DENSE_SPAWN: u16 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneDef {
    /// Folder name, `[a-z0-9_]`.
    pub name: String,
    /// Global position of the top left tile.
    pub origin: TilePos,
    pub width: i32,
    pub height: i32,
    pub view_radius: u16,
    /// Where new characters appear (phase 1: everyone).
    pub spawn: TilePos,
    /// New characters land on a free tile up to this far from `spawn` (0: on it).
    pub spawn_radius: u16,
    /// Row by row (`y` then `x`).
    pub tiles: Vec<TileDef>,
    /// NPCs that spawn with the zone (`npcs.ron`; empty when absent).
    pub npcs: Vec<NpcSpawn>,
    /// Safe and PvP rectangles (`zone.ron`); the rest is normal ground.
    pub regions: Vec<RegionDef>,
    /// Monster spawns (`monsters.ron`; empty when absent).
    pub monsters: Vec<MonsterSpawn>,
    /// Resource nodes (`nodes.ron`, phase 8; empty when absent).
    pub nodes: Vec<NodeSpawn>,
    /// Phase 10: new characters (and characters whose zone is gone) start
    /// here. With more than one zone, exactly one has it.
    pub starting: bool,
    /// Tiles that take a player to another tile of the world.
    pub portals: Vec<PortalDef>,
    /// A copy per party instead of one shared zone.
    pub instance: Option<InstanceRule>,
}

/// A rectangle of tiles, corners inclusive, in global coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub min: TilePos,
    pub max: TilePos,
}

impl Rect {
    pub fn contains(&self, p: TilePos) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }

    /// Tiles from `p` to the nearest tile of the rectangle (Chebyshev; 0 inside).
    pub fn distance_to(&self, p: TilePos) -> i32 {
        let dx = (self.min.x - p.x).max(p.x - self.max.x).max(0);
        let dy = (self.min.y - p.y).max(p.y - self.max.y).max(0);
        dx.max(dy)
    }

    /// Smallest Chebyshev distance between a tile of each (0: they overlap,
    /// 1: they touch, by a side or a corner).
    pub fn distance(&self, o: &Rect) -> i32 {
        let dx = (o.min.x - self.max.x).max(self.min.x - o.max.x).max(0);
        let dy = (o.min.y - self.max.y).max(self.min.y - o.max.y).max(0);
        dx.max(dy)
    }

    /// Grown by `n` tiles on every side.
    pub fn grow(&self, n: i32) -> Rect {
        Rect {
            min: TilePos::new(self.min.x.saturating_sub(n), self.min.y.saturating_sub(n)),
            max: TilePos::new(self.max.x.saturating_add(n), self.max.y.saturating_add(n)),
        }
    }
}

/// Rules of a piece of ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
pub enum RegionKind {
    /// No combat at all.
    Safe,
    /// Players may attack players.
    Pvp,
    /// Alpha: a workshop (forge, workbench). Outside a production class,
    /// crafting happens only here. It may lie inside another
    /// region.
    Workshop,
}

/// The last rectangle of `regions` that holds `p`, workshops left out.
pub fn region_of(regions: &[RegionDef], p: TilePos) -> Option<RegionKind> {
    regions
        .iter()
        .rev()
        .filter(|r| r.kind != RegionKind::Workshop)
        .find(|r| r.holds(p))
        .map(|r| r.kind)
}

/// Some workshop of `regions` holds `p`.
pub fn in_workshop(regions: &[RegionDef], p: TilePos) -> bool {
    regions
        .iter()
        .any(|r| r.kind == RegionKind::Workshop && r.holds(p))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RegionDef {
    pub kind: RegionKind,
    /// Corners, inclusive, global coordinates.
    pub min: TilePos,
    pub max: TilePos,
}

impl RegionDef {
    pub fn holds(&self, p: TilePos) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }
}

/// A zone that is never part of the open world: each party that walks
/// into it (a portal or a `Teleport` aimed at it) gets its own copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct InstanceRule {
    /// A tile of the open world where players go when the copy closes on
    /// them, and where a character that leaves the game inside is saved.
    pub exit: TilePos,
    /// An empty copy closes after this long.
    #[serde(default = "sixty")]
    pub close_after_secs: u32,
    /// Everyone inside goes to `exit` after this long (0: no limit).
    #[serde(default)]
    pub time_limit_secs: u32,
}

fn sixty() -> u32 {
    60
}

/// A portal: stepping on any tile of `min..=max` takes the player to `to`,
/// a tile of any zone (global coordinates, like the whole world).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PortalDef {
    /// Corners, inclusive, global coordinates.
    pub min: TilePos,
    pub max: TilePos,
    /// Where the player lands (the nearest free tile if taken).
    pub to: TilePos,
}

impl PortalDef {
    pub fn holds(&self, p: TilePos) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }
}

/// `count` monsters placed up to `radius` tiles from `pos`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MonsterSpawn {
    #[cfg_attr(feature = "editor", schemars(with = "crate::editor::MonsterRef"))]
    pub monster: String,
    pub pos: TilePos,
    #[serde(default = "one")]
    pub count: u16,
    #[serde(default)]
    pub radius: u16,
}

fn one() -> u16 {
    1
}

impl ZoneDef {
    /// The zone's own tiles.
    pub fn rect(&self) -> Rect {
        Rect {
            min: self.origin,
            max: TilePos::new(
                self.origin.x.saturating_add(self.width - 1),
                self.origin.y.saturating_add(self.height - 1),
            ),
        }
    }

    pub fn contains(&self, p: TilePos) -> bool {
        self.rect().contains(p)
    }

    /// The tile at a global position, when it is in the zone.
    pub fn tile_at(&self, p: TilePos) -> Option<&TileDef> {
        if !self.contains(p) {
            return None;
        }
        let lx = i64::from(p.x) - i64::from(self.origin.x);
        let ly = i64::from(p.y) - i64::from(self.origin.y);
        self.tiles
            .get(usize::try_from(ly * i64::from(self.width) + lx).ok()?)
    }

    /// Region of a tile: the last rectangle that holds it; `None` is normal
    /// ground. A workshop lies over the ground's region and never counts.
    pub fn region_at(&self, p: TilePos) -> Option<RegionKind> {
        region_of(&self.regions, p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TileDef {
    pub floor: u16,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocks_movement: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocks_sight: bool,
    /// Phase 11: the floor a player switches this tile to by
    /// clicking it within reach (a brazier lit or put out); clicking again
    /// brings the floor back. What the tile blocks does not change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<u16>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ZoneFile {
    origin: TilePos,
    view_radius: u16,
    spawn: TilePos,
    #[serde(default)]
    spawn_radius: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    regions: Vec<RegionDef>,
    /// Tiles that take a player elsewhere, in this zone or another.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    portals: Vec<PortalDef>,
    /// A dungeon: every party gets its own copy of this zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    instance: Option<InstanceRule>,
    /// Phase 10: new characters start in this zone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    starting: bool,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "editor", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct MapFile {
    legend: BTreeMap<char, TileDef>,
    rows: Vec<String>,
}

/// Parses and checks one zone; every problem is returned as a message.
pub(crate) fn parse_zone(
    name: &str,
    zone: &str,
    map: &str,
    npcs: Option<&str>,
    monsters: Option<&str>,
    nodes: Option<&str>,
) -> Result<ZoneDef, Vec<String>> {
    let mut errors = Vec::new();
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        errors.push("zone folder name must be [a-z0-9_]".to_owned());
    }
    let zone: ZoneFile = ron::from_str(zone).map_err(|e| vec![format!("zone.ron: {e}")])?;
    let map: MapFile = ron::from_str(map).map_err(|e| vec![format!("map.ron: {e}")])?;

    for (c, t) in &map.legend {
        if t.floor == 0 || t.floor > MAX_FLOOR {
            errors.push(format!("map.ron: legend '{c}': floor must be 1..={MAX_FLOOR}"));
        }
        if let Some(s) = t.switch
            && (s == 0 || s > MAX_FLOOR || s == t.floor)
        {
            errors.push(format!(
                "map.ron: legend '{c}': switch must be 1..={MAX_FLOOR} and not the floor itself"
            ));
        }
    }
    let height = map.rows.len();
    let width = map.rows.first().map_or(0, |r| r.chars().count());
    let side_ok = |n: usize| n >= 1 && n <= MAX_ZONE_SIDE as usize;
    if !side_ok(width) || !side_ok(height) {
        errors.push(format!(
            "map.ron: size {width}x{height}, each side must be 1..={MAX_ZONE_SIDE}"
        ));
        return Err(errors);
    }
    let mut tiles = Vec::with_capacity(width * height);
    for (y, row) in map.rows.iter().enumerate() {
        if row.chars().count() != width {
            errors.push(format!(
                "map.ron: row {y} has {} tiles, expected {width}",
                row.chars().count()
            ));
            continue;
        }
        for (x, c) in row.chars().enumerate() {
            match map.legend.get(&c) {
                Some(t) => tiles.push(*t),
                None => errors.push(format!("map.ron: row {y} column {x}: '{c}' is not in the legend")),
            }
        }
    }
    let (width, height) = (width as i32, height as i32);
    if zone.view_radius == 0 || zone.view_radius > MAX_VIEW_RADIUS {
        errors.push(format!("zone.ron: view_radius must be 1..={MAX_VIEW_RADIUS}"));
    }
    let local = |p: TilePos| -> Option<usize> {
        let lx = i64::from(p.x) - i64::from(zone.origin.x);
        let ly = i64::from(p.y) - i64::from(zone.origin.y);
        ((0..i64::from(width)).contains(&lx) && (0..i64::from(height)).contains(&ly))
            .then(|| (ly * i64::from(width) + lx) as usize)
    };
    match local(zone.spawn).and_then(|i| tiles.get(i)) {
        None if errors.is_empty() => errors.push("zone.ron: spawn is outside the map".to_owned()),
        Some(t) if t.blocks_movement => errors.push("zone.ron: spawn is on a blocking tile".to_owned()),
        _ => {}
    }
    if zone.origin.x.checked_add(width).is_none() || zone.origin.y.checked_add(height).is_none() {
        errors.push("zone.ron: origin + size overflows".to_owned());
    }
    let mut spawns = Vec::new();
    if let Some(text) = npcs {
        match parse_spawns(text) {
            Ok(list) => {
                for (i, spawn) in list.iter().enumerate() {
                    if !valid_id(&spawn.npc) {
                        errors.push(format!(
                            "npcs.ron: entry {i}: npc `{}` must be [a-z0-9_]",
                            spawn.npc
                        ));
                    }
                    match local(spawn.pos).and_then(|j| tiles.get(j)) {
                        None => {
                            errors.push(format!("npcs.ron: entry {i}: `{}` is outside the map", spawn.npc))
                        }
                        Some(t) if t.blocks_movement => errors.push(format!(
                            "npcs.ron: entry {i}: `{}` is on a blocking tile",
                            spawn.npc
                        )),
                        _ => {}
                    }
                    if spawns.iter().any(|s: &NpcSpawn| s.pos == spawn.pos) {
                        errors.push(format!(
                            "npcs.ron: entry {i}: `{}` is on a tile taken by another NPC",
                            spawn.npc
                        ));
                    }
                    spawns.push(NpcSpawn {
                        npc: spawn.npc.clone(),
                        pos: spawn.pos,
                    });
                }
            }
            Err(e) => errors.push(e),
        }
    }
    let mut monster_spawns = Vec::new();
    if let Some(text) = monsters {
        match ron::from_str::<Vec<MonsterSpawn>>(text) {
            Ok(list) => {
                for (i, m) in list.into_iter().enumerate() {
                    match local(m.pos).and_then(|j| tiles.get(j)) {
                        Some(t) if !t.blocks_movement && m.count >= 1 => monster_spawns.push(m),
                        _ => errors.push(format!(
                            "monsters.ron: entry {i}: `{}` outside the map, on a blocking tile or count 0",
                            m.monster
                        )),
                    }
                }
            }
            Err(e) => errors.push(format!("monsters.ron: {e}")),
        }
    }
    let mut node_spawns: Vec<NodeSpawn> = Vec::new();
    if let Some(text) = nodes {
        match crate::craft::parse_nodes(text) {
            Ok(list) => {
                for (i, n) in list.into_iter().enumerate() {
                    let free = local(n.pos)
                        .and_then(|j| tiles.get(j))
                        .is_some_and(|t| !t.blocks_movement);
                    // A node is a being on its tile: one per tile, never on
                    // an NPC or a wall.
                    let taken =
                        node_spawns.iter().any(|s| s.pos == n.pos) || spawns.iter().any(|s| s.pos == n.pos);
                    if !valid_id(&n.node) || !free || taken {
                        errors.push(format!(
                            "nodes.ron: entry {i}: `{}` has a bad id, is outside the map, on a blocking tile or on a taken tile",
                            n.node
                        ));
                        continue;
                    }
                    node_spawns.push(n);
                }
            }
            Err(e) => errors.push(e),
        }
    }
    for (i, r) in zone.regions.iter().enumerate() {
        if r.min.x > r.max.x || r.min.y > r.max.y {
            errors.push(format!("zone.ron: region {i}: min must be <= max"));
        }
    }
    for (i, p) in zone.portals.iter().enumerate() {
        if p.min.x > p.max.x || p.min.y > p.max.y {
            errors.push(format!("zone.ron: portal {i}: min must be <= max"));
        } else if local(p.min).is_none() || local(p.max).is_none() {
            errors.push(format!("zone.ron: portal {i}: outside the map"));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(ZoneDef {
        name: name.to_owned(),
        origin: zone.origin,
        width,
        height,
        view_radius: zone.view_radius,
        spawn: zone.spawn,
        spawn_radius: zone.spawn_radius,
        tiles,
        npcs: spawns,
        regions: zone.regions,
        monsters: monster_spawns,
        nodes: node_spawns,
        starting: zone.starting,
        portals: zone.portals,
        instance: zone.instance,
    })
}

/// Zones that touch `zone` (by a side or a corner): its seam neighbors,
/// in catalog order.
pub fn neighbors<'a>(zone: &ZoneDef, all: impl IntoIterator<Item = &'a ZoneDef>) -> Vec<&'a ZoneDef> {
    let own = zone.rect();
    all.into_iter()
        .filter(|z| z.name != zone.name && own.distance(&z.rect()) == 1)
        .collect()
}

/// Rules of the world as a whole (phase 10):
/// zones never overlap, one zone is the start, and the spawn, regions, NPCs
/// and dense spawners stay out of the seam band. `(zone, message)` pairs.
/// The zone whose map holds `p`, if any.
pub fn zone_at<'a>(zones: impl IntoIterator<Item = &'a ZoneDef>, p: TilePos) -> Option<&'a ZoneDef> {
    zones.into_iter().find(|z| z.tile_at(p).is_some())
}

pub(crate) fn world_problems(zones: &[ZoneDef]) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    // A portal lands on walkable ground of some zone, and never on another
    // portal (no ping-pong).
    for zone in zones {
        for (i, p) in zone.portals.iter().enumerate() {
            let msg = match zone_at(zones, p.to) {
                None => Some(format!("portal {i}: `to` is in no zone")),
                Some(z) if z.tile_at(p.to).is_some_and(|t| t.blocks_movement) => {
                    Some(format!("portal {i}: `to` is a blocking tile of `{}`", z.name))
                }
                Some(z) if z.portals.iter().any(|q| q.holds(p.to)) => {
                    Some(format!("portal {i}: `to` is on a portal of `{}`", z.name))
                }
                _ => None,
            };
            if let Some(m) = msg {
                problems.push((zone.name.clone(), m));
            }
        }
        // An instance stands apart: no seam, never the start, an exit on
        // the open world's walkable ground.
        if let Some(rule) = &zone.instance {
            if zone.starting {
                problems.push((
                    zone.name.clone(),
                    "an instance cannot be the starting zone".into(),
                ));
            }
            if !neighbors(zone, zones).is_empty() {
                problems.push((zone.name.clone(), "an instance cannot touch another zone".into()));
            }
            let exit_ok = zone_at(zones, rule.exit)
                .filter(|z| z.instance.is_none())
                .and_then(|z| z.tile_at(rule.exit))
                .is_some_and(|t| !t.blocks_movement);
            if !exit_ok {
                problems.push((
                    zone.name.clone(),
                    "instance exit: not walkable ground of an open-world zone".into(),
                ));
            }
            if rule.close_after_secs == 0 {
                problems.push((zone.name.clone(), "instance close_after_secs must be >= 1".into()));
            }
        }
    }
    for (i, a) in zones.iter().enumerate() {
        for b in zones.iter().skip(i + 1) {
            if a.rect().distance(&b.rect()) == 0 {
                problems.push((a.name.clone(), format!("overlaps zone `{}`", b.name)));
            }
        }
    }
    let starts: Vec<&str> = zones
        .iter()
        .filter(|z| z.starting)
        .map(|z| z.name.as_str())
        .collect();
    // Instances are not part of the open world: they never count here.
    let open = zones.iter().filter(|z| z.instance.is_none()).count();
    if open > 1 && starts.len() != 1 {
        problems.push((
            starts.first().copied().unwrap_or("").to_owned(),
            format!("exactly one zone needs `starting: true`, found {}", starts.len()),
        ));
    }
    for zone in zones {
        // A newbie who falls comes back on the spawn: next to monsters they
        // would fall again at once (alpha pace test). In a world of several
        // zones, a zone with monsters keeps its spawn on safe ground.
        let spawn_area = Rect {
            min: zone.spawn,
            max: zone.spawn,
        }
        .grow(i32::from(zone.spawn_radius));
        let safe = zone.regions.iter().any(|r| {
            let area = Rect {
                min: r.min,
                max: r.max,
            };
            r.kind == RegionKind::Safe && area.contains(spawn_area.min) && area.contains(spawn_area.max)
        });
        if zones.len() > 1 && !zone.monsters.is_empty() && !safe {
            problems.push((
                zone.name.clone(),
                "the spawn (where fallen newbies come back) needs a `Safe` region around it".to_owned(),
            ));
        }
        for n in neighbors(zone, zones) {
            let other = n.rect();
            let near = |r: &Rect| r.distance(&other) <= SEAM_BAND;
            let mut bad = |what: String| {
                problems.push((
                    zone.name.clone(),
                    format!("{what} is in the seam band of `{}` ({SEAM_BAND} tiles)", n.name),
                ));
            };
            let point = |p: TilePos| Rect { min: p, max: p };
            if near(&point(zone.spawn).grow(i32::from(zone.spawn_radius))) {
                bad("the spawn".to_owned());
            }
            for (i, r) in zone.regions.iter().enumerate() {
                if near(&Rect {
                    min: r.min,
                    max: r.max,
                }) {
                    bad(format!("region {i} ({:?})", r.kind));
                }
            }
            for s in &zone.npcs {
                if near(&point(s.pos)) {
                    bad(format!("npc `{}`", s.npc));
                }
            }
            for m in zone.monsters.iter().filter(|m| m.count >= DENSE_SPAWN) {
                if near(&point(m.pos).grow(i32::from(m.radius))) {
                    bad(format!("the dense spawn of `{}`", m.monster));
                }
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE: &str = "(origin: (x: -2, y: 5), view_radius: 20, spawn: (x: -1, y: 6))";
    const MAP: &str = r####"(
        legend: { '.': (floor: 1), '#': (floor: 3, blocks_movement: true, blocks_sight: true) },
        rows: ["###", "#.#", "###"],
    )"####;

    #[test]
    fn valid_zone() {
        let z = parse_zone("starting_city", ZONE, MAP, None, None, None).unwrap();
        assert_eq!((z.width, z.height), (3, 3));
        assert_eq!(z.tiles.len(), 9);
        assert!(z.tiles[0].blocks_sight);
        assert!(!z.tiles[4].blocks_movement);
    }

    #[test]
    fn problems() {
        let bad_spawn = "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 0, y: 0))";
        assert!(parse_zone("z", bad_spawn, MAP, None, None, None).unwrap_err()[0].contains("blocking"));
        let outside = "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 9, y: 0))";
        assert!(parse_zone("z", outside, MAP, None, None, None).unwrap_err()[0].contains("outside"));
        let ragged = r#"(legend: { '.': (floor: 1) }, rows: ["..", "."])"#;
        let z = "(origin: (x: 0, y: 0), view_radius: 20, spawn: (x: 0, y: 0))";
        assert!(parse_zone("z", z, ragged, None, None, None).unwrap_err()[0].contains("row 1"));
        let unknown = r#"(legend: { '.': (floor: 1) }, rows: [".x"])"#;
        assert!(parse_zone("z", z, unknown, None, None, None).unwrap_err()[0].contains("'x'"));
        assert!(
            parse_zone(
                "Bad Name",
                z,
                r#"(legend: { '.': (floor: 1) }, rows: ["."])"#,
                None,
                None,
                None
            )
            .is_err()
        );
        assert!(parse_zone("z", "(origin: (x: 0, y: 0))", MAP, None, None, None).is_err());
    }

    /// Alpha pace test: in a world, a zone with monsters revives fallen
    /// newbies on safe ground.
    #[test]
    fn a_hunting_zone_revives_on_safe_ground() {
        let map = r#"(legend: { '.': (floor: 1) }, rows: [".....", ".....", ".....", ".....", "....."])"#;
        let zone = |x: i32, starting: bool, safe: bool| {
            let monsters = format!("[(monster: \"rat\", pos: (x: {}, y: 4))]", x + 4);
            let regions = if safe {
                format!(
                    "regions: [(kind: Safe, min: (x: {x}, y: 0), max: (x: {}, y: 2))],",
                    x + 2
                )
            } else {
                String::new()
            };
            let text = format!(
                "(origin: (x: {x}, y: 0), view_radius: 20, spawn: (x: {}, y: 1), {regions} starting: {starting})",
                x + 1
            );
            parse_zone("z", &text, map, None, Some(&monsters), None).unwrap()
        };
        let problems = world_problems(&[zone(0, true, true), zone(5, false, false)]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].1.contains("`Safe`"));
        assert!(world_problems(&[zone(0, true, true), zone(5, false, true)]).is_empty());
    }
}

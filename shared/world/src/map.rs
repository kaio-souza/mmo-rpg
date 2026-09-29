use base::{RegionId, TilePos};
use std::fmt;

/// Chunk side sent to the client (same as `protocol::CHUNK_SIZE`).
pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_TILES: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;
/// Bit set in a chunk tile word when the tile blocks movement; the low 13
/// bits are the floor (same encoding as `protocol::TILE_BLOCKS`).
pub const TILE_BLOCKS: u16 = 0x8000;
/// Bit set when the tile blocks sight (protocol 18,: the client
/// shades what the player cannot see).
pub const TILE_HIDES: u16 = 0x4000;
/// Bit set when a player can switch the tile (protocol 21,: a
/// brazier lit or put out).
pub const TILE_SWITCH: u16 = 0x2000;
/// Floor bits of a tile word.
pub const TILE_FLOOR: u16 = 0x1fff;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Tile {
    /// Floor sprite, 1..=0x1fff (`0` means nothing).
    pub floor: u16,
    pub blocks_movement: bool,
    pub blocks_sight: bool,
    pub region: RegionId,
    /// The floor a player switches this tile to (`0`: none); switching
    /// swaps the two, so the next switch brings the first one back.
    pub switch: u16,
}

impl Tile {
    /// The tile as a chunk word.
    pub fn word(&self) -> u16 {
        self.floor
            | if self.blocks_movement { TILE_BLOCKS } else { 0 }
            | if self.blocks_sight { TILE_HIDES } else { 0 }
            | if self.switch != 0 { TILE_SWITCH } else { 0 }
    }
}

/// The part of the world map one zone owns, in global coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileMap {
    origin: TilePos,
    width: i32,
    height: i32,
    tiles: Vec<Tile>,
    /// One bit per tile, set when it blocks sight: line of sight reads it
    /// at every step, and a bit set stays in cache where the tiles do not.
    /// What a tile blocks never changes (`switch` keeps it).
    hides: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    BadSize { width: i32, height: i32 },
    TileCount { expected: usize, got: usize },
    FloorTooLarge { floor: u16 },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSize { width, height } => write!(f, "bad map size {width}x{height}"),
            Self::TileCount { expected, got } => write!(f, "map has {got} tiles, expected {expected}"),
            Self::FloorTooLarge { floor } => write!(f, "floor {floor} does not fit in 14 bits"),
        }
    }
}

impl std::error::Error for MapError {}

/// Largest side of a zone map.
const MAX_SIDE: i32 = 4096;

impl TileMap {
    /// `tiles` row by row (`y` then `x`).
    pub fn new(origin: TilePos, width: i32, height: i32, tiles: Vec<Tile>) -> Result<TileMap, MapError> {
        if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
            return Err(MapError::BadSize { width, height });
        }
        let expected = (width * height) as usize;
        if tiles.len() != expected {
            return Err(MapError::TileCount {
                expected,
                got: tiles.len(),
            });
        }
        if let Some(t) = tiles.iter().find(|t| (t.floor | t.switch) & !TILE_FLOOR != 0) {
            return Err(MapError::FloorTooLarge {
                floor: t.floor.max(t.switch),
            });
        }
        let mut hides = vec![0u64; tiles.len().div_ceil(64)];
        for (i, t) in tiles.iter().enumerate() {
            if t.blocks_sight
                && let Some(w) = hides.get_mut(i / 64)
            {
                *w |= 1 << (i % 64);
            }
        }
        Ok(TileMap {
            origin,
            width,
            height,
            tiles,
            hides,
        })
    }

    pub fn origin(&self) -> TilePos {
        self.origin
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn contains(&self, p: TilePos) -> bool {
        self.index(p).is_some()
    }

    fn index(&self, p: TilePos) -> Option<usize> {
        let lx = p.x.checked_sub(self.origin.x)?;
        let ly = p.y.checked_sub(self.origin.y)?;
        if (0..self.width).contains(&lx) && (0..self.height).contains(&ly) {
            Some((ly * self.width + lx) as usize)
        } else {
            None
        }
    }

    pub fn tile(&self, p: TilePos) -> Option<&Tile> {
        self.tiles.get(self.index(p)?)
    }

    /// Outside the map blocks.
    pub fn blocks_movement(&self, p: TilePos) -> bool {
        self.tile(p).is_none_or(|t| t.blocks_movement)
    }

    /// `p` in this map's own coordinates, when it is on the map.
    pub(crate) fn local(&self, p: TilePos) -> Option<(i32, i32)> {
        self.index(p)?;
        Some((p.x - self.origin.x, p.y - self.origin.y))
    }

    /// `blocks_sight` for a tile already known to be on the map, in local
    /// coordinates (line of sight's inner loop).
    pub(crate) fn hides_local(&self, x: i32, y: i32) -> bool {
        let i = (y * self.width + x) as usize;
        self.hides.get(i / 64).is_none_or(|w| w >> (i % 64) & 1 == 1)
    }

    /// Outside the map blocks.
    pub fn blocks_sight(&self, p: TilePos) -> bool {
        self.index(p)
            .and_then(|i| self.hides.get(i / 64).map(|w| w >> (i % 64) & 1 == 1))
            .unwrap_or(true)
    }

    pub fn region(&self, p: TilePos) -> Option<RegionId> {
        self.tile(p).map(|t| t.region)
    }

    /// Switches a tile that can be switched (its floor and `switch` trade
    /// places; what it blocks stays). The new word, or `None`.
    pub fn switch(&mut self, p: TilePos) -> Option<u16> {
        let i = self.index(p)?;
        let t = self.tiles.get_mut(i).filter(|t| t.switch != 0)?;
        std::mem::swap(&mut t.floor, &mut t.switch);
        Some(t.word())
    }

    /// Tile words of chunk `(cx, cy)` in global chunk coordinates; tiles
    /// outside this map are `0`.
    pub fn chunk_words(&self, cx: i32, cy: i32) -> Vec<u16> {
        let mut out = Vec::with_capacity(CHUNK_TILES);
        let (x0, y0) = (cx.saturating_mul(CHUNK_SIZE), cy.saturating_mul(CHUNK_SIZE));
        for ly in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let p = TilePos::new(x0.saturating_add(lx), y0.saturating_add(ly));
                out.push(self.tile(p).map_or(0, Tile::word));
            }
        }
        out
    }

    /// Chunks that overlap this map, row by row.
    pub fn chunks(&self) -> impl Iterator<Item = (i32, i32)> {
        let x0 = self.origin.x.div_euclid(CHUNK_SIZE);
        let y0 = self.origin.y.div_euclid(CHUNK_SIZE);
        let x1 = (self.origin.x + self.width - 1).div_euclid(CHUNK_SIZE);
        let y1 = (self.origin.y + self.height - 1).div_euclid(CHUNK_SIZE);
        (y0..=y1).flat_map(move |cy| (x0..=x1).map(move |cx| (cx, cy)))
    }

    /// Map from text rows, for tests and tools: `#` wall (blocks both),
    /// `+` glass (blocks movement only), anything else floor.
    pub fn from_rows(origin: TilePos, rows: &[&str]) -> Result<TileMap, MapError> {
        let height = rows.len() as i32;
        let width = rows.first().map_or(0, |r| r.chars().count()) as i32;
        let tiles = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(|c| Tile {
                floor: 1,
                blocks_movement: c == '#' || c == '+',
                blocks_sight: c == '#',
                region: RegionId::new(0),
                switch: 0,
            })
            .collect();
        TileMap::new(origin, width, height, tiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_coordinates() {
        let m = TileMap::from_rows(TilePos::new(-2, 100), &["..#", "..."]).unwrap();
        assert!(!m.blocks_movement(TilePos::new(-2, 100)));
        assert!(m.blocks_movement(TilePos::new(0, 100)));
        assert!(m.blocks_movement(TilePos::new(1, 100))); // outside
        assert!(m.blocks_movement(TilePos::new(-2, 99)));
        assert!(m.contains(TilePos::new(0, 101)));
    }

    #[test]
    fn chunks_and_words() {
        let m = TileMap::from_rows(TilePos::new(-1, 0), &["#.", ".."]).unwrap();
        assert_eq!(m.chunks().collect::<Vec<_>>(), [(-1, 0), (0, 0)]);
        let w = m.chunk_words(-1, 0);
        assert_eq!(w.len(), CHUNK_TILES);
        assert_eq!(w[31], 1 | TILE_BLOCKS | TILE_HIDES); // global (-1, 0), a wall
        assert_eq!(w[31] & TILE_FLOOR, 1);
        assert_eq!(w[63], 1); // global (-1, 1)
        assert_eq!(w[0], 0);
        assert_eq!(m.chunk_words(0, 0)[0], 1);
    }

    #[test]
    fn a_switch_trades_floors_and_keeps_what_blocks() {
        let mut m = TileMap::from_rows(TilePos::new(0, 0), &["+."]).unwrap();
        assert_eq!(m.switch(TilePos::new(0, 0)), None, "nothing to switch");
        let brazier = Tile {
            floor: 6,
            switch: 19,
            blocks_movement: true,
            ..Tile::default()
        };
        let mut m = TileMap::new(TilePos::new(0, 0), 1, 1, vec![brazier]).unwrap();
        let p = TilePos::new(0, 0);
        assert_eq!(m.chunk_words(0, 0)[0], 6 | TILE_BLOCKS | TILE_SWITCH);
        assert_eq!(m.switch(p), Some(19 | TILE_BLOCKS | TILE_SWITCH));
        assert!(m.blocks_movement(p));
        assert_eq!(m.switch(p), Some(6 | TILE_BLOCKS | TILE_SWITCH), "and back");
        assert_eq!(m.switch(TilePos::new(5, 5)), None, "outside");
    }

    #[test]
    fn rejects_bad_maps() {
        assert!(TileMap::new(TilePos::new(0, 0), 0, 1, vec![]).is_err());
        assert!(TileMap::new(TilePos::new(0, 0), 2, 1, vec![Tile::default()]).is_err());
        let big = Tile {
            floor: 0x8000,
            ..Tile::default()
        };
        assert!(TileMap::new(TilePos::new(0, 0), 1, 1, vec![big]).is_err());
    }
}

//! Terrain is sent in chunks of 32×32 tiles, in global coordinates.

use base::TilePos;
use std::fmt;

pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_TILES: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Bit set in a tile word when the tile blocks movement. The low 13 bits are
/// the floor sprite (`0` = nothing, outside the map).
pub const TILE_BLOCKS: u16 = 0x8000;
/// Protocol 18: bit set when the tile blocks sight.
pub const TILE_HIDES: u16 = 0x4000;
/// Protocol 21: bit set when a player can switch the tile
/// (`ClientMsg::UseTile`: a brazier lit or put out).
pub const TILE_SWITCH: u16 = 0x2000;
/// The floor bits of a tile word.
pub const TILE_FLOOR: u16 = 0x1fff;

/// Line of sight between two tiles over tile words, the same walk as the
/// server's (`world::line_of_sight`): the ends never block, a tile in
/// between with `TILE_HIDES` (or unknown, `None`) does.
pub fn sight_line(tile: impl Fn(TilePos) -> Option<u16>, a: TilePos, b: TilePos) -> bool {
    // Walk from the smaller endpoint so the line is the same both ways.
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let (adx, ady) = (dx.abs(), dy.abs());
    let (sx, sy) = (dx.signum(), dy.signum());
    let mut err = adx - ady;
    let (mut x, mut y) = (i64::from(from.x), i64::from(from.y));
    let (tx, ty) = (i64::from(to.x), i64::from(to.y));
    if x == tx && y == ty {
        return true;
    }
    loop {
        let e2 = 2 * err;
        if e2 > -ady {
            err -= ady;
            x += sx;
        }
        if e2 < adx {
            err += adx;
            y += sy;
        }
        if x == tx && y == ty {
            return true;
        }
        let (Ok(px), Ok(py)) = (i32::try_from(x), i32::try_from(y)) else {
            return false;
        };
        if tile(TilePos::new(px, py)).is_none_or(|w| w & TILE_HIDES != 0) {
            return false;
        }
    }
}

/// Chunk coordinates of the chunk that holds `p` (floor division).
pub fn chunk_of(p: TilePos) -> (i32, i32) {
    (p.x.div_euclid(CHUNK_SIZE), p.y.div_euclid(CHUNK_SIZE))
}

/// Chunks are sent at least this far beyond the view radius, so terrain is
/// there before entities show up on it. The client limits its zoom so the
/// screen never reaches past it (: no black edge).
pub const CHUNK_MARGIN: i32 = 24;

/// Chunks the client keeps around its player, in chunks each way;
/// farther ones are forgotten.
pub const KEEP_CHUNKS: i32 = 4;

/// The chunk `(cx, cy)` is at most `keep` chunks from the one holding
/// `center`. The client forgets with `KEEP_CHUNKS` and the server with one
/// less, so the server never thinks the client still has a chunk it dropped.
pub fn chunk_kept(center: TilePos, (cx, cy): (i32, i32), keep: i32) -> bool {
    let (x, y) = chunk_of(center);
    (cx - x).abs() <= keep && (cy - y).abs() <= keep
}

/// Exactly `CHUNK_TILES` tile words, row by row (`y` then `x`).
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "Vec<u16>", into = "Vec<u16>")]
pub struct ChunkTiles(Vec<u16>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkTilesError {
    pub got: usize,
}

impl fmt::Display for ChunkTilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "chunk has {} tiles, expected {CHUNK_TILES}", self.got)
    }
}

impl std::error::Error for ChunkTilesError {}

impl TryFrom<Vec<u16>> for ChunkTiles {
    type Error = ChunkTilesError;
    fn try_from(v: Vec<u16>) -> Result<Self, Self::Error> {
        if v.len() == CHUNK_TILES {
            Ok(Self(v))
        } else {
            Err(ChunkTilesError { got: v.len() })
        }
    }
}

impl From<ChunkTiles> for Vec<u16> {
    fn from(c: ChunkTiles) -> Vec<u16> {
        c.0
    }
}

impl ChunkTiles {
    /// Tile word at local coordinates; `None` outside `0..CHUNK_SIZE`.
    pub fn get(&self, lx: i32, ly: i32) -> Option<u16> {
        if !(0..CHUNK_SIZE).contains(&lx) || !(0..CHUNK_SIZE).contains(&ly) {
            return None;
        }
        self.0.get((ly * CHUNK_SIZE + lx) as usize).copied()
    }

    pub fn words(&self) -> &[u16] {
        &self.0
    }
}

impl fmt::Debug for ChunkTiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkTiles({} tiles)", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_of_negative() {
        assert_eq!(chunk_of(TilePos::new(0, 31)), (0, 0));
        assert_eq!(chunk_of(TilePos::new(32, -1)), (1, -1));
        assert_eq!(chunk_of(TilePos::new(-33, -32)), (-2, -1));
    }

    #[test]
    fn size_checked() {
        assert!(ChunkTiles::try_from(vec![0; CHUNK_TILES]).is_ok());
        assert!(ChunkTiles::try_from(vec![0; 10]).is_err());
        let mut v = vec![0; CHUNK_TILES];
        v[33] = 7;
        let c = ChunkTiles::try_from(v).unwrap();
        assert_eq!(c.get(1, 1), Some(7));
        assert_eq!(c.get(32, 0), None);
    }
}

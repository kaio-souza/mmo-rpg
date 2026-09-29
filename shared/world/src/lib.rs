//! World geometry: tile map, what blocks what, spatial grid, line of sight
//! and pathfinding. Knows tiles and entity IDs, nothing about players or
//! combat.
//!
//!
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
#![deny(clippy::float_arithmetic)]

mod grid;
mod los;
mod map;
mod path;

pub use grid::{CELL_SIZE, SpatialGrid};
pub use los::line_of_sight;
pub use map::{
    CHUNK_SIZE, CHUNK_TILES, MapError, TILE_BLOCKS, TILE_FLOOR, TILE_HIDES, TILE_SWITCH, Tile, TileMap,
};
pub use path::{path, path_avoiding};

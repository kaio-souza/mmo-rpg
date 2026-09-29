//! Spatial grid: cells of 16×16 tiles over the zone map. A neighbor search
//! only visits the cells inside the radius.

use crate::TileMap;
use base::{EntityId, TilePos};

pub const CELL_SIZE: i32 = 16;

#[derive(Clone, Debug)]
pub struct SpatialGrid {
    origin: TilePos,
    cols: i32,
    rows: i32,
    /// Entities per cell, in insertion order (deterministic).
    cells: Vec<Vec<(EntityId, TilePos)>>,
}

impl SpatialGrid {
    pub fn new(map: &TileMap) -> SpatialGrid {
        let cols = (map.width() + CELL_SIZE - 1) / CELL_SIZE;
        let rows = (map.height() + CELL_SIZE - 1) / CELL_SIZE;
        SpatialGrid {
            origin: map.origin(),
            cols,
            rows,
            cells: vec![Vec::new(); (cols * rows) as usize],
        }
    }

    fn cell_xy(&self, p: TilePos) -> (i32, i32) {
        let axis = |v: i32, o: i32, n: i32| {
            (i64::from(v) - i64::from(o))
                .div_euclid(i64::from(CELL_SIZE))
                .clamp(0, i64::from(n - 1)) as i32
        };
        let cx = axis(p.x, self.origin.x, self.cols);
        let cy = axis(p.y, self.origin.y, self.rows);
        (cx, cy)
    }

    fn cell_mut(&mut self, p: TilePos) -> Option<&mut Vec<(EntityId, TilePos)>> {
        let (cx, cy) = self.cell_xy(p);
        self.cells.get_mut((cy * self.cols + cx) as usize)
    }

    /// Positions outside the map are kept in the nearest edge cell.
    pub fn insert(&mut self, id: EntityId, p: TilePos) {
        if let Some(c) = self.cell_mut(p) {
            c.push((id, p));
        }
    }

    pub fn remove(&mut self, id: EntityId, p: TilePos) {
        if let Some(c) = self.cell_mut(p)
            && let Some(i) = c.iter().position(|(e, _)| *e == id)
        {
            c.remove(i);
        }
    }

    pub fn moved(&mut self, id: EntityId, from: TilePos, to: TilePos) {
        if self.cell_xy(from) == self.cell_xy(to) {
            if let Some(c) = self.cell_mut(from)
                && let Some(entry) = c.iter_mut().find(|(e, _)| *e == id)
            {
                entry.1 = to;
            }
        } else {
            self.remove(id, from);
            self.insert(id, to);
        }
    }

    /// Entities within `radius` tiles (Chebyshev) of `center`, appended to
    /// `out` in a deterministic order (cells row by row, then insertion).
    pub fn neighbors(&self, center: TilePos, radius: u16, out: &mut Vec<(EntityId, TilePos)>) {
        let r = i32::from(radius);
        let (x0, y0) = self.cell_xy(TilePos::new(
            center.x.saturating_sub(r),
            center.y.saturating_sub(r),
        ));
        let (x1, y1) = self.cell_xy(TilePos::new(
            center.x.saturating_add(r),
            center.y.saturating_add(r),
        ));
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let Some(cell) = self.cells.get((cy * self.cols + cx) as usize) else {
                    continue;
                };
                out.extend(
                    cell.iter()
                        .filter(|(_, p)| p.distance(center) <= u32::from(radius)),
                );
            }
        }
    }

    /// True if some entity stands on `p` (one per tile).
    pub fn occupied(&self, p: TilePos) -> bool {
        let (cx, cy) = self.cell_xy(p);
        self.cells
            .get((cy * self.cols + cx) as usize)
            .is_some_and(|c| c.iter().any(|(_, q)| *q == p))
    }

    pub fn len(&self) -> usize {
        self.cells.iter().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(Vec::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> TileMap {
        let row = ".".repeat(64);
        let rows: Vec<&str> = (0..64).map(|_| row.as_str()).collect();
        TileMap::from_rows(TilePos::new(100, 100), &rows).unwrap()
    }

    fn ids(g: &SpatialGrid, c: TilePos, r: u16) -> Vec<u32> {
        let mut out = Vec::new();
        g.neighbors(c, r, &mut out);
        out.iter().map(|(e, _)| e.get()).collect()
    }

    #[test]
    fn insert_move_remove() {
        let mut g = SpatialGrid::new(&map());
        let a = TilePos::new(110, 110);
        g.insert(EntityId::new(1), a);
        g.insert(EntityId::new(2), TilePos::new(140, 140));
        assert_eq!(ids(&g, a, 5), [1]);
        assert_eq!(ids(&g, a, 30), [1, 2]);
        let b = TilePos::new(160, 110);
        g.moved(EntityId::new(1), a, b);
        assert_eq!(ids(&g, a, 5), Vec::<u32>::new());
        assert_eq!(ids(&g, b, 0), [1]);
        g.moved(EntityId::new(1), b, TilePos::new(161, 110)); // same cell
        assert_eq!(ids(&g, TilePos::new(161, 110), 0), [1]);
        g.remove(EntityId::new(1), TilePos::new(161, 110));
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn edge_and_outside() {
        let mut g = SpatialGrid::new(&map());
        g.insert(EntityId::new(7), TilePos::new(100, 100));
        assert_eq!(ids(&g, TilePos::new(90, 90), 10), [7]);
        assert_eq!(
            ids(&g, TilePos::new(i32::MIN, i32::MAX), u16::MAX),
            Vec::<u32>::new()
        );
    }
}

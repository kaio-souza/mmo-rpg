//! Positions and directions on the tile grid.
//!
//! Tile axes: `x` grows to the east, `y` grows to the south. Isometric
//! projection is a client concern (`client-render`).

/// Global tile coordinates in the open world (instanced zones use their own).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Tiles between two positions when moving in 8 directions (Chebyshev).
    pub fn distance(self, other: TilePos) -> u32 {
        let dx = self.x.abs_diff(other.x);
        let dy = self.y.abs_diff(other.y);
        dx.max(dy)
    }

    /// The neighbor tile in `dir`. Saturates at the edge of the coordinate space.
    pub fn step(self, dir: Direction) -> TilePos {
        let (dx, dy) = dir.delta();
        TilePos::new(self.x.saturating_add(dx), self.y.saturating_add(dy))
    }

    /// Direction of a single step from `self` toward `other`; `None` if equal.
    pub fn direction_to(self, other: TilePos) -> Option<Direction> {
        let dx = (other.x - self.x).signum();
        let dy = (other.y - self.y).signum();
        Direction::from_delta(dx, dy)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Direction {
    pub const ALL: [Direction; 8] = [
        Direction::N,
        Direction::NE,
        Direction::E,
        Direction::SE,
        Direction::S,
        Direction::SW,
        Direction::W,
        Direction::NW,
    ];

    /// Ticks one step this way takes (`STEP_TICKS`, or `WIDE_STEP_TICKS`
    /// along the screen's horizontal).
    pub const fn step_ticks(self) -> u64 {
        match self {
            Direction::NE | Direction::SW => crate::time::WIDE_STEP_TICKS,
            _ => crate::time::STEP_TICKS,
        }
    }

    pub const fn delta(self) -> (i32, i32) {
        match self {
            Direction::N => (0, -1),
            Direction::NE => (1, -1),
            Direction::E => (1, 0),
            Direction::SE => (1, 1),
            Direction::S => (0, 1),
            Direction::SW => (-1, 1),
            Direction::W => (-1, 0),
            Direction::NW => (-1, -1),
        }
    }

    pub const fn from_delta(dx: i32, dy: i32) -> Option<Direction> {
        Some(match (dx, dy) {
            (0, -1) => Direction::N,
            (1, -1) => Direction::NE,
            (1, 0) => Direction::E,
            (1, 1) => Direction::SE,
            (0, 1) => Direction::S,
            (-1, 1) => Direction::SW,
            (-1, 0) => Direction::W,
            (-1, -1) => Direction::NW,
            _ => return None,
        })
    }

    pub const fn opposite(self) -> Direction {
        match self {
            Direction::N => Direction::S,
            Direction::NE => Direction::SW,
            Direction::E => Direction::W,
            Direction::SE => Direction::NW,
            Direction::S => Direction::N,
            Direction::SW => Direction::NE,
            Direction::W => Direction::E,
            Direction::NW => Direction::SE,
        }
    }

    pub const fn is_diagonal(self) -> bool {
        let (dx, dy) = self.delta();
        dx != 0 && dy != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_chebyshev() {
        let a = TilePos::new(0, 0);
        assert_eq!(a.distance(TilePos::new(3, -2)), 3);
        assert_eq!(a.distance(a), 0);
        assert_eq!(
            TilePos::new(i32::MIN, 0).distance(TilePos::new(i32::MAX, 0)),
            u32::MAX
        );
    }

    #[test]
    fn directions_roundtrip() {
        for d in Direction::ALL {
            let (dx, dy) = d.delta();
            assert_eq!(Direction::from_delta(dx, dy), Some(d));
            assert_eq!(d.opposite().opposite(), d);
            let p = TilePos::new(5, 5);
            assert_eq!(p.step(d).step(d.opposite()), p);
            assert_eq!(p.direction_to(p.step(d)), Some(d));
        }
        assert_eq!(Direction::from_delta(0, 0), None);
    }
}

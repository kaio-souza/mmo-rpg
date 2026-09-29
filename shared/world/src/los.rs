use crate::TileMap;
use base::TilePos;

/// True when no tile strictly between `a` and `b` blocks sight (Bresenham,
/// symmetric: `los(a, b) == los(b, a)`). The endpoints never block, so a
/// wall itself can be seen.
pub fn line_of_sight(map: &TileMap, a: TilePos, b: TilePos) -> bool {
    // Walk from the smaller endpoint so the line is the same both ways.
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    // Both ends on the map: the whole line is, so the walk needs neither
    // the wide integers nor the bounds check of the general case below.
    if let (Some(f), Some(t)) = (map.local(from), map.local(to)) {
        return clear_on_map(map, f, t);
    }
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let (adx, ady) = (dx.abs(), dy.abs());
    let (sx, sy) = (dx.signum(), dy.signum());
    let mut err = adx - ady;
    let (mut x, mut y) = (i64::from(from.x), i64::from(from.y));
    let (tx, ty) = (i64::from(to.x), i64::from(to.y));
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
        if map.blocks_sight(TilePos::new(px, py)) {
            return false;
        }
    }
}

/// The same walk as [`line_of_sight`], in map coordinates, both ends on
/// the map (sides up to 4096, so `i32` never overflows).
fn clear_on_map(map: &TileMap, (fx, fy): (i32, i32), (tx, ty): (i32, i32)) -> bool {
    let (dx, dy) = (tx - fx, ty - fy);
    let (adx, ady) = (dx.abs(), dy.abs());
    let (sx, sy) = (dx.signum(), dy.signum());
    let mut err = adx - ady;
    let (mut x, mut y) = (fx, fy);
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
        if map.hides_local(x, y) {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> TileMap {
        TileMap::from_rows(
            TilePos::new(0, 0),
            &[
                ".......", //
                "...#...", "...+...", ".......",
            ],
        )
        .unwrap()
    }

    #[test]
    fn walls_block_glass_does_not() {
        let m = map();
        let p = TilePos::new;
        assert!(!line_of_sight(&m, p(0, 1), p(6, 1)));
        assert!(line_of_sight(&m, p(0, 2), p(6, 2))); // glass
        assert!(line_of_sight(&m, p(0, 0), p(6, 0)));
        assert!(line_of_sight(&m, p(0, 1), p(3, 1))); // the wall itself
        assert!(line_of_sight(&m, p(2, 2), p(2, 2)));
    }

    #[test]
    fn symmetric() {
        let m = map();
        for ax in 0..7 {
            for ay in 0..4 {
                for bx in 0..7 {
                    for by in 0..4 {
                        let (a, b) = (TilePos::new(ax, ay), TilePos::new(bx, by));
                        assert_eq!(line_of_sight(&m, a, b), line_of_sight(&m, b, a), "{a:?} {b:?}");
                    }
                }
            }
        }
    }

    /// The walk before the on-map fast path, kept to check it.
    fn reference(map: &TileMap, a: TilePos, b: TilePos) -> bool {
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        let (dx, dy) = (
            i64::from(to.x) - i64::from(from.x),
            i64::from(to.y) - i64::from(from.y),
        );
        let (adx, ady) = (dx.abs(), dy.abs());
        let (sx, sy) = (dx.signum(), dy.signum());
        let mut err = adx - ady;
        let (mut x, mut y) = (i64::from(from.x), i64::from(from.y));
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
            if x == i64::from(to.x) && y == i64::from(to.y) {
                return true;
            }
            if map.blocks_sight(TilePos::new(x as i32, y as i32)) {
                return false;
            }
        }
    }

    #[test]
    fn fast_path_matches_the_reference() {
        let m = TileMap::from_rows(
            TilePos::new(-3, 5),
            &[
                "....#....", //
                ".#.....#.",
                "...##....",
                ".........",
                "#...+..#.",
                "..#......",
            ],
        )
        .unwrap();
        for ax in -5..8 {
            for ay in 3..13 {
                for bx in -5..8 {
                    for by in 3..13 {
                        let (a, b) = (TilePos::new(ax, ay), TilePos::new(bx, by));
                        assert_eq!(line_of_sight(&m, a, b), reference(&m, a, b), "{a:?} {b:?}");
                    }
                }
            }
        }
    }
}

//! A* over 8 directions. Every step takes the same time in game, but a
//! diagonal costs a bit more here so paths prefer straight lines. A diagonal
//! cannot cut a corner (both side tiles must be free).

use crate::TileMap;
use base::{Direction, TilePos};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 11;

fn heuristic(a: TilePos, b: TilePos) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    STRAIGHT * dx.max(dy) + (DIAGONAL - STRAIGHT) * dx.min(dy)
}

/// Steps from `from` to `to` (without `from`, with `to`). `None` when `to`
/// blocks or is not reached within `max_nodes` expanded nodes.
pub fn path(map: &TileMap, from: TilePos, to: TilePos, max_nodes: u32) -> Option<Vec<TilePos>> {
    path_avoiding(map, from, to, max_nodes, |_| false)
}

/// Like [`path`], but tiles for which `avoid` is true are also closed
/// (occupied tiles near the walker). Corners are judged by the
/// map only.
pub fn path_avoiding(
    map: &TileMap,
    from: TilePos,
    to: TilePos,
    max_nodes: u32,
    avoid: impl Fn(TilePos) -> bool,
) -> Option<Vec<TilePos>> {
    if from == to {
        return Some(Vec::new());
    }
    if map.blocks_movement(to) || avoid(to) {
        return None;
    }
    // (f, h, insertion order) makes the choice between equal nodes fixed.
    let mut open: BinaryHeap<Reverse<(u32, u32, u32, TilePos)>> = BinaryHeap::new();
    let mut came: BTreeMap<TilePos, (u32, Option<TilePos>)> = BTreeMap::new();
    let mut seq = 0u32;
    came.insert(from, (0, None));
    open.push(Reverse((heuristic(from, to), heuristic(from, to), seq, from)));
    let mut expanded = 0u32;
    while let Some(Reverse((_, _, _, cur))) = open.pop() {
        if cur == to {
            let mut steps = vec![cur];
            let mut at = cur;
            while let Some((_, Some(prev))) = came.get(&at) {
                if *prev == from {
                    break;
                }
                steps.push(*prev);
                at = *prev;
            }
            steps.reverse();
            return Some(steps);
        }
        expanded += 1;
        if expanded > max_nodes {
            return None;
        }
        let g = came.get(&cur).map_or(0, |c| c.0);
        for dir in Direction::ALL {
            let next = cur.step(dir);
            if next == cur || map.blocks_movement(next) || avoid(next) {
                continue;
            }
            let cost = if dir.is_diagonal() {
                let (dx, dy) = dir.delta();
                let side_a = TilePos::new(cur.x.saturating_add(dx), cur.y);
                let side_b = TilePos::new(cur.x, cur.y.saturating_add(dy));
                if map.blocks_movement(side_a) || map.blocks_movement(side_b) {
                    continue;
                }
                DIAGONAL
            } else {
                STRAIGHT
            };
            let ng = g + cost;
            if came.get(&next).is_some_and(|(old, _)| *old <= ng) {
                continue;
            }
            came.insert(next, (ng, Some(cur)));
            seq += 1;
            let h = heuristic(next, to);
            open.push(Reverse((ng + h, h, seq, next)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y)
    }

    fn valid(map: &TileMap, from: TilePos, steps: &[TilePos]) {
        let mut at = from;
        for s in steps {
            assert_eq!(at.distance(*s), 1, "{at:?} -> {s:?}");
            assert!(!map.blocks_movement(*s));
            at = *s;
        }
    }

    #[test]
    fn around_a_wall() {
        let m = TileMap::from_rows(
            p(0, 0),
            &[
                ".....", //
                ".###.", ".....",
            ],
        )
        .unwrap();
        let steps = path(&m, p(2, 0), p(2, 2), 100).unwrap();
        valid(&m, p(2, 0), &steps);
        assert_eq!(steps.last(), Some(&p(2, 2)));
        // No corner cutting: down the left side, then along the bottom.
        assert_eq!(steps.len(), 6);
    }

    #[test]
    fn straight_and_diagonal() {
        let m = TileMap::from_rows(p(0, 0), &["....", "....", "...."]).unwrap();
        assert_eq!(
            path(&m, p(0, 0), p(3, 0), 100).unwrap(),
            [p(1, 0), p(2, 0), p(3, 0)]
        );
        assert_eq!(path(&m, p(0, 0), p(2, 2), 100).unwrap(), [p(1, 1), p(2, 2)]);
        assert_eq!(path(&m, p(1, 1), p(1, 1), 100).unwrap(), []);
    }

    #[test]
    fn no_corner_cutting() {
        let m = TileMap::from_rows(p(0, 0), &[".#", ".."]).unwrap();
        let steps = path(&m, p(0, 0), p(1, 1), 100).unwrap();
        assert_eq!(steps, [p(0, 1), p(1, 1)]);
    }

    #[test]
    fn blocked_unreachable_and_limit() {
        let m = TileMap::from_rows(p(0, 0), &["..#..", "..#..", "..#.."]).unwrap();
        assert_eq!(path(&m, p(0, 0), p(2, 0), 100), None);
        assert_eq!(path(&m, p(0, 0), p(4, 0), 1000), None);
        let open = TileMap::from_rows(p(0, 0), &["..........", ".........."]).unwrap();
        assert_eq!(path(&open, p(0, 0), p(9, 1), 3), None);
    }

    #[test]
    fn avoiding_goes_around_an_occupied_tile() {
        let map =
            TileMap::from_rows(TilePos::new(0, 0), &["#####", "#...#", "#...#", "#...#", "#####"]).unwrap();
        let busy = p(2, 2);
        let steps = path_avoiding(&map, p(1, 2), p(3, 2), 100, |t| t == busy).unwrap();
        assert!(!steps.contains(&busy));
        valid(&map, p(1, 2), &steps);
        assert_eq!(path_avoiding(&map, p(1, 2), busy, 100, |t| t == busy), None);
    }

    #[test]
    fn deterministic() {
        let m = TileMap::from_rows(p(0, 0), &["......", "......", "......", "......"]).unwrap();
        let a = path(&m, p(0, 3), p(5, 0), 100);
        for _ in 0..10 {
            assert_eq!(path(&m, p(0, 3), p(5, 0), 100), a);
        }
    }
}

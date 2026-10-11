//! Seven-segment digits for debug numbers, until fonts arrive (phase 5).

use crate::{Batch, Rgba};

// Segments: a top, b top-right, c bottom-right, d bottom, e bottom-left,
// f top-left, g middle.
const A: u8 = 1;
const B: u8 = 2;
const C: u8 = 4;
const D: u8 = 8;
const E: u8 = 16;
const F: u8 = 32;
const G: u8 = 64;

fn segments(ch: char) -> Option<u8> {
    Some(match ch {
        '0' => A | B | C | D | E | F,
        '1' => B | C,
        '2' => A | B | G | E | D,
        '3' => A | B | G | C | D,
        '4' => F | G | B | C,
        '5' => A | F | G | C | D,
        '6' => A | F | G | E | C | D,
        '7' => A | B | C,
        '8' => A | B | C | D | E | F | G,
        '9' => A | B | C | D | F | G,
        '-' => G,
        _ => return None,
    })
}

/// Draws `text` with its top-left at `pos`. Unknown characters are skipped
/// as a space; `,` and `.` draw a dot.
pub(crate) fn draw(batch: &mut Batch, text: &str, pos: (f32, f32), height: f32, color: Rgba) {
    let w = height * 0.5;
    let t = (height * 0.12).max(1.0);
    let half = height / 2.0;
    let mut x = pos.0;
    let y = pos.1;
    for ch in text.chars() {
        if ch == ',' || ch == '.' {
            batch.rect(x, y + height - t, t, t, color);
            x += t * 3.0;
            continue;
        }
        if let Some(s) = segments(ch) {
            let seg = |on: u8, batch: &mut Batch, rx: f32, ry: f32, rw: f32, rh: f32| {
                if s & on != 0 {
                    batch.rect(rx, ry, rw, rh, color);
                }
            };
            seg(A, batch, x, y, w, t);
            seg(B, batch, x + w - t, y, t, half);
            seg(C, batch, x + w - t, y + half, t, half);
            seg(D, batch, x, y + height - t, w, t);
            seg(E, batch, x, y + half, t, half);
            seg(F, batch, x, y, t, half);
            seg(G, batch, x, y + half - t / 2.0, w, t);
        }
        x += w + t * 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_has_seven_segments_and_space_none() {
        let mut b = Batch::default();
        draw(&mut b, "8", (0.0, 0.0), 20.0, Rgba::WHITE);
        assert_eq!(b.verts.len(), 7 * 6);
        b.verts.clear();
        draw(&mut b, " ", (0.0, 0.0), 20.0, Rgba::WHITE);
        assert!(b.verts.is_empty());
        draw(&mut b, "-1,", (0.0, 0.0), 20.0, Rgba::WHITE);
        assert_eq!(b.verts.len(), (1 + 2 + 1) * 6);
    }
}

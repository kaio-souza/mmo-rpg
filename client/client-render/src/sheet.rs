//! The sprite sheet: many small RGBA images packed into one texture
//! Shelf packing: images sorted
//! by height, placed left to right in rows; 1 transparent pixel between them
//! so the nearest filter never bleeds a neighbour.

/// Where one image landed: pixels in the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// A packed sheet, ready for `Render::draw`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    pub width: u32,
    pub height: u32,
    /// RGBA, row by row.
    pub pixels: Vec<u8>,
    /// Same order as the images given to [`pack`].
    pub placed: Vec<Placed>,
}

impl Sheet {
    /// Texture coordinates of an image, in the form the shader expects
    /// (`u` shifted by 2 to select the sheet).
    pub fn uv(&self, p: Placed) -> [f32; 4] {
        let (w, h) = (self.width.max(1) as f32, self.height.max(1) as f32);
        [
            2.0 + p.x as f32 / w,
            p.y as f32 / h,
            2.0 + (p.x + p.w) as f32 / w,
            (p.y + p.h) as f32 / h,
        ]
    }
}

const GAP: u32 = 1;

/// Packs `(width, height, RGBA)` images. Width grows in powers of two
/// until the rows fit in a square-ish sheet; at most 8192.
pub fn pack(images: &[(u32, u32, &[u8])]) -> Sheet {
    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(images.get(i).map_or(0, |im| im.1)));
    let area: u64 = images
        .iter()
        .map(|im| u64::from(im.0 + GAP) * u64::from(im.1 + GAP))
        .sum();
    let widest = images.iter().map(|im| im.0 + GAP).max().unwrap_or(1);
    let mut width = 64u32;
    while (u64::from(width) * u64::from(width) < area || width < widest) && width < 8192 {
        width *= 2;
    }
    let mut placed = vec![
        Placed {
            x: 0,
            y: 0,
            w: 0,
            h: 0
        };
        images.len()
    ];
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    for i in order {
        let Some(&(w, h, _)) = images.get(i) else {
            continue;
        };
        if x + w > width {
            x = 0;
            y += row + GAP;
            row = 0;
        }
        if let Some(p) = placed.get_mut(i) {
            *p = Placed { x, y, w, h };
        }
        x += w + GAP;
        row = row.max(h);
    }
    let height = (y + row).max(1).next_power_of_two();
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for (p, &(w, _, data)) in placed.iter().zip(images) {
        for row in 0..p.h {
            let src = (row * w * 4) as usize..((row + 1) * w * 4) as usize;
            let dst = (((p.y + row) * width + p.x) * 4) as usize;
            if let (Some(s), Some(d)) = (data.get(src), pixels.get_mut(dst..dst + (w * 4) as usize)) {
                d.copy_from_slice(s);
            }
        }
    }
    Sheet {
        width,
        height,
        pixels,
        placed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_do_not_overlap_and_keep_their_pixels() {
        let a = vec![255u8; 64 * 32 * 4];
        let b: Vec<u8> = (0..32 * 32 * 4).map(|i| (i % 251) as u8).collect();
        let c = vec![7u8; 64 * 64 * 4];
        let s = pack(&[(64, 32, &a), (32, 32, &b), (64, 64, &c)]);
        for (i, p) in s.placed.iter().enumerate() {
            assert!(p.x + p.w <= s.width && p.y + p.h <= s.height);
            for q in &s.placed[i + 1..] {
                let apart = p.x + p.w <= q.x || q.x + q.w <= p.x || p.y + p.h <= q.y || q.y + q.h <= p.y;
                assert!(apart, "{p:?} {q:?}");
            }
        }
        let pb = s.placed[1];
        let at = |x: u32, y: u32| (((pb.y + y) * s.width + pb.x + x) * 4) as usize;
        assert_eq!(s.pixels[at(0, 0)..at(0, 0) + 4], b[0..4]);
        assert_eq!(
            s.pixels[at(31, 31)..at(31, 31) + 4],
            b[(31 * 32 + 31) * 4..(31 * 32 + 31) * 4 + 4]
        );
        let uv = s.uv(pb);
        assert!(uv[0] >= 2.0 && uv[2] <= 3.0);
    }

    #[test]
    fn nothing_packs_to_a_tiny_sheet() {
        let s = pack(&[]);
        assert!(s.placed.is_empty() && s.width >= 1 && s.height >= 1);
    }
}

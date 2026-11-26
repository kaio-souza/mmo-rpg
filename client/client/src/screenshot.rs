//! Screenshots (F12, or `--screenshot-after SECS`): the renderer copies one
//! presented frame, saved here as a PNG in `<user folder>/screenshots/`.

use crate::interface::config_dir;
use client_render::Screenshot;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Writes `shot` as `<user folder>/screenshots/<unix seconds>.png`.
pub fn save(shot: &Screenshot) -> Result<PathBuf, String> {
    // A page has no folder to save into: the browser's own capture is there.
    if cfg!(target_arch = "wasm32") {
        return Err("screenshots are not saved in a browser".into());
    }
    let dir = config_dir()
        .ok_or("no user folder for screenshots")?
        .join("screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut path = dir.join(format!("{secs}.png"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{secs}-{n}.png"));
        n += 1;
    }
    let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), shot.width, shot.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&shot.rgba))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

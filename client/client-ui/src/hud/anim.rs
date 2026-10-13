//! Windows open and close with a short fade and a small rise (theme
//! `windows`), so nothing pops in or out in a single frame.
//!
//! No window lies on another: the
//! newest of the player's windows closes the older ones it would cover; a
//! shop, a trade, a dialogue or an invite never closes on its own, so a
//! player's window that would cover one stays hidden until it is gone.

use super::*;

/// Ease out: fast at first, settling at the end.
fn ease(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

impl HudState {
    /// How far a window is shown (0..=1, linear), or `None` when it is shut
    /// and done fading. A window that turns around mid-fade goes on from
    /// where it is.
    fn level(&mut self, f: &Frame<'_>, name: &'static str, open: bool) -> Option<f32> {
        let now = f.input.now_ms;
        let a = &f.theme.windows;
        let (open_ms, close_ms) = (u64::from(a.open_ms.max(1)), u64::from(a.close_ms.max(1)));
        let at = |open: bool, since: u64| -> f32 {
            let t = now.saturating_sub(since) as f32;
            if open {
                (t / open_ms as f32).min(1.0)
            } else {
                1.0 - (t / close_ms as f32).min(1.0)
            }
        };
        let Some(&(was, since)) = self.anim.get(name) else {
            // Never seen: a closed window was never drawn.
            if open {
                self.anim.insert(name, (true, now));
                return Some(0.0);
            }
            return None;
        };
        if was != open {
            let here = at(was, since);
            let back = if open {
                (here * open_ms as f32) as u64
            } else {
                ((1.0 - here) * close_ms as f32) as u64
            };
            self.anim.insert(name, (open, now.saturating_sub(back)));
        }
        let (open, since) = self.anim[name];
        let k = at(open, since);
        if !open && k <= 0.0 {
            self.anim.remove(name);
            return None;
        }
        Some(k)
    }

    /// Draws a window kept open by the HUD itself: fading in when it
    /// opens, and still drawn (deaf to the mouse) while it fades out.
    /// While closing or hidden the result is `R::default()`.
    pub(super) fn animated<R: Default>(
        &mut self,
        f: &mut Frame<'_>,
        name: &'static str,
        open: bool,
        draw: impl FnOnce(&mut HudState, &mut Frame<'_>) -> R,
    ) -> R {
        if open && self.hidden.contains(name) {
            self.measure(f, name, draw);
            return R::default();
        }
        let Some(k) = self.level(f, name, open) else {
            return R::default();
        };
        let (start, placed) = (f.painter.verts.len(), f.placed.len());
        let out = if open {
            draw(self, f)
        } else {
            f.without_mouse(|f| {
                draw(self, f);
            });
            R::default()
        };
        if open {
            self.keep_rect(f, name, placed);
        }
        finish(f, start, k);
        out
    }

    /// Draws a window that exists only while the model has it (shop,
    /// trade, dialogue): it fades in; when the model drops it, it is gone.
    pub(super) fn appear<R: Default>(
        &mut self,
        f: &mut Frame<'_>,
        name: &'static str,
        draw: impl FnOnce(&mut HudState, &mut Frame<'_>) -> R,
    ) -> R {
        if self.hidden.contains(name) {
            self.measure(f, name, draw);
            return R::default();
        }
        let k = self.level(f, name, true).unwrap_or(1.0);
        let (start, placed) = (f.painter.verts.len(), f.placed.len());
        let out = draw(self, f);
        self.keep_rect(f, name, placed);
        finish(f, start, k);
        out
    }

    /// A hidden window: laid out (its place is still known) but not drawn
    /// and deaf to the mouse; it fades in again once shown.
    fn measure<R>(
        &mut self,
        f: &mut Frame<'_>,
        name: &'static str,
        draw: impl FnOnce(&mut HudState, &mut Frame<'_>) -> R,
    ) {
        let (start, placed) = (f.painter.verts.len(), f.placed.len());
        let actions = f.actions.len();
        f.without_mouse(|f| {
            draw(self, f);
        });
        self.keep_rect(f, name, placed);
        f.painter.verts.truncate(start);
        f.placed.truncate(placed);
        f.actions.truncate(actions);
        self.anim.remove(name);
    }

    /// The first part a window places is the window itself.
    fn keep_rect(&mut self, f: &Frame<'_>, name: &'static str, from: usize) {
        if let Some((_, r)) = f.placed.get(from) {
            self.next_rects.insert(name, *r);
        }
    }

    /// A window the model no longer has: next time it fades in again.
    pub(super) fn gone(&mut self, name: &'static str) {
        self.anim.remove(name);
    }

    fn flag(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "inventory" => &mut self.inventory,
            "sheet" => &mut self.sheet,
            "quests" => &mut self.quests,
            "techniques" => &mut self.techniques,
            "help" => &mut self.help,
            "options" => &mut self.options,
            "crafting" => &mut self.crafting,
            _ => return None,
        })
    }

    /// Before drawing: from where the windows were last frame, the newest
    /// of the player's windows closes the older ones it covers, and the
    /// player's windows that would cover a shop, a trade, a dialogue or an
    /// invite are hidden (they never close on their own).
    pub(super) fn resolve(&mut self, f: &Frame<'_>, m: &HudModel) {
        let now = f.input.now_ms;
        for name in PLAYER_WINDOWS {
            let open = self.flag(name).is_some_and(|o| *o);
            if open {
                self.opened.entry(name).or_insert(now);
            } else {
                self.opened.remove(name);
            }
        }
        let rects = self.rects.clone();
        let rect = |n: &str| rects.get(n).copied();
        // Newest first; the same time: the order of `PLAYER_WINDOWS`.
        let mut open: Vec<(&'static str, u64)> = PLAYER_WINDOWS
            .iter()
            .filter_map(|n| Some((*n, *self.opened.get(n)?)))
            .collect();
        open.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
        let mut shut = Vec::new();
        for (i, (new, _)) in open.iter().enumerate() {
            if shut.contains(new) {
                continue;
            }
            let Some(rn) = rect(new) else { continue };
            for (old, _) in &open[i + 1..] {
                if !shut.contains(old) && rect(old).is_some_and(|ro| covers(rn, ro)) {
                    shut.push(*old);
                }
            }
        }
        for name in shut {
            if let Some(o) = self.flag(name) {
                *o = false;
            }
            self.opened.remove(name);
        }
        let fixed: Vec<&'static str> = [
            ("shop", m.shop.is_some()),
            ("trade", m.trade.is_some()),
            ("dialogue", m.dialogue.is_some()),
            ("invite", m.invite.is_some()),
        ]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(n, _)| n)
        .collect();
        self.hidden.clear();
        for name in PLAYER_WINDOWS {
            if !self.opened.contains_key(name) {
                continue;
            }
            // The backpack works with the shop and the trade, beside them.
            let partners: &[&str] = if name == "inventory" {
                &["shop", "trade"]
            } else {
                &[]
            };
            let Some(r) = rect(name) else { continue };
            if fixed
                .iter()
                .any(|x| !partners.contains(x) && rect(x).is_some_and(|rx| covers(r, rx)))
            {
                self.hidden.insert(name);
            }
        }
        // The quest tracker is the least of all: anything shown hides it.
        if let Some(t) = rect("tracker") {
            let shown = PLAYER_WINDOWS
                .iter()
                .filter(|n| self.opened.contains_key(*n) && !self.hidden.contains(*n))
                .chain(fixed.iter());
            if shown.filter_map(|n| rect(n)).any(|r| covers(t, r)) {
                self.hidden.insert("tracker");
            }
        }
    }
}

/// The windows the player opens and closes (keys, menu, Esc).
const PLAYER_WINDOWS: [&str; 7] = [
    "inventory",
    "sheet",
    "quests",
    "techniques",
    "help",
    "options",
    "crafting",
];

/// Two windows cover each other (a shared edge does not count).
pub(crate) fn covers(a: Rect, b: Rect) -> bool {
    a.x + 0.5 < b.x + b.w && b.x + 0.5 < a.x + a.w && a.y + 0.5 < b.y + b.h && b.y + 0.5 < a.y + a.h
}

fn finish(f: &mut Frame<'_>, start: usize, k: f32) {
    let e = ease(k);
    let rise = f.px(f.theme.windows.rise);
    f.painter.fade_since(start, e, (1.0 - e) * rise);
}

#[cfg(test)]
mod tests {
    use super::ease;

    #[test]
    fn the_ease_starts_fast_and_settles() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!(ease(0.5) > 0.5, "more than half the way at half the time");
    }
}

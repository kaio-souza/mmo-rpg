//! The crafting window: tabs per craft, groups per kind, a can-make switch.

use super::windows::{scroll_rows, window};
use super::*;

/// The order of the kinds in the crafting list.
const KIND_ORDER: [base::ItemKind; 6] = [
    base::ItemKind::Weapon,
    base::ItemKind::Armor,
    base::ItemKind::Offhand,
    base::ItemKind::Consumable,
    base::ItemKind::Material,
    base::ItemKind::Quest,
];

fn kind_key(k: base::ItemKind) -> &'static str {
    match k {
        base::ItemKind::Weapon => "ui.crafting.kind.weapon",
        base::ItemKind::Armor => "ui.crafting.kind.armor",
        base::ItemKind::Offhand => "ui.crafting.kind.offhand",
        base::ItemKind::Consumable => "ui.crafting.kind.consumable",
        base::ItemKind::Material => "ui.crafting.kind.material",
        base::ItemKind::Quest => "ui.crafting.kind.quest",
    }
}

/// `Iron ingot: 1/3`.
fn material(f: &Frame<'_>, name: &str, have: u32, need: u32) -> String {
    f.text.t_args(
        "ui.crafting.material",
        &[
            Arg::Str(name.to_owned()),
            Arg::Int(i64::from(have)),
            Arg::Int(i64::from(need)),
        ],
    )
}

impl HudState {
    /// The crafting window (phase 8): one tab per craft, the recipes grouped
    /// by what they make, and a switch to see only what can be made now.
    pub(super) fn crafting_window(&mut self, f: &mut Frame<'_>, m: &HudModel) -> bool {
        let groups: Vec<&str> = m.recipes.iter().fold(Vec::new(), |mut v, r| {
            if !v.contains(&r.group.as_str()) {
                v.push(r.group.as_str());
            }
            v
        });
        self.craft_tab = self.craft_tab.min(groups.len().saturating_sub(1));
        let tab = groups.get(self.craft_tab).copied();
        let shown: Vec<&RecipeModel> = m
            .recipes
            .iter()
            .filter(|r| Some(r.group.as_str()) == tab && (!self.craft_ready || r.can))
            .collect();
        let s = f.theme.sizes.clone();
        let pad = f.px(s.padding);
        let lh = f.line_height(Role::Text, s.small);
        let name_h = f.line_height(Role::Text, s.text);
        // Never reach the target panel at the top center (as the status strip).
        let w = f.px(380.0).min(f.input.screen.0 / 2.0 - f.px(130.0) - 2.0 * pad);
        // One block per recipe: the name, its materials on one line, and what
        // a click does (or why it cannot be made); a header per kind.
        let block = name_h + 2.0 * lh + pad / 2.0;
        let head = lh + pad / 2.0;
        let mut rows: Vec<Option<&RecipeModel>> = Vec::new();
        for kind in KIND_ORDER {
            let of_kind: Vec<&RecipeModel> = shown.iter().copied().filter(|r| r.kind == kind).collect();
            if !of_kind.is_empty() {
                rows.push(None);
                rows.extend(of_kind.into_iter().map(Some));
            }
        }
        let heights: Vec<f32> = rows
            .iter()
            .map(|r| if r.is_some() { block } else { head })
            .collect();
        let tabs_h = if groups.len() > 1 { lh + pad } else { 0.0 };
        let title_h = f.line_height(Role::Title, s.title);
        let h = title_h + tabs_h + lh + pad + heights.iter().sum::<f32>().max(lh) + 4.0 * pad;
        let top = under_status(f);
        let r = Rect::new(pad, top, w, h.min(f.input.screen.1 - top - f.px(190.0)));
        let (mut y, closed) = window(f, r, "ui.crafting.title", Some(&key_of(m, MenuWindow::Crafting)));
        let c = f.theme.colors.clone();
        if m.recipes.is_empty() {
            f.label_fit(
                Role::Text,
                s.small,
                (r.x + pad, y),
                r.w - 2.0 * pad,
                c.dim_text,
                &m.no_recipes,
            );
            return closed;
        }
        // The crafts as tabs, with how many recipes each has.
        if groups.len() > 1 {
            let each = (r.w - 2.0 * pad - (groups.len() - 1) as f32 * pad / 4.0) / groups.len() as f32;
            for (i, g) in groups.iter().enumerate() {
                let n = m.recipes.iter().filter(|x| x.group == *g).count();
                let tr = Rect::new(r.x + pad + i as f32 * (each + pad / 4.0), y, each, lh + pad / 2.0);
                let hover = tr.contains(f.input.mouse);
                let on = i == self.craft_tab;
                f.panel(tr, hover || on);
                let label = format!("{g} {n}");
                let color = if on { c.highlight } else { c.dim_text };
                f.label_center(Role::Text, s.small, tr, color, &label);
                if hover && f.input.clicked && !on {
                    self.craft_tab = i;
                    self.scroll[Scroll::Crafting as usize] = 0;
                }
            }
            y += tabs_h;
        }
        // "Only what I can make".
        let mark = if self.craft_ready { "[x]" } else { "[ ]" };
        let label = format!("{mark} {}", f.tr("ui.crafting.ready_only"));
        let tr = Rect::new(r.x + pad, y, r.w - 2.0 * pad, lh);
        let hover = tr.contains(f.input.mouse);
        f.label_fit(
            Role::Text,
            s.small,
            (tr.x, tr.y),
            tr.w,
            if hover || self.craft_ready {
                c.highlight
            } else {
                c.dim_text
            },
            &label,
        );
        if hover && f.input.clicked {
            self.craft_ready = !self.craft_ready;
            self.scroll[Scroll::Crafting as usize] = 0;
        }
        y += lh + pad;
        if rows.is_empty() {
            let none = f.tr("ui.crafting.none_ready");
            f.label_fit(
                Role::Text,
                s.small,
                (r.x + pad, y),
                r.w - 2.0 * pad,
                c.dim_text,
                &none,
            );
            return closed;
        }
        let area = Rect::new(r.x + pad, y, r.w - 2.0 * pad, r.y + r.h - pad - y);
        let range = scroll_rows(f, area, &heights, &mut self.scroll[Scroll::Crafting as usize]);
        let mut tip = None;
        for i in range {
            let Some(recipe) = rows[i] else {
                // The header of the kind that follows.
                if let Some(Some(next)) = rows.get(i + 1) {
                    let label = f.tr(kind_key(next.kind));
                    f.label_fit(
                        Role::Title,
                        s.small,
                        (area.x, y + pad / 4.0),
                        area.w,
                        c.system,
                        &label,
                    );
                }
                y += head;
                continue;
            };
            let icon_side = block - pad;
            let whole = Rect::new(area.x, y, area.w - pad / 2.0, block - pad / 2.0);
            let hover = whole.contains(f.input.mouse);
            if hover {
                f.panel(whole, recipe.can);
            }
            let ir = Rect::new(area.x, y + pad / 4.0, icon_side, icon_side);
            if let Some(uv) = recipe.icon {
                let k = if recipe.can { 1.0 } else { 0.5 };
                f.painter.image(ir, uv, [k, k, k, 1.0]);
            }
            let tx = ir.x + icon_side + pad / 2.0;
            let tw = whole.w - icon_side - pad;
            let mut color = c.of_rarity(recipe.rarity);
            if !recipe.can {
                color[3] *= 0.55;
            }
            let name = if recipe.qty > 1 {
                format!("{} x{}", recipe.name, recipe.qty)
            } else {
                recipe.name.clone()
            };
            f.label_fit(Role::Text, s.text, (tx, y), tw, color, &name);
            // Materials on one line; red when one is short.
            let short = recipe.materials.iter().any(|(_, have, need)| have < need);
            let line = recipe
                .materials
                .iter()
                .map(|(name, have, need)| material(f, name, *have, *need))
                .collect::<Vec<_>>()
                .join(" · ");
            f.label_fit(
                Role::Text,
                s.small,
                (tx, y + name_h),
                tw,
                if short { c.error } else { c.text },
                &line,
            );
            let foot = if recipe.can {
                f.tr("ui.crafting.click")
            } else {
                recipe.blocked.clone()
            };
            f.label_fit(Role::Text, s.small, (tx, y + name_h + lh), tw, c.dim_text, &foot);
            if m.pending.recipe == Some(recipe.id) {
                pulse(f, whole);
            }
            if hover {
                let mut lines = vec![(name, c.of_rarity(recipe.rarity))];
                if !recipe.description.is_empty() {
                    lines.push((recipe.description.clone(), c.dim_text));
                }
                for (n, have, need) in &recipe.materials {
                    let color = if have < need { c.error } else { c.text };
                    lines.push((material(f, n, *have, *need), color));
                }
                tip = Some(lines);
                if recipe.can && f.input.clicked {
                    f.actions.push(UiAction::Craft(recipe.id));
                }
            }
            y += block;
        }
        if let Some(lines) = tip {
            overlay::tooltip(f, &lines);
        }
        closed
    }
}

//! Content problems as text keys for the editor (P12).
//!
//! Every loader message stays an English sentence (the `validate` tool
//! prints it). [`problem_parts`] cuts a message at `": "` and recognizes
//! each piece by [`TEMPLATES`]: a key and named arguments, which the editor
//! shows in its own language (`editor.problem.<key>`). A piece no template
//! knows stays as it is. An argument is recognized the same way, so
//! `region 0 (Safe) is in the seam band of ...` nests the region.

/// One piece of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProblemPart {
    /// Not recognized: shown as written.
    Text(String),
    /// `editor.problem.<key>` with its arguments.
    Coded {
        key: &'static str,
        args: Vec<(&'static str, ProblemPart)>,
    },
}

/// `(key, English piece)`; `{name}` stands for any text. A new loader
/// message adds its line here and its key to the editor's texts; one left
/// out still shows, in English.
pub const TEMPLATES: &[(&str, &str)] = &[
    ("error", "error"),
    ("warning", "warning"),
    ("entry", "entry {n}"),
    ("row_column", "row {y} column {x}"),
    ("region", "region {n} ({kind})"),
    ("option_of_node", "node `{node}` option {n}"),
    ("node_named", "node `{node}`"),
    ("item_named", "item `{id}`"),
    ("the_spawn", "the spawn"),
    ("dense_spawn", "the dense spawn of `{id}`"),
    ("seam_band", "{what} is in the seam band of `{zone}` ({n} tiles)"),
    ("overlaps_zone", "overlaps zone `{zone}`"),
    (
        "starting_zone",
        "exactly one zone needs `starting: true`, found {n}",
    ),
    ("missing_file", "missing {file}"),
    ("not_in_legend", "'{c}' is not in the legend"),
    ("row_width", "row {y} has {n} tiles, expected {width}"),
    ("min_max", "min must be <= max"),
    ("view_radius", "view_radius must be 1..={max}"),
    ("spawn_blocked", "spawn is on a blocking tile"),
    ("spawn_outside", "spawn is outside the map"),
    ("outside_map", "`{id}` is outside the map"),
    ("on_blocking", "`{id}` is on a blocking tile"),
    ("on_npc", "`{id}` is on a tile taken by another NPC"),
    (
        "monster_spot",
        "`{id}` outside the map, on a blocking tile or count 0",
    ),
    (
        "node_spot",
        "`{id}` has a bad id, is outside the map, on a blocking tile or on a taken tile",
    ),
    ("bad_id", "{kind} id `{id}` must be [a-z0-9_]"),
    ("bad_name", "{kind} `{id}` must be [a-z0-9_]"),
    ("no_text_key", "no text key `{key}`"),
    ("item_not_found", "item `{id}` not found"),
    ("npc_not_found", "npc `{id}` not found"),
    ("monster_not_found", "monster `{id}` not found"),
    ("node_not_found", "node `{id}` not found"),
    ("dialogue_not_found", "dialogue `{id}` not found"),
    ("shop_not_found", "shop `{id}` not found"),
    ("quest_not_found", "quest `{id}` not found"),
    ("proficiency_not_found", "proficiency `{id}` not found"),
    ("technique_not_found", "technique `{id}` not found"),
    ("recipe_not_found", "recipe `{id}` not found"),
    ("loot_table_not_found", "loot table `{id}` not found"),
    ("material_not_found", "material `{id}` not found"),
    ("result_not_found", "result item `{id}` not found"),
    ("twice", "item `{id}` twice"),
    ("not_a_node", "next `{id}` is not a node"),
    ("start_not_a_node", "start `{id}` is not a node"),
    ("too_many_options", "node `{node}` has {n} options, at most {max}"),
    ("qty_min", "qty must be at least 1"),
    ("value_zero", "item `{id}` has value 0"),
    ("give_value", "give it a value before selling it"),
    ("only_equipment_durability", "only equipment has durability"),
    ("only_equipment_powers", "only equipment has powers"),
    ("only_equipment_light", "only equipment gives light"),
    ("only_equipment_hidden", "only equipment has hidden attributes"),
    ("only_consumable_use", "only kind Consumable has `use`"),
    ("needs_equip", "kinds Weapon, Armor and Offhand need `equip`"),
    ("own_identity", "equipment has its own identity"),
    ("max_stack_one", "max_stack must be 1"),
    ("max_stack_range", "max_stack {n} must be 1 to {max}"),
    ("cooldown_min", "cooldown_ticks must be >= 1"),
    ("objectives", "needs 1 to {max} objectives"),
    ("icon_file", "icon `{id}` has no file in assets/icons/"),
    ("sprite_file", "sprite `{id}` has no file in assets/sprites/"),
    ("credit_missing", "the credit (author) is missing"),
    (
        "no_source",
        "no way into the game (no drop, shop, node, quest, dialogue or recipe with sourced materials)",
    ),
    ("same_as_en", "`{key}` is the same as in en"),
    ("latin_letters", "`{key}` is all in Latin letters"),
    ("copied", "copied, not translated?"),
    ("icon_erased", "icon is empty but assets/icons/{id}.png exists"),
];

/// The pieces of a loader message, in order (joined with `": "`). A
/// `": "` between backticks (`` `starting: true` ``) does not cut.
pub fn problem_parts(message: &str) -> Vec<ProblemPart> {
    let mut parts = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in message.char_indices() {
        match c {
            '`' => quoted = !quoted,
            ':' if !quoted && message[i..].starts_with(": ") => {
                parts.push(piece(&message[start..i]));
                start = i + 2;
            }
            _ => {}
        }
    }
    parts.push(piece(&message[start..]));
    parts
}

/// The template with the most fixed text wins, so `region {n} ({kind})`
/// does not swallow a longer sentence that starts with a region.
fn piece(text: &str) -> ProblemPart {
    let best = TEMPLATES
        .iter()
        .filter_map(|(key, template)| Some((key, template, fill(template, text)?)))
        .max_by_key(|(_, template, found)| {
            template.len() - found.iter().map(|(name, _)| name.len() + 2).sum::<usize>()
        });
    match best {
        Some((key, _, found)) => ProblemPart::Coded {
            key,
            args: found.into_iter().map(|(n, v)| (n, piece(v))).collect(),
        },
        None => ProblemPart::Text(text.to_owned()),
    }
}

/// The arguments when `text` is `template` with each `{name}` filled by
/// at least one character.
fn fill<'a>(template: &'static str, text: &'a str) -> Option<Vec<(&'static str, &'a str)>> {
    let Some(open) = template.find('{') else {
        return (template == text).then(Vec::new);
    };
    let (head, rest) = template.split_at(open);
    let text = text.strip_prefix(head)?;
    let close = rest.find('}')?;
    let name = &rest[1..close];
    let after = &rest[close + 1..];
    // The value ends where the rest of the template matches; try the
    // shortest value first.
    for (end, _) in text.char_indices().skip(1).chain([(text.len(), ' ')]) {
        if let Some(mut more) = fill(after, &text[end..]) {
            more.insert(0, (name, &text[..end]));
            return Some(more);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coded(key: &'static str, args: Vec<(&'static str, ProblemPart)>) -> ProblemPart {
        ProblemPart::Coded { key, args }
    }

    fn text(s: &str) -> ProblemPart {
        ProblemPart::Text(s.to_owned())
    }

    #[test]
    fn a_message_becomes_keys_with_nested_arguments() {
        let parts =
            problem_parts("zones/aurelia: region 0 (Safe) is in the seam band of `testania` (8 tiles)");
        assert_eq!(
            parts,
            vec![
                text("zones/aurelia"),
                coded(
                    "seam_band",
                    vec![
                        (
                            "what",
                            coded("region", vec![("n", text("0")), ("kind", text("Safe"))])
                        ),
                        ("zone", text("testania")),
                        ("n", text("8")),
                    ]
                ),
            ]
        );
        assert_eq!(
            problem_parts("npcs.ron: entry 2: npc `guard` not found")[1..],
            [
                coded("entry", vec![("n", text("2"))]),
                coded("npc_not_found", vec![("id", text("guard"))]),
            ]
        );
        assert_eq!(problem_parts("something new"), vec![text("something new")]);
    }

    #[test]
    fn keys_are_unique_and_templates_well_formed() {
        let mut keys: Vec<&str> = TEMPLATES.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), TEMPLATES.len());
        for (k, t) in TEMPLATES {
            assert!(k.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{k}");
            assert_eq!(t.matches('{').count(), t.matches('}').count(), "{k}");
        }
    }
}

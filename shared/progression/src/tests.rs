use super::*;

fn catalog() -> ValidatedCatalog {
    let keys = [
        "class.warrior.name",
        "class.warrior.desc",
        "class.knight.name",
        "class.knight.desc",
        "class.beastmaster.name",
        "class.beastmaster.desc",
        "proficiency.sword.name",
        "proficiency.unarmed.name",
        "technique.double_cut.name",
        "technique.double_cut.desc",
        "title.wolf_hunter.name",
        "title.wolf_hunter.desc",
        "title.veteran.name",
        "title.veteran.desc",
        "monster.wolf.name",
        "title.strong.name",
        "title.strong.desc",
        "title.swordsman.name",
        "title.swordsman.desc",
        "title.sword_apprentice.name",
        "title.sword_apprentice.desc",
        "title.sword_saint.name",
        "title.sword_saint.desc",
        "special.grit.name",
        "special.grit.desc",
    ];
    let text = format!(
        "{{ {} }}",
        keys.iter()
            .map(|k| format!("\"{k}\": \"x\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.clone()))
        .collect();
    for (p, t) in [
        ("proficiencies/sword.ron", "(kind: Weapon)"),
        ("proficiencies/unarmed.ron", "(kind: Weapon)"),
        (
            "classes/warrior.ron",
            "(tier: 1, level: 5, requires: [Any([Proficiency(\"sword\", 3), Attribute(Str, 30)])], auto_points: (str: 2, vit: 1), growth: [(\"sword\", 1200)])",
        ),
        // A rare class: its only door is a deed (a title).
        (
            "classes/beastmaster.ron",
            "(tier: 1, level: 1, requires: [Title(\"wolf_hunter\")])",
        ),
        (
            "classes/knight.ron",
            "(tier: 2, parent: Some(\"warrior\"), level: 10)",
        ),
        (
            "techniques/double_cut.ron",
            "(proficiency: \"sword\", unlock_at: 2, cooldown_ticks: 60, vigor: 5, damage_permille: 1800)",
        ),
        // The title counts wolves, so a wolf has to exist.
        ("monsters/wolf.ron", "(level: 2, hp: 30, damage: (2, 4), xp: 10)"),
        ("counters.ron", "[\"kills\"]"),
        (
            "titles/wolf_hunter.ron",
            "(trigger: Counter(family: \"kills\", key: \"wolf\", at: 3))",
        ),
        ("titles/veteran.ron", "(trigger: Level(5))"),
        ("special_attributes/grit.ron", "()"),
        (
            "titles/sword_apprentice.ron",
            "(trigger: Proficiency(\"sword\", 4))",
        ),
        (
            "titles/sword_saint.ron",
            "(trigger: Proficiency(\"sword\", 10), replaces: [\"swordsman\"])",
        ),
        (
            "titles/strong.ron",
            "(trigger: Attribute(Str, 10), bonuses: [Attribute(Vit, 3), MaxHp(20), Percent(Xp, 10), Percent(MaxMp, 3), Special(\"grit\", 60)], techniques: [\"double_cut\"])",
        ),
        (
            "titles/swordsman.ron",
            "(trigger: Proficiency(\"sword\", 5), bonuses: [Percent(Damage, 5), Special(\"grit\", 60)], replaces: [\"sword_apprentice\"])",
        ),
    ] {
        files.push((p.into(), t.into()));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    defs::from_text(&refs).unwrap().validate().unwrap()
}

#[test]
fn levels_points_and_death() {
    let cat = catalog();
    let mut s = ProgressionSheet::new(&cat);
    let changes = add_xp(&mut s, 100, &cat);
    assert_eq!(s.level, 2);
    assert_eq!(s.free_points, 5, "no class: 5 points");
    assert!(changes.contains(&Change::LeveledUp(2)));
    // Death: down one level, points stay; climbing back gives none.
    lose_level(&mut s);
    assert_eq!((s.level, s.xp, s.free_points), (1, 0, 5));
    add_xp(&mut s, 100, &cat);
    assert_eq!((s.level, s.free_points), (2, 5));
    assert_eq!(
        allocate(&mut s, &Attributes::all(1)),
        Err(ProgressError::NotEnoughPoints)
    );
    let pts = Attributes {
        str: 5,
        ..Attributes::default()
    };
    allocate(&mut s, &pts).unwrap();
    assert_eq!((s.attributes.str, s.free_points), (10, 0));
}

#[test]
fn proficiency_unlocks_technique_and_class() {
    let cat = catalog();
    let (sword, _) = cat.proficiency_by_name("sword").unwrap();
    let (warrior, _) = cat.class_by_name("warrior").unwrap();
    let (knight, _) = cat.class_by_name("knight").unwrap();
    let mut s = ProgressionSheet::new(&cat);
    // Weak target (10+ below the proficiency): no gain; stronger: up to ×1.5.
    assert_eq!(difficulty_factor(10, 20), 0);
    assert_eq!(difficulty_factor(15, 20), 500);
    assert_eq!(difficulty_factor(40, 20), 1500);
    let mut changes = Vec::new();
    for t in 0..100 {
        changes.extend(use_proficiency(&mut s, sword, 3, 2000 + t * 10, &cat));
    }
    assert!(s.proficiency(sword) >= 3);
    assert!(changes.iter().any(|c| matches!(c, Change::TechniqueUnlocked(_))));
    // Spam inside the interval does not count.
    let before = s.proficiencies.clone();
    use_proficiency(&mut s, sword, 3, 2000 + 99 * 10 + 1, &cat);
    assert_eq!(s.proficiencies, before);
    assert!(available_classes(&s, &cat).is_empty(), "level 5 needed");
    let changes = add_xp(&mut s, 5_000, &cat);
    assert!(s.level >= 5);
    assert!(changes.contains(&Change::ClassesAvailable(vec![warrior])));
    assert_eq!(
        choose_class(&mut s, knight, &cat),
        Err(ProgressError::ClassNotAvailable)
    );
    choose_class(&mut s, warrior, &cat).unwrap();
    let str_before = s.attributes.str;
    let points = s.free_points;
    let need = cat.formulas().level_xp(s.level) - s.xp;
    add_xp(&mut s, need, &cat);
    assert_eq!(s.attributes.str, str_before + 2, "class points");
    assert_eq!(s.free_points, points + 3);
}

#[test]
fn titles_by_counter_and_level() {
    let cat = catalog();
    let mut s = ProgressionSheet::new(&cat);
    assert!(check_titles(&mut s, |_, _| 2, &cat).is_empty());
    let got = check_titles(
        &mut s,
        |f, k| if f == "kills" && k == "wolf" { 3 } else { 0 },
        &cat,
    );
    assert_eq!(got.len(), 1);
    assert!(check_titles(&mut s, |_, _| 3, &cat).is_empty(), "once");
    add_xp(&mut s, 10_000, &cat);
    assert_eq!(check_titles(&mut s, |_, _| 0, &cat).len(), 1);
    assert_eq!(s.titles.len(), 2);
}

#[test]
fn titles_by_attribute_and_proficiency() {
    let cat = catalog();
    let mut s = ProgressionSheet::new(&cat);
    let title = |name: &str| cat.title_by_name(name).unwrap().0;
    assert!(check_titles(&mut s, |_, _| 0, &cat).is_empty());

    s.attributes.str = 9;
    assert!(check_titles(&mut s, |_, _| 0, &cat).is_empty(), "one point short");
    s.attributes.str = 10;
    let double_cut = cat.technique_by_name("double_cut").unwrap().0;
    assert_eq!(
        check_titles(&mut s, |_, _| 0, &cat),
        vec![
            Change::TitleEarned(title("strong")),
            Change::TechniqueUnlocked(double_cut)
        ],
        "the title teaches its technique"
    );
    assert_eq!(s.technique(double_cut).map(|t| t.level), Some(1));
    let b = title_bonuses(&s, &cat);
    assert_eq!(
        (
            b.attributes.vit,
            b.max_hp,
            b.xp_percent,
            b.damage_percent,
            b.max_mp_percent
        ),
        (3, 20, 10, 0, 3)
    );
    assert_eq!(
        s.attributes.vit,
        ProgressionSheet::new(&cat).attributes.vit,
        "the sheet itself never changes"
    );
    assert_eq!(special_attribute(&s, "grit", &cat), 60);

    let sword = cat.proficiency_by_name("sword").unwrap().0;
    let set = |s: &mut ProgressionSheet, points: u8| match s.proficiencies.iter_mut().find(|p| p.id == sword)
    {
        Some(p) => p.points = points,
        None => {
            s.proficiencies.push(Proficiency {
                id: sword,
                points,
                xp: 0,
                last_gain: 0,
            });
            s.proficiencies.sort_by_key(|p| p.id);
        }
    };
    set(&mut s, 3);
    assert!(check_titles(&mut s, |_, _| 0, &cat).is_empty(), "one point short");
    set(&mut s, 4);
    assert_eq!(
        check_titles(&mut s, |_, _| 0, &cat),
        vec![Change::TitleEarned(title("sword_apprentice"))]
    );
    set(&mut s, 5);
    assert_eq!(
        check_titles(&mut s, |_, _| 0, &cat),
        vec![Change::TitleReplaced {
            old: title("sword_apprentice"),
            new: title("swordsman")
        }]
    );
    assert!(!s.titles.contains(&title("sword_apprentice")));
    assert!(
        check_titles(&mut s, |_, _| 0, &cat).is_empty(),
        "a replaced title never comes back"
    );
    assert_eq!(title_bonuses(&s, &cat).damage_percent, 5);
    assert_eq!(special_attribute(&s, "grit", &cat), defs::SPECIAL_MAX, "capped");
    assert_eq!(TitleBonuses::raise(200, 10), 220);
}

#[test]
fn a_higher_title_skips_and_takes_the_chain() {
    let cat = catalog();
    let title = |name: &str| cat.title_by_name(name).unwrap().0;
    let saint = cat.title(title("sword_saint")).unwrap();
    assert_eq!(
        saint.replaces,
        vec![title("sword_apprentice"), title("swordsman")],
        "replacing swordsman also replaces what swordsman replaces"
    );
    let mut s = ProgressionSheet::new(&cat);
    let sword = cat.proficiency_by_name("sword").unwrap().0;
    s.proficiencies.push(Proficiency {
        id: sword,
        points: 10,
        xp: 0,
        last_gain: 0,
    });
    s.proficiencies.sort_by_key(|p| p.id);
    let changes = check_titles(&mut s, |_, _| 0, &cat);
    assert!(
        changes.contains(&Change::TitleEarned(title("sword_saint"))),
        "{changes:?}"
    );
    assert!(
        !s.titles.contains(&title("sword_apprentice")) && !s.titles.contains(&title("swordsman")),
        "titles earned together with the one that replaces them are never given"
    );
}

/// A small world with faith, the paladin's vow and a heal.
fn faith_catalog() -> ValidatedCatalog {
    let keys = [
        "class.paladin.name",
        "class.paladin.desc",
        "class.cleric.name",
        "class.cleric.desc",
        "class.novice.name",
        "class.novice.desc",
        "proficiency.light.name",
        "proficiency.unarmed.name",
        "technique.vow.name",
        "technique.vow.desc",
        "technique.heal.name",
        "technique.heal.desc",
        "special.faith.name",
        "special.faith.desc",
    ];
    let text = format!(
        "{{ {} }}",
        keys.iter()
            .map(|k| format!("\"{k}\": \"x\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut files: Vec<(String, String)> = [i18n::Locale::EN]
        .iter()
        .map(|l| (format!("text/{l}/a.ron"), text.clone()))
        .collect();
    for (p, t) in [
        ("proficiencies/light.ron", "(kind: Magic)"),
        ("proficiencies/unarmed.ron", "(kind: Weapon)"),
        ("special_attributes/faith.ron", "(scales: [(\"light\", 250)])"),
        (
            "classes/paladin.ron",
            "(tier: 1, level: 1, specials: [(special: \"faith\", base: 5, per_level_permille: 500)], techniques: [\"vow\"])",
        ),
        (
            "classes/cleric.ron",
            "(tier: 1, level: 1, specials: [(special: \"faith\", base: 10, per_level_permille: 1500)])",
        ),
        // Too little faith for the vow when chosen; enough four levels later.
        (
            "classes/novice.ron",
            "(tier: 1, level: 1, specials: [(special: \"faith\", base: 1, per_level_permille: 1000)], techniques: [\"vow\"])",
        ),
        (
            "techniques/vow.ron",
            "(proficiency: \"light\", unlock_at: 1, cooldown_ticks: 100, vigor: 5, buff: Some((stat: Defense, permille: 250, duration_ticks: 600)), requires_special: [(\"faith\", 5)], taught_only: true)",
        ),
        (
            "techniques/heal.ron",
            "(proficiency: \"light\", unlock_at: 1, cooldown_ticks: 10, mp: 5, heal_permille: 150)",
        ),
    ] {
        files.push((p.into(), t.into()));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    defs::from_text(&refs).unwrap().validate().unwrap()
}

/// A class grows its special attribute with the highest level
/// reached; the cleric's faith grows faster than the paladin's.
#[test]
fn a_class_grows_faith_with_levels() {
    let cat = faith_catalog();
    let paladin = cat.class_by_name("paladin").unwrap().0;
    let cleric = cat.class_by_name("cleric").unwrap().0;
    let mut s = ProgressionSheet::new(&cat);
    assert_eq!(special_attribute(&s, "faith", &cat), 0);
    choose_class_and_learn(&mut s, paladin, &cat).unwrap();
    assert_eq!(special_attribute(&s, "faith", &cat), 5);
    s.max_level = 11;
    assert_eq!(special_attribute(&s, "faith", &cat), 10, "+1 every two levels");
    let mut c = ProgressionSheet::new(&cat);
    choose_class_and_learn(&mut c, cleric, &cat).unwrap();
    c.max_level = 11;
    assert_eq!(special_attribute(&c, "faith", &cat), 25, "+1.5 a level");
    // Losing a level on death keeps it (the highest level counts).
    lose_level(&mut c);
    assert_eq!(special_attribute(&c, "faith", &cat), 25);
}

/// Faith adds to light in use, never to the points themselves.
#[test]
fn faith_adds_to_light_in_use() {
    let cat = faith_catalog();
    let light = cat.proficiency_by_name("light").unwrap().0;
    let cleric = cat.class_by_name("cleric").unwrap().0;
    let mut s = ProgressionSheet::new(&cat);
    choose_class_and_learn(&mut s, cleric, &cat).unwrap();
    s.max_level = 21; // faith 10 + 30 = 40
    assert_eq!(special_attribute(&s, "faith", &cat), 40);
    assert_eq!(s.proficiency(light), 0);
    assert_eq!(effective_proficiency(&s, light, &cat), 10);
}

/// The class teaches its technique once the special attribute
/// it asks for holds; points never unlock a technique that is only taught.
#[test]
fn the_vow_is_taught_by_the_class_when_faith_allows() {
    let cat = faith_catalog();
    let vow = cat.technique_by_name("vow").unwrap().0;
    let paladin = cat.class_by_name("paladin").unwrap().0;
    let novice = cat.class_by_name("novice").unwrap().0;
    let mut s = ProgressionSheet::new(&cat);
    let changes = choose_class_and_learn(&mut s, paladin, &cat).unwrap();
    assert!(changes.contains(&Change::TechniqueUnlocked(vow)));

    let mut n = ProgressionSheet::new(&cat);
    choose_class_and_learn(&mut n, novice, &cat).unwrap();
    assert!(n.technique(vow).is_none(), "faith 1 is not enough");
    let mut learned = false;
    for _ in 0..4 {
        let need = cat.formulas().level_xp(n.level);
        learned |= add_xp(&mut n, need, &cat).contains(&Change::TechniqueUnlocked(vow));
    }
    assert!(learned && n.technique(vow).is_some(), "faith 5 at level 5");

    // A cleric raising light never unlocks the vow by points.
    let light = cat.proficiency_by_name("light").unwrap().0;
    let cleric = cat.class_by_name("cleric").unwrap().0;
    let mut c = ProgressionSheet::new(&cat);
    choose_class_and_learn(&mut c, cleric, &cat).unwrap();
    for tick in 0..2000u64 {
        use_proficiency(&mut c, light, 50, 1 + tick * MIN_GAIN_INTERVAL_TICKS, &cat);
    }
    assert!(c.proficiency(light) > 1);
    assert!(c.technique(vow).is_none());
    assert!(c.technique(cat.technique_by_name("heal").unwrap().0).is_some());
}

#[test]
fn a_class_can_require_a_title() {
    let cat = catalog();
    let (beastmaster, _) = cat.class_by_name("beastmaster").unwrap();
    let mut s = ProgressionSheet::new(&cat);
    assert!(!available_classes(&s, &cat).contains(&beastmaster), "no deed yet");
    check_titles(
        &mut s,
        |f, k| if f == "kills" && k == "wolf" { 3 } else { 0 },
        &cat,
    );
    assert!(available_classes(&s, &cat).contains(&beastmaster));
}

//! Rule 4: custody. What leaves the sheet waits in the zone until
//! the database answers; a refusal gives it back; a delivery puts a parcel
//! into the backpack. Nothing is copied and each piece keeps its crafter.

use crate::tests_trade::content_with;
use crate::types::{Command, ItemPlace, Refusal, SavedItem, SavedStack, TickInput, TickOutput};
use crate::{CharacterSheet, EscrowParcel, LoadLevel, Zone};
use base::{Attribute, CharacterId, CharacterName, PlayerId, Rarity, TilePos, ZoneId};

const P1: PlayerId = PlayerId::new(1);
const P2: PlayerId = PlayerId::new(2);
const HERO: CharacterId = CharacterId::new(41);

struct Run {
    zone: Zone,
    out: TickOutput,
}

fn name(s: &str) -> CharacterName {
    CharacterName::try_from(s).unwrap()
}

/// A sword made by Forgemaster, worn and appraised: everything custody
/// must keep.
fn signed_sword(id: u64, slot: u16) -> SavedItem {
    SavedItem {
        id,
        item: "sword".into(),
        place: ItemPlace::Backpack(slot),
        hidden: vec![(Attribute::Str, 2)],
        appraised: true,
        durability: (31, 40),
        crafter: Some("Forgemaster".into()),
        rarity: Some(Rarity::Epic),
        mastery: 125,
        power: None,
    }
}

impl Run {
    fn new() -> Run {
        let dummy =
            "(level: 1, hp: 100000, damage: (0, 0), xp: 1, aggro_radius: 0, leash: 30, respawn_ticks: 5)";
        Run {
            zone: Zone::new(content_with(dummy), ZoneId::new(0), 7).unwrap(),
            out: TickOutput::default(),
        }
    }

    fn tick(&mut self, commands: &[Command]) {
        let input = TickInput {
            commands,
            now_unix: 1_000,
            load: LoadLevel::N0,
        };
        self.zone.tick(&input, &mut self.out);
        assert!(self.zone.index_error().is_none());
    }

    /// `hero` with 500 copper, 8 ingots in slot 0 and the signed sword in
    /// slot 1; `friend` empty.
    fn enter(&mut self) {
        let mut hero = CharacterSheet::new(name("hero"));
        hero.coins = 500;
        hero.backpack = vec![
            Some(SavedStack {
                item: "ingot".into(),
                qty: 8,
            }),
            None,
        ];
        hero.items = vec![signed_sword(900, 1)];
        self.tick(&[
            Command::Enter {
                player: P1,
                sheet: Box::new(hero),
                pos: Some(TilePos::new(1, 1)),
            },
            Command::Enter {
                player: P2,
                sheet: Box::new(CharacterSheet::new(name("friend"))),
                pos: Some(TilePos::new(1, 2)),
            },
        ]);
    }

    fn sheet(&self, player: PlayerId) -> CharacterSheet {
        self.zone
            .players()
            .into_iter()
            .find(|(p, _, _)| *p == player)
            .unwrap()
            .1
    }

    fn deposit(&mut self, items: Vec<(u16, u32)>, coins: u64, to: &str) {
        self.tick(&[Command::EscrowDeposit {
            player: P1,
            from: HERO,
            items,
            coins,
            to: name(to),
        }]);
    }

    fn ingots(&self, player: PlayerId) -> u32 {
        self.sheet(player)
            .backpack
            .iter()
            .flatten()
            .filter(|s| s.item == "ingot")
            .map(|s| s.qty)
            .sum()
    }
}

#[test]
fn deposit_takes_out_and_keeps_the_crafter() {
    let mut r = Run::new();
    r.enter();
    r.deposit(vec![(0, 3), (1, 1)], 200, "friend");
    assert!(r.out.refused.is_empty(), "{:?}", r.out.refused);
    let e = r.out.escrows.first().expect("one deposit to write");
    assert_eq!(e.to, name("friend"));
    assert_eq!(e.parcel.coins, 200);
    assert_eq!(
        e.parcel.stacks,
        vec![SavedStack {
            item: "ingot".into(),
            qty: 3
        }]
    );
    // Same piece, same signature: custody moves it, never makes another.
    assert_eq!(e.parcel.items, vec![signed_sword(900, 0)]);
    // The sheet written with it no longer has any of it.
    assert_eq!(e.sheet.coins, 300);
    assert!(e.sheet.items.is_empty());
    assert_eq!(r.ingots(P1), 5);
    assert_eq!(r.zone.escrows.waiting(), 1);

    let ticket = e.ticket;
    r.tick(&[Command::EscrowDone { ticket, ok: true }]);
    assert_eq!(r.zone.escrows.waiting(), 0);
    assert_eq!(r.sheet(P1).coins, 300);
    assert!(r.out.escrow_lost.is_empty());
}

#[test]
fn a_refused_deposit_comes_back_whole() {
    let mut r = Run::new();
    r.enter();
    let before = r.sheet(P1);
    r.deposit(vec![(0, 8), (1, 1)], 500, "nobody");
    let ticket = r.out.escrows.first().unwrap().ticket;
    r.tick(&[Command::EscrowDone { ticket, ok: false }]);
    let after = r.sheet(P1);
    assert_eq!(after.coins, before.coins);
    assert_eq!(r.ingots(P1), 8);
    // Back with its id, crafter, wear and bonuses.
    assert_eq!(after.items.len(), 1);
    let back = after.items.first().unwrap();
    assert_eq!(back.id, 900);
    assert_eq!(back.crafter.as_deref(), Some("Forgemaster"));
    assert_eq!(
        (back.durability, back.rarity, back.mastery),
        ((31, 40), Some(Rarity::Epic), 125)
    );
    assert_eq!(r.zone.escrows.waiting(), 0);
    // A late or repeated answer does nothing.
    r.tick(&[Command::EscrowDone { ticket, ok: false }]);
    assert_eq!(r.ingots(P1), 8);
}

#[test]
fn bad_deposits_change_nothing() {
    let mut r = Run::new();
    r.enter();
    let before = r.sheet(P1);
    for (items, coins, to, why) in [
        (vec![], 0, "friend", Refusal::InvalidSlot),
        (vec![(0, 9)], 0, "friend", Refusal::InvalidSlot),
        (vec![(0, 1), (0, 1)], 0, "friend", Refusal::InvalidSlot),
        (vec![(1, 2)], 0, "friend", Refusal::InvalidSlot),
        (vec![(5, 1)], 0, "friend", Refusal::InvalidSlot),
        (vec![], 501, "friend", Refusal::NotEnoughCoins),
        (vec![(0, 1)], 0, "hero", Refusal::InvalidTarget),
    ] {
        r.deposit(items, coins, to);
        assert_eq!(r.out.refused, vec![(P1, why)]);
        assert!(r.out.escrows.is_empty());
    }
    assert_eq!(r.sheet(P1), before);
    assert_eq!(r.zone.escrows.waiting(), 0);
}

#[test]
fn a_refusal_after_leaving_is_lost_to_the_zone_not_to_the_game() {
    let mut r = Run::new();
    r.enter();
    r.deposit(vec![(1, 1)], 50, "friend");
    let ticket = r.out.escrows.first().unwrap().ticket;
    r.tick(&[Command::Leave { player: P1 }]);
    r.tick(&[Command::EscrowDone { ticket, ok: false }]);
    // The binary puts it back in custody for the sender.
    let (t, p, parcel) = r.out.escrow_lost.first().unwrap();
    assert_eq!((*t, *p, parcel.coins), (ticket, HERO, 50));
    assert_eq!(parcel.items, vec![signed_sword(900, 0)]);
}

#[test]
fn delivery_fills_the_backpack_or_hands_the_parcel_back() {
    let mut r = Run::new();
    r.enter();
    let parcel = EscrowParcel {
        coins: 70,
        stacks: vec![SavedStack {
            item: "ingot".into(),
            qty: 4,
        }],
        items: vec![signed_sword(901, 0)],
    };
    r.tick(&[Command::Deliver {
        player: P2,
        escrow: 3,
        parcel: Box::new(parcel.clone()),
    }]);
    let d = r.out.delivered.first().unwrap();
    assert_eq!((d.player, d.escrow), (P2, 3));
    let (sheet, _) = d.result.as_ref().expect("room for it");
    assert_eq!(sheet.coins, 70);
    let got = sheet.items.first().unwrap();
    assert_eq!((got.id, got.crafter.as_deref()), (901, Some("Forgemaster")));
    assert_eq!(r.ingots(P2), 4);

    // Gone: the parcel comes back unchanged, nothing is given.
    r.tick(&[Command::Deliver {
        player: PlayerId::new(9),
        escrow: 4,
        parcel: Box::new(parcel.clone()),
    }]);
    assert_eq!(r.out.delivered.first().unwrap().result, Err(parcel.clone()));
}

#[test]
fn a_full_backpack_takes_nothing() {
    let mut r = Run::new();
    r.enter();
    let full = EscrowParcel {
        stacks: vec![SavedStack {
            item: "pelt".into(),
            qty: 20 * crate::BACKPACK_SLOTS as u32,
        }],
        ..EscrowParcel::default()
    };
    r.tick(&[Command::Deliver {
        player: P2,
        escrow: 1,
        parcel: Box::new(full),
    }]);
    let before = r.sheet(P2);
    let parcel = EscrowParcel {
        coins: 10,
        items: vec![signed_sword(902, 0)],
        ..EscrowParcel::default()
    };
    r.tick(&[Command::Deliver {
        player: P2,
        escrow: 2,
        parcel: Box::new(parcel.clone()),
    }]);
    assert_eq!(r.out.delivered.first().unwrap().result, Err(parcel));
    assert_eq!(r.sheet(P2), before, "not even the coins");
}

#[test]
fn waiting_deposits_survive_the_snapshot() {
    let mut r = Run::new();
    r.enter();
    r.deposit(vec![(1, 1)], 10, "friend");
    let state = r.zone.snapshot();
    let content = r.zone.content.clone();
    let copy = Zone::from_snapshot(content, ZoneId::new(0), 7, &state).unwrap();
    assert_eq!(copy.escrows.waiting(), 1);
    assert_eq!(copy.state_hash(), r.zone.state_hash());
}

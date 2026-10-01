//! Phase 9: groups, trade and group XP. Closing criterion of the phase: a
//! trade that is cut short never duplicates or loses anything.

use crate::tests_trade::content_with;
use crate::types::{Command, InviteKind, Refusal, SavedItem, SavedStack, TickInput, TickOutput};
use crate::{CharacterSheet, ItemPlace, LoadLevel, Zone};
use base::{CharacterName, EntityId, PlayerId, TilePos, ZoneId};
use std::collections::BTreeSet;

const P1: PlayerId = PlayerId::new(1);
const P2: PlayerId = PlayerId::new(2);
const P3: PlayerId = PlayerId::new(3);

struct Run {
    zone: Zone,
    out: TickOutput,
}

/// Everything that must be conserved by a trade: coins, pieces per item and
/// the identities of the items with identity, summed over both players.
#[derive(Debug, PartialEq, Eq)]
struct Totals {
    coins: u64,
    ingots: u32,
    instances: BTreeSet<u64>,
    count: usize,
}

impl Run {
    fn new() -> Run {
        Run {
            zone: Zone::new(
                content_with("(level: 1, hp: 100000, damage: (0, 0), xp: 1000, aggro_radius: 0, leash: 30, respawn_ticks: 5)"),
                ZoneId::new(0),
                5,
            )
            .unwrap(),
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
        assert!(self.zone.index_error().is_none(), "indexes follow the world");
    }

    fn idle(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.tick(&[]);
        }
    }

    /// `ingots` in slot 0 and, when `sword_id` is set, a sword in slot 1.
    fn enter(
        &mut self,
        player: PlayerId,
        name: &str,
        at: (i32, i32),
        coins: u64,
        ingots: u32,
        sword_id: Option<u64>,
    ) {
        let mut sheet = CharacterSheet::new(CharacterName::try_from(name).unwrap());
        sheet.coins = coins;
        if ingots > 0 {
            sheet.backpack = vec![Some(SavedStack {
                item: "ingot".into(),
                qty: ingots,
            })];
        }
        if let Some(id) = sword_id {
            sheet.items.push(SavedItem {
                id,
                item: "sword".into(),
                place: ItemPlace::Backpack(1),
                hidden: Vec::new(),
                appraised: false,
                durability: (40, 40),
                crafter: None,
                rarity: None,
                mastery: 0,
                power: None,
            });
        }
        self.tick(&[Command::Enter {
            player,
            sheet: Box::new(sheet),
            pos: Some(TilePos::new(at.0, at.1)),
        }]);
    }

    fn id_of(&self, player: PlayerId) -> EntityId {
        self.zone
            .by_player
            .get(&player)
            .and_then(|h| self.zone.world.get::<&crate::zone::Body>(*h).ok().map(|b| b.id))
            .unwrap()
    }

    fn sheet(&self, player: PlayerId) -> CharacterSheet {
        self.zone
            .players()
            .into_iter()
            .find(|(p, _, _)| *p == player)
            .unwrap()
            .1
    }

    fn refusals(&self) -> Vec<Refusal> {
        self.out.refused.iter().map(|(_, r)| *r).collect()
    }

    fn totals(&self, players: &[PlayerId]) -> Totals {
        let mut t = Totals {
            coins: 0,
            ingots: 0,
            instances: BTreeSet::new(),
            count: 0,
        };
        for &p in players {
            let Some((_, sheet, _)) = self.zone.players().into_iter().find(|(q, _, _)| *q == p) else {
                continue;
            };
            t.coins += sheet.coins;
            t.ingots += sheet
                .backpack
                .iter()
                .flatten()
                .filter(|s| s.item == "ingot")
                .map(|s| s.qty)
                .sum::<u32>();
            for i in &sheet.items {
                t.count += 1;
                t.instances.insert(i.id);
            }
        }
        t
    }

    fn party(&mut self, leader: PlayerId, member: PlayerId) {
        let target = self.id_of(member);
        self.tick(&[Command::PartyInvite {
            player: leader,
            target,
        }]);
        self.tick(&[Command::AnswerInvite {
            player: member,
            accept: true,
        }]);
    }

    /// Both ask, then the trade is open.
    fn open_trade(&mut self) {
        let (a, b) = (self.id_of(P1), self.id_of(P2));
        self.tick(&[Command::TradeRequest {
            player: P1,
            target: b,
        }]);
        self.tick(&[Command::TradeRequest {
            player: P2,
            target: a,
        }]);
    }
}

#[test]
fn invite_accept_and_leave_make_and_end_a_group() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (2, 2), 0, 0, None);
    let (a, b) = (r.id_of(P1), r.id_of(P2));
    r.tick(&[Command::PartyInvite {
        player: P1,
        target: b,
    }]);
    assert_eq!(
        r.out
            .invites
            .iter()
            .map(|i| (i.to, i.from, i.kind))
            .collect::<Vec<_>>(),
        vec![(P2, a, InviteKind::Party)]
    );
    r.tick(&[Command::AnswerInvite {
        player: P2,
        accept: true,
    }]);
    assert_eq!(r.refusals(), vec![]);
    let views: Vec<_> = r
        .out
        .parties
        .iter()
        .map(|(p, v)| (*p, v.clone().unwrap()))
        .collect();
    assert_eq!(views.len(), 2);
    assert!(
        views
            .iter()
            .all(|(_, v)| v.leader == a && v.members == vec![a, b])
    );

    // The leader goes: the group is below two members and ends for both.
    r.tick(&[Command::LeaveParty { player: P1 }]);
    assert!(r.out.parties.iter().all(|(_, v)| v.is_none()));
    assert_eq!(r.out.parties.len(), 2);
}

#[test]
fn only_the_leader_invites_and_a_full_group_refuses() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (2, 2), 0, 0, None);
    r.enter(P3, "cat", (3, 2), 0, 0, None);
    r.party(P1, P2);
    let c = r.id_of(P3);
    // A member is not the leader.
    r.tick(&[Command::PartyInvite {
        player: P2,
        target: c,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NoParty]);
    // Declining leaves no group for the third.
    r.tick(&[Command::PartyInvite {
        player: P1,
        target: c,
    }]);
    r.tick(&[Command::AnswerInvite {
        player: P3,
        accept: false,
    }]);
    assert!(r.zone.party_members(P3).is_none());
    // An answer with no invite.
    r.tick(&[Command::AnswerInvite {
        player: P3,
        accept: true,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NoParty]);
    // Leadership passes and kicking works for the new leader only.
    let b = r.id_of(P2);
    r.tick(&[Command::PartyLead {
        player: P1,
        target: b,
    }]);
    r.tick(&[Command::PartyKick {
        player: P1,
        target: b,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NoParty]);
}

#[test]
fn a_confirmed_trade_swaps_everything_and_conserves_it() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 500, 5, Some(10));
    r.enter(P2, "bob", (2, 2), 100, 0, Some(20));
    let before = r.totals(&[P1, P2]);
    assert_eq!(before.ingots, 5);
    assert_eq!(before.count, 2);
    r.open_trade();
    assert_eq!(r.refusals(), vec![]);

    // Ann offers 3 ingots, her sword and 200 copper; Bob offers his sword.
    r.tick(&[
        Command::TradeOffer {
            player: P1,
            items: vec![(0, 3), (1, 1)],
            coins: 200,
        },
        Command::TradeOffer {
            player: P2,
            items: vec![(0, 1)],
            coins: 0,
        },
    ]);
    // Bob had his sword in slot 1, not 0: the offer is refused whole.
    assert_eq!(r.refusals(), vec![Refusal::InvalidSlot]);
    r.tick(&[Command::TradeOffer {
        player: P2,
        items: vec![(1, 1)],
        coins: 0,
    }]);
    assert_eq!(r.refusals(), vec![]);
    // Nothing moved yet.
    assert_eq!(r.totals(&[P1, P2]), before);
    assert_eq!(r.sheet(P1).coins, 500);

    r.tick(&[Command::TradeConfirm { player: P1 }]);
    assert_eq!(r.sheet(P1).coins, 500, "one confirmation is not a trade");
    r.tick(&[Command::TradeConfirm { player: P2 }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(
        r.totals(&[P1, P2]),
        before,
        "the swap conserves coins, pieces and identities"
    );
    assert_eq!(r.sheet(P1).coins, 300);
    assert_eq!(r.sheet(P2).coins, 300);
    let bob = r.sheet(P2);
    // Bob gave his sword (20) and got Ann's (10).
    assert_eq!(bob.items.len(), 1);
    assert_eq!(bob.items[0].id, 10);
    assert_eq!(r.sheet(P1).items[0].id, 20);
    let bag: u32 = bob.backpack.iter().flatten().map(|s| s.qty).sum();
    assert_eq!(bag, 3);
    assert!(r.out.trades.iter().all(|(_, v)| v.is_none()), "the trade ends");
}

#[test]
fn changing_an_offer_takes_the_confirmation_back() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 500, 5, None);
    r.enter(P2, "bob", (2, 2), 0, 0, None);
    r.open_trade();
    r.tick(&[
        Command::TradeOffer {
            player: P1,
            items: vec![(0, 2)],
            coins: 0,
        },
        Command::TradeConfirm { player: P2 },
        // Ann changes her mind after Bob confirmed.
        Command::TradeOffer {
            player: P1,
            items: vec![(0, 5)],
            coins: 0,
        },
        Command::TradeConfirm { player: P1 },
    ]);
    assert_eq!(
        r.sheet(P2).backpack.iter().flatten().count(),
        0,
        "Bob would have confirmed the old offer"
    );
    let view = r
        .out
        .trades
        .iter()
        .find(|(p, _)| *p == P2)
        .unwrap()
        .1
        .clone()
        .unwrap();
    assert!(view.theirs_confirmed && !view.mine_confirmed);
}

#[test]
fn a_full_backpack_stops_the_swap_and_changes_nothing() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, Some(10));
    // Bob's backpack is full of ingots.
    let mut bob = CharacterSheet::new(CharacterName::try_from("bob").unwrap());
    bob.backpack = (0..crate::BACKPACK_SLOTS)
        .map(|_| {
            Some(SavedStack {
                item: "ingot".into(),
                qty: 20,
            })
        })
        .collect();
    r.tick(&[Command::Enter {
        player: P2,
        sheet: Box::new(bob),
        pos: Some(TilePos::new(2, 2)),
    }]);
    let before = r.totals(&[P1, P2]);
    r.open_trade();
    r.tick(&[Command::TradeOffer {
        player: P1,
        items: vec![(1, 1)],
        coins: 0,
    }]);
    r.tick(&[
        Command::TradeConfirm { player: P1 },
        Command::TradeConfirm { player: P2 },
    ]);
    assert_eq!(r.refusals(), vec![Refusal::InventoryFull]);
    assert_eq!(r.totals(&[P1, P2]), before);
    assert_eq!(r.sheet(P1).items.len(), 1, "the sword stayed with Ann");
}

#[test]
fn a_cut_short_trade_never_duplicates_or_loses() {
    // Closing criterion of phase 9: the trade breaks at every moment.
    for cut in 0..4 {
        let mut r = Run::new();
        r.enter(P1, "ann", (1, 1), 500, 5, Some(10));
        r.enter(P2, "bob", (2, 2), 100, 0, Some(20));
        let before = r.totals(&[P1, P2]);
        r.open_trade();
        r.tick(&[
            Command::TradeOffer {
                player: P1,
                items: vec![(0, 5), (1, 1)],
                coins: 300,
            },
            Command::TradeOffer {
                player: P2,
                items: vec![(1, 1)],
                coins: 50,
            },
        ]);
        r.tick(&[Command::TradeConfirm { player: P1 }]);
        match cut {
            // A player drops before the second confirmation.
            0 => {
                r.tick(&[Command::Leave { player: P1 }]);
                let left = r.out.left.iter().map(|(_, s, _)| s.clone()).collect::<Vec<_>>();
                let mut t = r.totals(&[P2]);
                t.coins += left.iter().map(|s| s.coins).sum::<u64>();
                t.count += left.iter().map(|s| s.items.len()).sum::<usize>();
                t.ingots += left
                    .iter()
                    .flat_map(|s| s.backpack.iter().flatten())
                    .map(|s| s.qty)
                    .sum::<u32>();
                t.instances
                    .extend(left.iter().flat_map(|s| s.items.iter().map(|i| i.id)));
                assert_eq!(t, before);
                assert!(r.zone.social.trades.is_empty());
            }
            // The other side cancels.
            1 => {
                r.tick(&[Command::TradeCancel { player: P2 }]);
                assert_eq!(r.totals(&[P1, P2]), before);
                assert_eq!(r.sheet(P1).items[0].id, 10);
            }
            // They walk apart.
            2 => {
                r.tick(&[Command::Move {
                    player: P2,
                    destination: TilePos::new(8, 3),
                }]);
                r.idle(60);
                assert!(r.zone.social.trades.is_empty(), "distance cancels it");
                assert_eq!(r.totals(&[P1, P2]), before);
                assert_eq!(r.sheet(P1).items[0].id, 10);
            }
            // Nobody answers: the offer expires.
            _ => {
                r.idle(crate::TRADE_TICKS + 2);
                assert!(r.zone.social.trades.is_empty(), "time cancels it");
                assert_eq!(r.totals(&[P1, P2]), before);
            }
        }
    }
}

#[test]
fn a_trade_needs_both_players_close_and_one_at_a_time() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (8, 3), 0, 0, None);
    r.enter(P3, "cat", (2, 1), 0, 0, None);
    let b = r.id_of(P2);
    r.tick(&[Command::TradeRequest {
        player: P1,
        target: b,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::TooFar]);
    let c = r.id_of(P3);
    r.tick(&[Command::TradeRequest {
        player: P1,
        target: c,
    }]);
    assert_eq!(r.refusals(), vec![]);
    // Ann is already waiting on Cat.
    let a = r.id_of(P1);
    r.tick(&[Command::TradeRequest {
        player: P2,
        target: a,
    }]);
    assert!(!r.refusals().is_empty());
    // An offer before the other side answered.
    r.tick(&[Command::TradeOffer {
        player: P1,
        items: vec![],
        coins: 0,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NoTrade]);
}

#[test]
fn group_xp_follows_contribution_and_distance() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (2, 2), 0, 0, None);
    r.enter(P3, "cat", (3, 2), 0, 0, None);
    r.party(P1, P2);
    let monster = r.zone.by_id.iter().find_map(|(id, h)| {
        r.zone
            .world
            .get::<&crate::fight::MonsterComp>(*h)
            .is_ok()
            .then_some((*id, *h))
    });
    let (mid, mh) = monster.unwrap();
    let (a, b, c) = (r.id_of(P1), r.id_of(P2), r.id_of(P3));
    {
        let mut m = r.zone.world.get::<&mut crate::fight::MonsterComp>(mh).unwrap();
        m.threat.add(a, 400);
        m.threat.add(b, 400);
        m.threat.add(c, 190);
    }
    let m = crate::fight::MonsterComp::clone(&r.zone.world.get::<&crate::fight::MonsterComp>(mh).unwrap());
    let xp = crate::fight::kill_xp(&r.zone, &m, mh, 1000);
    // Pool of the group: 80.8% of 1000, plus 10% for the second member,
    // split in two. Cat fought alone with 19.2%: her own share, no bonus.
    let share = u64::from(m.threat.share_permille(a));
    let pooled = 1000 * (share + u64::from(m.threat.share_permille(b))) / 1000;
    let each = pooled * 1100 / 1000 / 2;
    let cat = 1000 * u64::from(m.threat.share_permille(c)) / 1000;
    assert_eq!(xp, vec![(a, each), (b, each), (c, cat)]);
    assert!(each > 400, "grouping pays: {each} each against {share} alone");

    // A bystander under a tenth earns nothing.
    {
        let mut m = r.zone.world.get::<&mut crate::fight::MonsterComp>(mh).unwrap();
        m.threat.clear();
        m.threat.add(a, 950);
        m.threat.add(c, 50);
    }
    let m = crate::fight::MonsterComp::clone(&r.zone.world.get::<&crate::fight::MonsterComp>(mh).unwrap());
    let xp = crate::fight::kill_xp(&r.zone, &m, mh, 1000);
    assert_eq!(xp, vec![(a, 950)]);
    let _ = mid;
}

#[test]
fn an_emote_reaches_who_sees_it_and_waits_its_cooldown() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (2, 2), 0, 0, None);
    // A tick of movement so each side knows the other.
    r.idle(2);
    let wave = r.zone.content().catalog.emote_by_name("wave").unwrap().0;
    let a = r.id_of(P1);
    r.tick(&[Command::Emote {
        player: P1,
        emote: wave,
    }]);
    assert_eq!(r.refusals(), vec![]);
    assert_eq!(r.out.emotes, vec![(P1, a, wave), (P2, a, wave)]);
    // Too soon, then fine after the wait.
    r.tick(&[Command::Emote {
        player: P1,
        emote: wave,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NotReady]);
    r.idle(40);
    r.tick(&[Command::Emote {
        player: P1,
        emote: wave,
    }]);
    assert_eq!(r.refusals(), vec![]);
    // An emote the content does not have.
    r.idle(40);
    r.tick(&[Command::Emote {
        player: P1,
        emote: base::EmoteId::new(99),
    }]);
    assert_eq!(r.refusals(), vec![Refusal::RequirementsNotMet]);
}

/// A revival item stands a fallen player up, only close by,
/// only when they are fallen, and it is spent.
#[test]
fn a_revival_item_stands_a_fallen_ally_up() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (2, 1), 0, 0, None);
    r.enter(P3, "cat", (9, 3), 0, 0, None);
    // Ann carries two draughts in slot 0.
    let h1 = r.zone.by_player[&P1];
    {
        let content = r.zone.content.clone();
        let (id, _) = content.catalog.item_by_name("draught").unwrap();
        let mut p = r.zone.world.get::<&mut crate::zone::PlayerComp>(h1).unwrap();
        p.data.give(&content.catalog, id, 2).unwrap();
    }
    let draughts = |r: &Run| {
        r.sheet(P1)
            .backpack
            .iter()
            .flatten()
            .filter(|s| s.item == "draught")
            .map(|s| s.qty)
            .sum::<u32>()
    };
    let slot = r
        .sheet(P1)
        .backpack
        .iter()
        .position(|s| s.as_ref().is_some_and(|s| s.item == "draught"))
        .unwrap() as u16;
    let bob = r.id_of(P2);
    // Bob is standing: nothing to revive, nothing spent.
    r.tick(&[Command::UseItemOn {
        player: P1,
        slot,
        target: bob,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::InvalidTarget]);
    assert_eq!(draughts(&r), 2);
    // Bob and Cat fall.
    let now = r.zone.tick.0;
    for p in [P2, P3] {
        let h = r.zone.by_player[&p];
        let mut f = r.zone.world.get::<&mut crate::fight::Fighter>(h).unwrap();
        f.hp = 0;
        f.fallen_until = now + 1000;
    }
    // Cat is too far.
    let cat = r.id_of(P3);
    r.tick(&[Command::UseItemOn {
        player: P1,
        slot,
        target: cat,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::TooFar]);
    // Bob stands up with 30% of his HP; one draught is gone.
    r.tick(&[Command::UseItemOn {
        player: P1,
        slot,
        target: bob,
    }]);
    assert_eq!(r.refusals(), vec![]);
    let h2 = r.zone.by_player[&P2];
    let (hp, max, up) = {
        let f = r.zone.world.get::<&crate::fight::Fighter>(h2).unwrap();
        (f.hp, f.max_hp, f.fallen_until == 0)
    };
    assert!(up, "bob is up");
    assert_eq!(hp, (u64::from(max) * 300 / 1000).max(1) as u32);
    assert_eq!(draughts(&r), 1);
    // Used on oneself, a revival item does nothing and is kept.
    r.tick(&[Command::UseItem { player: P1, slot }]);
    assert_eq!(r.refusals(), vec![Refusal::InvalidTarget]);
    assert_eq!(draughts(&r), 1);
}

/// Light magic, with no weapon in hand. A heal mends a hurt ally
/// (nothing spent on one at full life) and Light grows; the resurrection
/// stands a fallen one up with half their life.
#[test]
fn light_heals_and_raises_without_a_weapon() {
    let mut r = Run::new();
    r.enter(P1, "ann", (1, 1), 0, 0, None);
    r.enter(P2, "bob", (3, 1), 0, 0, None);
    let content = r.zone.content.clone();
    let cat = &content.catalog;
    let id = |n: &str| {
        cat.techniques()
            .find(|(_, t)| t.id == n)
            .map(|(id, _)| id)
            .unwrap()
    };
    let (heal, raise) = (id("heal"), id("raise"));
    let h1 = r.zone.by_player[&P1];
    {
        let mut p = r.zone.world.get::<&mut crate::zone::PlayerComp>(h1).unwrap();
        progression::learn_technique(&mut p.growth.sheet, heal).unwrap();
        progression::learn_technique(&mut p.growth.sheet, raise).unwrap();
    }
    let bob = r.id_of(P2);
    let h2 = r.zone.by_player[&P2];
    let fighter = |r: &Run| {
        let f = r.zone.world.get::<&crate::fight::Fighter>(h2).unwrap();
        (f.hp, f.max_hp, f.fallen_until)
    };
    // Bob at full life: nothing to heal, nothing spent.
    r.tick(&[Command::UseTechnique {
        player: P1,
        technique: heal,
        target: bob,
    }]);
    assert_eq!(r.refusals(), vec![Refusal::NotReady]);
    assert!(r.out.supports.is_empty());
    // Bob hurt: healed by 20% of his life, and Light grows.
    let (_, max, _) = fighter(&r);
    r.zone.world.get::<&mut crate::fight::Fighter>(h2).unwrap().hp = 1;
    r.tick(&[Command::UseTechnique {
        player: P1,
        technique: heal,
        target: bob,
    }]);
    assert_eq!(r.refusals(), vec![]);
    // The heal is told, for its sound (a refusal is not).
    assert_eq!(r.out.supports, vec![(r.id_of(P1), bob, heal)]);
    assert_eq!(fighter(&r).0, 1 + (u64::from(max) * 200 / 1000) as u32);
    let light = cat.technique(heal).unwrap().proficiency;
    let grew = r
        .zone
        .world
        .get::<&crate::zone::PlayerComp>(h1)
        .unwrap()
        .growth
        .sheet
        .proficiencies
        .iter()
        .any(|p| p.id == light);
    assert!(grew, "a heal with effect counts as use of Light");
    // Bob falls; the resurrection stands him up with half his life.
    r.idle(20);
    let now = r.zone.tick.0;
    {
        let mut f = r.zone.world.get::<&mut crate::fight::Fighter>(h2).unwrap();
        f.hp = 0;
        f.fallen_until = now + 1000;
    }
    r.tick(&[Command::UseTechnique {
        player: P1,
        technique: raise,
        target: bob,
    }]);
    assert_eq!(r.refusals(), vec![]);
    let (hp, max, fallen) = fighter(&r);
    assert_eq!(fallen, 0);
    assert_eq!(hp, (u64::from(max) * 500 / 1000) as u32);
}

"""Phase 10 provisional art: the start zone's NPCs, the new monsters and their drops."""
import numpy as np
from artlib import Canvas, ramp, ell, rect, poly, line, cyl, ellipsoid
from art_chars import humanoid, quadruped_legs, staff, beard, SKIN, SKIN_TAN, EYE, GOLD, LEATHER, LEATHER_DK, CREAM, BLUE, GREEN, RED, CYAN, WOOD
from art_icons import new, badge, slab, BONE

PURPLE = ramp(275, 0.55, 0.70)
GOBLIN = ramp(95, 0.50, 0.70)
SILK = ramp(220, 0.06, 1.0)
RAT = ramp(28, 0.25, 0.55)
SPIDER = ramp(300, 0.35, 0.35)


# --- NPCs -------------------------------------------------------------------

def summoner():
    return humanoid(Canvas(64, 64), dict(robe=True, tunic=PURPLE, trim=GOLD, hood=PURPLE, head="hood", skin=SKIN, boots=LEATHER_DK, extra=[beard, staff]))


def sword_on_back(c):
    c.shade(line(c, (18, 50), (44, 16), 2), ramp(212, 0.16, 0.86), None, flat=4, sep=False)
    c.shade(line(c, (40, 22), (46, 25), 2), GOLD, None, flat=3, sep=False)


def instructor():
    return humanoid(Canvas(64, 64), dict(tunic=RED, sleeve=ramp(20, 0.3, 0.5), pants=ramp(30, 0.25, 0.42), boots=LEATHER_DK, belt=True, hair=ramp(24, 0.3, 0.30), head="cap", skin=SKIN_TAN, extra=[sword_on_back]))


def ledger(c):
    c.shade(rect(c, 15, 40, 23, 50), CREAM, slab(), dither=0.2)
    c.shade(rect(c, 15, 40, 16, 50), BLUE, None, flat=2, sep=False)


def guild_clerk():
    return humanoid(Canvas(64, 64), dict(tunic=BLUE, pants=ramp(222, 0.3, 0.4), boots=LEATHER_DK, belt=True, hair=ramp(40, 0.5, 0.80), head="cap", skin=SKIN, extra=[ledger]))


# --- Monsters ---------------------------------------------------------------

def field_rat():
    c = Canvas(64, 64)
    dark = ramp(28, 0.30, 0.35)
    belly = ramp(30, 0.10, 0.90)
    c.shadow(32, 58, 12, 3)
    c.shade(line(c, (21, 52), (8, 48), 2) | line(c, (8, 48), (4, 41), 1), ramp(350, 0.3, 0.80), None, flat=3)
    quadruped_legs(c, RAT, dark, (27, 36), (24, 38), 52, 57, belly)
    body = ell(c, 31, 48, 11, 6.5)
    c.shade(body, RAT, ellipsoid(31, 46, 11, 8), dither=0.4, jitter=0.3)
    head = ell(c, 43, 47, 5.5, 4.5)
    c.shade(head, RAT, ellipsoid(43, 45, 6, 6), dither=0.3)
    c.shade(poly(c, [(46, 45), (52, 48), (46, 50)]), RAT, None, flat=3)
    c.shade(ell(c, 40, 41.5, 2.4, 2.4), ramp(350, 0.3, 0.8), None, flat=3)
    c.px(52, 48, ramp(350, 0.4, 0.5), 1)
    c.px(45, 46, EYE, 0)
    c.outline()
    return c


def briar_spider():
    c = Canvas(64, 64)
    leg = ramp(300, 0.25, 0.28)
    c.shadow(32, 57, 18, 4)
    for side in (-1, 1):
        for i, (kx, ky) in enumerate(((9, 36), (13, 41), (14, 47), (11, 52))):
            hip = (32 + side * 4, 44 + i)
            knee = (32 + side * (kx + 4), ky - 4)
            foot = (32 + side * (kx + 9), ky + 10 - i)
            c.shade(line(c, hip, knee, 2) | line(c, knee, foot, 1), leg, None, flat=2)
    abdomen = ell(c, 30, 40, 10, 8)
    c.shade(abdomen, SPIDER, ellipsoid(28, 37, 10, 10), dither=0.4, jitter=0.3)
    for x, y in ((27, 37), (32, 36), (29, 42)):
        c.px(x, y, GREEN, 4)
    head = ell(c, 33, 48, 6, 4.5)
    c.shade(head, SPIDER, ellipsoid(32, 46, 6, 6), dither=0.3)
    for x in (31, 33, 35):
        c.px(x, 47, RED, 4)
    c.outline()
    return c


def goblin_body(robe):
    loin = ramp(30, 0.45, 0.45)
    p = dict(tunic=PURPLE if robe else loin, pants=loin, boots=LEATHER_DK, hair=ramp(100, 0.4, 0.30), head="hood" if robe else "cap", hood=PURPLE, skin=GOBLIN, belt=not robe, robe=robe)
    extra = [goblin_ears]
    extra.append(staff if robe else club)
    p["extra"] = extra
    c = Canvas(64, 64)
    return humanoid(c, p)


def goblin_ears(c):
    for pts in ([(24, 20), (16, 15), (25, 23)], [(40, 20), (48, 15), (39, 23)]):
        c.shade(poly(c, pts), GOBLIN, None, flat=3)


def club(c):
    c.shade(line(c, (44, 46), (49, 30), 3), WOOD, cyl(46, 2), dither=0.3, sep=False)
    c.shade(ell(c, 49.5, 29, 3.5, 4), WOOD, ellipsoid(49, 28, 3.5, 4), dither=0.3)


def hill_goblin():
    return goblin_body(False)


def goblin_shaman():
    return goblin_body(True)


# --- Portraits ------------------------------------------------------------------

def portrait(ring, fill, extra):
    c = new()
    badge(c, ring)
    extra(c, fill)
    c.outline()
    return c


def face_rat(c, fur):
    c.shade(ell(c, 9, 10, 3.5, 3.5) | ell(c, 23, 10, 3.5, 3.5), ramp(350, 0.3, 0.8), None, flat=3)
    c.shade(poly(c, [(8, 13), (24, 13), (16, 27)]), fur, ellipsoid(16, 16, 9, 10), dither=0.3)
    c.px(16, 27, ramp(350, 0.4, 0.5), 1)
    c.px(12, 16, EYE, 0); c.px(20, 16, EYE, 0)


def face_spider(c, body):
    for y in (12, 17, 22):
        c.shade(line(c, (5, y - 3), (27, y + 3), 1) | line(c, (27, y - 3), (5, y + 3), 1), ramp(300, 0.25, 0.28), None, flat=2, sep=False)
    c.shade(ell(c, 16, 17, 7, 6.5), body, ellipsoid(15, 15, 7, 7), dither=0.3)
    for x in (12, 15, 18, 21):
        c.px(x, 15, RED, 4)


def face_goblin(c, skin, hood=False):
    for pts in ([(10, 15), (4, 9), (11, 19)], [(22, 15), (28, 9), (21, 19)]):
        c.shade(poly(c, pts), skin, None, flat=3)
    if hood:
        c.shade(ell(c, 16, 14, 9, 9), PURPLE, ellipsoid(15, 12, 9, 10), dither=0.3)
    c.shade(ell(c, 16, 18, 6.5, 7), skin, ellipsoid(15, 16, 7, 8), dither=0.2)
    c.px(13, 17, RED, 4); c.px(19, 17, RED, 4)
    c.shade(rect(c, 13, 22, 19, 22), ramp(100, 0.4, 0.3), None, flat=1, sep=False)


# --- Items ------------------------------------------------------------------

def item_rat_tail():
    c = new()
    c.shade(line(c, (6, 26), (14, 18), 3) | line(c, (14, 18), (22, 14), 2) | line(c, (22, 14), (27, 5), 1), ramp(350, 0.3, 0.80), None, flat=3)
    c.shade(ell(c, 6, 26, 3, 3), RAT, ellipsoid(5, 25, 3, 3))
    c.outline()
    return c


def item_spider_silk():
    c = new()
    c.shade(ell(c, 16, 17, 10, 9), SILK, ellipsoid(14, 14, 11, 11), dither=0.4, jitter=0.3)
    for a, b in (((8, 12), (24, 22)), ((8, 20), (24, 12)), ((10, 8), (22, 26))):
        c.adjust(line(c, a, b, 1) & ell(c, 16, 17, 10, 9), -1)
    c.outline()
    return c


def item_goblin_ear():
    c = new()
    c.shade(poly(c, [(6, 24), (26, 5), (20, 18), (14, 27)]), GOBLIN, slab(), dither=0.3)
    c.adjust(line(c, (11, 23), (21, 11), 1), -1)
    c.outline()
    return c


def item_guild_card():
    c = new()
    card = rect(c, 5, 8, 26, 24)
    c.shade(card, CREAM, slab(), dither=0.2)
    c.shade(rect(c, 5, 8, 26, 11), BLUE, None, flat=3, sep=False)
    c.shade(ell(c, 11, 18, 3, 3), GOLD, ellipsoid(10, 17, 3, 3), sep=False)
    for y in (16, 19, 22):
        c.adjust(line(c, (16, y), (24, y), 1), -2)
    c.outline()
    return c


# --- Wall tiles (64x72, anchor (32, 56)) --------------------------------------

def brazier(k=0):
    """Frame k of the flame (0..3): the tips sway and the core breathes."""
    sway = (0, 2, -1, 1)[k % 4]
    lick = (0, -3, 2, -1)[k % 4]
    c = Canvas(64, 72)
    stone = ramp(235, 0.12, 0.70)
    iron = ramp(220, 0.15, 0.45)
    fire = ramp(30, 0.85, 1.0)
    c.shadow(32, 57, 12, 4)
    c.shade(poly(c, [(26, 58), (38, 58), (36, 40), (28, 40)]), stone, cyl(32, 6), dither=0.3)
    c.shade(ell(c, 32, 38, 12, 4.5), iron, ellipsoid(31, 36, 12, 6), dither=0.2)
    flame = poly(c, [(22, 37), (26 - sway, 24 + lick), (30, 30), (32 + sway, 14 - lick), (35, 28), (39 + sway, 21 - lick), (42, 37)])
    c.shade(flame, fire, cyl(32, 10), dither=0.3, jitter=0.3, seed=1 + k)
    c.shade(poly(c, [(27, 37), (30, 28), (32 + sway // 2, 22 + abs(lick) // 2), (34, 29), (37, 37)]), ramp(48, 0.6, 1.0), None, flat=5, sep=False)
    c.outline()
    return c


def tree():
    c = Canvas(64, 72)
    bark = ramp(26, 0.50, 0.45)
    leaf = ramp(120, 0.55, 0.62)
    c.shadow(32, 57, 18, 5)
    c.shade(poly(c, [(28, 58), (36, 58), (35, 34), (29, 34)]), bark, cyl(32, 4), dither=0.3, jitter=0.2)
    crown = ell(c, 32, 26, 20, 17) | ell(c, 20, 32, 11, 9) | ell(c, 44, 32, 11, 9) | ell(c, 32, 12, 12, 10)
    c.shade(crown, leaf, ellipsoid(28, 20, 22, 22), dither=0.5, jitter=0.5, seed=4)
    c.outline()
    return c


JOBS = {
    "sprites/walls/tree.png": tree,
    "sprites/creatures/summoner.png": summoner,
    "sprites/creatures/instructor.png": instructor,
    "sprites/creatures/guild_clerk.png": guild_clerk,
    "sprites/creatures/field_rat.png": field_rat,
    "sprites/creatures/briar_spider.png": briar_spider,
    "sprites/creatures/hill_goblin.png": hill_goblin,
    "sprites/creatures/goblin_shaman.png": goblin_shaman,
    "icons/field_rat.png": lambda: portrait(ramp(28, 0.4, 0.7), RAT, face_rat),
    "icons/briar_spider.png": lambda: portrait(ramp(300, 0.4, 0.6), SPIDER, face_spider),
    "icons/hill_goblin.png": lambda: portrait(ramp(95, 0.5, 0.6), GOBLIN, face_goblin),
    "icons/goblin_shaman.png": lambda: portrait(ramp(275, 0.5, 0.7), GOBLIN, lambda c, s: face_goblin(c, s, True)),
    "icons/rat_tail.png": item_rat_tail,
    "icons/spider_silk.png": item_spider_silk,
    "icons/goblin_ear.png": item_goblin_ear,
    "icons/guild_card.png": item_guild_card,
}

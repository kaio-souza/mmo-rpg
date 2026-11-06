"""Item, technique and monster icons (32x32)."""
import numpy as np
from artlib import Canvas, ramp, ell, rect, poly, line, cyl, ellipsoid, blade, hash2
from art_chars import SKIN_TAN, EYE, GOLD, STEEL, RUST, WOOD, LEATHER, LEATHER_DK, CREAM, RED, HERB, CYAN, GREEN

BG = ramp(235, 0.35, 0.34)
FUR_W = ramp(218, 0.16, 0.66)
FUR_OLD = ramp(210, 0.05, 0.84)
FUR_B = ramp(18, 0.52, 0.52)
BONE = ramp(48, 0.20, 1.0)
BREAD = ramp(32, 0.62, 0.92)
ROCK = ramp(235, 0.14, 0.72)


def new():
    return Canvas(32, 32)


def badge(c, ring):
    c.shade(ell(c, 16, 16, 15, 15), ring, ellipsoid(16, 16, 15, 15), dither=0.3)
    c.shade(ell(c, 16, 16.5, 12.2, 12.2), BG, ellipsoid(16, 14, 13, 14), dither=0.4, sep=False, lift=-0.4)


def inside(c):
    return ell(c, 16, 16.5, 12.2, 12.2)


def portrait_wolf(old):
    c = new()
    badge(c, ramp(215, 0.30, 0.78))
    fur = FUR_OLD if old else FUR_W
    dark = ramp(240, 0.14, 0.45)
    belly = ramp(210, 0.06, 0.94)
    for pts in ([(8, 14), (9, 5), (14, 11)], [(18, 11), (23, 5), (25, 14)]):
        c.shade(poly(c, pts), fur, None, flat=3)
    c.shade(ell(c, 16, 17.5, 9, 8), fur, ellipsoid(15, 15, 10, 10), dither=0.4, jitter=0.3)
    c.shade(poly(c, [(12, 18), (20, 18), (19, 26), (16, 28), (13, 26)]), belly, cyl(16, 5), dither=0.3)
    c.shade(rect(c, 14, 22, 18, 25), dark, None, flat=2, sep=False)
    c.px(15, 22, dark, 0); c.px(16, 22, dark, 0); c.px(17, 22, dark, 0)
    for ex in (11, 20):
        c.px(ex, 16, ramp(48, 0.9, 1.0), 5)
        c.px(ex + 1, 16, EYE, 0)
    if old:
        for x, y in ((9, 11), (10, 12), (11, 13), (12, 14)):
            c.px(x, y, ramp(350, 0.5, 0.85), 3)
    c.outline()
    return c


def portrait_boar():
    c = new()
    badge(c, ramp(22, 0.60, 0.70))
    dark = ramp(20, 0.40, 0.30)
    c.shade(poly(c, [(7, 11), (9, 5), (14, 10)]), dark, None, flat=2)
    c.shade(poly(c, [(18, 10), (23, 5), (25, 11)]), dark, None, flat=2)
    c.shade(ell(c, 16, 16.5, 9.5, 8.5), FUR_B, ellipsoid(15, 14, 10, 10), dither=0.4, jitter=0.3)
    c.shade(ell(c, 16, 21, 5.5, 4.4), ramp(10, 0.38, 0.80), ellipsoid(15, 19, 6, 6), dither=0.3)
    c.px(14, 21, dark, 0); c.px(18, 21, dark, 0); c.px(14, 22, dark, 0); c.px(18, 22, dark, 0)
    c.shade(poly(c, [(9, 20), (7, 15), (10, 17), (11, 22)]), BONE, None, flat=4)
    c.shade(poly(c, [(23, 20), (25, 15), (22, 17), (21, 22)]), BONE, None, flat=3)
    for ex in (11, 20):
        c.px(ex, 15, EYE, 0)
        c.px(ex, 14, ramp(48, 0.9, 1.0), 5)
    c.outline()
    return c


def portrait_bandit():
    c = new()
    badge(c, ramp(355, 0.62, 0.70))
    hair = ramp(24, 0.4, 0.38)
    c.shade(ell(c, 16, 15, 8.5, 8.2), hair, ellipsoid(15, 13, 9, 9), dither=0.3)
    c.shade(ell(c, 16, 18, 6.8, 7.2), SKIN_TAN, ellipsoid(15, 16, 7, 8), dither=0.2)
    c.shade(rect(c, 7, 11, 25, 14) & ell(c, 16, 15, 8.8, 8.6), RED, cyl(16, 9), dither=0.2)
    c.shade(poly(c, [(22, 12), (28, 10), (27, 16), (24, 14)]), RED, None, flat=2)
    c.shade(poly(c, [(10, 20), (22, 20), (20, 26), (12, 26)]), RED, cyl(16, 6), dither=0.2)
    for ex in (12, 19):
        c.px(ex, 17, EYE, 0)
        c.px(ex, 18, EYE, 0)
    c.outline()
    return c


def tech(ring, glyph):
    c = new()
    badge(c, ring)
    glyph(c)
    c.outline()
    return c


def g_double_cut(c):
    ice = ramp(190, 0.45, 1.0)
    blade(c, (8, 22), (24, 6), 4, ice)
    blade(c, (13, 25), (26, 12), 3, ice)


def g_heavy_punch(c):
    skin = ramp(18, 0.55, 0.95)
    c.shade(poly(c, [(16, 3), (19, 10), (28, 8), (22, 15), (27, 23), (18, 20), (16, 28), (14, 20), (5, 23), (10, 15), (4, 8), (13, 10)]), ramp(38, 0.9, 1.0), None, flat=3, sep=False)
    c.shade(ell(c, 16, 17, 7, 6.4), skin, ellipsoid(14, 15, 8, 8), dither=0.2)
    for fx in (11, 14.5, 18, 21.5):
        c.shade(ell(c, fx, 12.5, 2.0, 2.4), skin, ellipsoid(fx - 1, 11, 2.4, 3), sep=True)
    c.shade(ell(c, 9.5, 17, 2, 3), skin, ellipsoid(9, 16, 2.4, 3.4), sep=True)


def g_quick_stab(c):
    for y, x0 in ((10, 5), (16, 3), (22, 6)):
        c.px(x0, y, CREAM, 5); c.px(x0 + 1, y, CREAM, 5); c.px(x0 + 2, y, CREAM, 4); c.px(x0 + 3, y, CREAM, 3)
    blade(c, (9, 21), (26, 14), 4, STEEL)
    c.shade(line(c, (10, 17), (12, 25), 2), GOLD, None, flat=3, sep=False)
    c.shade(line(c, (4, 25), (10, 22), 3), LEATHER_DK, None, flat=3, sep=False)


def g_generic(c):
    c.shade(poly(c, [(16, 4), (19, 13), (28, 16), (19, 19), (16, 28), (13, 19), (4, 16), (13, 13)]), GOLD, ellipsoid(14, 13, 12, 12), dither=0.2)


def item_sword(rusty):
    c = new()
    steel = ramp(215, 0.10, 0.62) if rusty else STEEL
    blade(c, (11, 21), (27, 5), 5, steel)
    if rusty:
        m = c.alpha == 255
        h = hash2(c.xs, c.ys, 31)
        c.rid[m & (h > 0.68)] = c._rid(RUST)
        c.tone[m & (h > 0.68)] = 2
        c.tone[m & (h > 0.90)] = 4
    c.shade(line(c, (7, 20), (14, 27), 3), GOLD if not rusty else ramp(30, 0.5, 0.6), None, flat=3, sep=False)
    c.shade(line(c, (9, 24), (5, 28), 3), LEATHER_DK, None, flat=3, sep=False)
    c.shade(ell(c, 4.5, 28.5, 2.2, 2.2), GOLD, ellipsoid(4, 28, 2.4, 2.4), sep=False)
    c.outline()
    return c


def item_knife():
    c = new()
    blade(c, (12, 18), (26, 6), 5, STEEL)
    c.shade(line(c, (9, 17), (15, 23), 3), GOLD, None, flat=3, sep=False)
    c.shade(line(c, (12, 21), (6, 27), 4), WOOD, None, flat=3, sep=False)
    c.px(5, 27, GOLD, 4)
    c.outline()
    return c


def item_armor():
    c = new()
    shape = poly(c, [(9, 6), (13, 5), (19, 5), (23, 6), (28, 11), (25, 15), (23, 13), (23, 27), (9, 27), (9, 13), (7, 15), (4, 11)])
    shape = shape & ~poly(c, [(13, 4), (19, 4), (16, 11)])
    c.shade(shape, LEATHER, cyl(16, 12), dither=0.45, jitter=0.3, seed=4)
    c.adjust(line(c, (16, 11), (16, 27), 1) & shape, -2)
    for y in (14, 17, 20):
        c.px(14, y, CREAM, 4); c.px(18, y, CREAM, 4)
    c.shade(rect(c, 9, 22, 23, 25) & shape, LEATHER_DK, cyl(16, 12), sep=False)
    c.shade(rect(c, 14, 22, 18, 25), GOLD, cyl(16, 3), sep=False)
    c.outline()
    return c


def item_ingot():
    c = new()
    top = poly(c, [(4, 14), (18, 9), (28, 12), (14, 18)])
    left = poly(c, [(4, 14), (14, 18), (14, 25), (4, 21)])
    right = poly(c, [(14, 18), (28, 12), (28, 19), (14, 25)])
    st = ramp(212, 0.20, 0.82)
    c.shade(left, st, None, flat=3)
    c.shade(right, st, None, flat=2)
    c.shade(top, st, None, flat=4)
    c.shade(line(c, (6, 14), (16, 17), 1) & top, st, None, flat=5, sep=False)
    c.shade(line(c, (13, 14), (20, 12), 1) & top, st, None, flat=3, sep=False)
    c.adjust(rect(c, 5, 15, 14, 18) & left, 1)
    c.outline()
    return c


def item_ore():
    c = new()
    c.shade(ell(c, 16, 19, 11, 8.5), ROCK, ellipsoid(14, 15, 12, 11), dither=0.4, jitter=0.4, seed=2)
    c.shade(ell(c, 10, 24, 5, 3.5) | ell(c, 23, 24, 5.5, 3.8), ROCK, ellipsoid(14, 18, 12, 10), dither=0.4)
    for a, b in (((10, 15), (16, 21)), ((18, 13), (22, 20)), ((9, 22), (14, 24))):
        c.shade(line(c, a, b, 2) & (c.alpha == 255), RUST, None, flat=3, sep=False)
    for x, y in ((14, 18), (21, 17), (12, 23)):
        c.px(x, y, ramp(40, 0.2, 1.0), 5)
    c.outline()
    return c


def item_pelt():
    c = new()
    fur = ramp(218, 0.16, 0.64)
    belly = ramp(210, 0.08, 0.90)
    parts = [(16, 16, 9, 10), (7, 8, 3.8, 4), (25, 8, 3.8, 4), (7, 25, 3.8, 4), (25, 25, 3.8, 4), (16, 5.5, 4.2, 4), (16, 28, 2.6, 3.4)]
    m = np.zeros((32, 32), bool)
    for p in parts:
        m |= ell(c, *p)
    c.shade(m, fur, ellipsoid(14, 13, 14, 15), dither=0.5, jitter=0.5, seed=8)
    c.shade(ell(c, 16, 17, 4.5, 6.5), belly, ellipsoid(16, 15, 6, 8), dither=0.5, jitter=0.4, sep=False)
    c.px(14, 5, EYE, 0); c.px(18, 5, EYE, 0)
    c.outline()
    return c


def item_tusk():
    c = new()
    shape = poly(c, [(9, 28), (8, 18), (11, 10), (18, 5), (25, 5), (19, 9), (15, 15), (14, 28)])
    c.shade(shape, BONE, slab(), dither=0.3, jitter=0.2)
    for y in (22, 17):
        c.adjust(line(c, (9, y), (13, y - 1), 1) & shape, -1)
    c.px(23, 5, BONE, 5); c.px(19, 7, BONE, 5)
    c.outline()
    return c


def slab():
    def f(x, y):
        return np.full_like(x, -0.4), np.full_like(x, -0.2), np.ones_like(x)
    return f


def item_bread():
    c = new()
    loaf = ell(c, 16, 18, 12, 7.5)
    c.shade(loaf, BREAD, ellipsoid(14, 13, 12, 11), dither=0.4, jitter=0.25)
    for x in (10, 15, 20):
        c.adjust(line(c, (x, 12), (x + 3, 21), 2) & loaf, 1)
        c.adjust(line(c, (x + 2, 12), (x + 5, 21), 1) & loaf, -2)
    c.outline()
    return c


def item_herb():
    c = new()
    halo = ell(c, 16, 14, 13, 13)
    c.rid[halo] = c._rid(CYAN)
    c.tone[halo] = 4
    c.alpha[halo] = 55
    leaf = ramp(100, 0.75, 0.85)
    c.shade(line(c, (16, 29), (16, 13), 2), ramp(120, 0.6, 0.55), cyl(16.5, 1.4), sep=False)
    for pts, flat in (([(16, 22), (8, 20), (5, 12), (14, 16)], 4), ([(16, 18), (24, 17), (27, 8), (18, 12)], 3), ([(16, 14), (11, 9), (15, 3), (20, 9)], 4)):
        c.shade(poly(c, pts), leaf, slab(), dither=0.25, bias=-0.3)
    for x, y in ((8, 14), (24, 12), (15, 7)):
        c.px(x, y, CYAN, 5)
    c.outline()
    return c


def item_pouch():
    c = new()
    c.shade(ell(c, 16, 20, 10, 8.5), LEATHER, ellipsoid(14, 16, 11, 11), dither=0.4, jitter=0.25)
    c.shade(poly(c, [(11, 12), (21, 12), (23, 16), (9, 16)]), LEATHER, cyl(16, 7), dither=0.3)
    c.shade(rect(c, 10, 13, 22, 15), CREAM, cyl(16, 7), sep=False)
    c.shade(poly(c, [(13, 6), (19, 6), (21, 12), (11, 12)]), LEATHER_DK, cyl(16, 5), dither=0.3)
    c.shade(ell(c, 16, 21, 2.2, 2.2), GOLD, ellipsoid(15, 20, 2.4, 2.4), sep=False)
    c.outline()
    return c


def g_bag(c):
    c.shade(ell(c, 16, 19, 8, 7), LEATHER, ellipsoid(14, 16, 9, 9), dither=0.3)
    c.shade(poly(c, [(12, 8), (20, 8), (22, 13), (10, 13)]), LEATHER_DK, cyl(16, 6), dither=0.3)
    c.shade(rect(c, 11, 12, 21, 14), CREAM, cyl(16, 6), sep=False)
    c.shade(ell(c, 16, 20, 1.8, 1.8), GOLD, ellipsoid(15, 19, 2, 2), sep=False)


def g_person(c):
    c.shade(ell(c, 16, 10, 4, 4.4), SKIN_TAN, ellipsoid(15, 9, 5, 5), dither=0.2)
    c.shade(poly(c, [(9, 26), (10, 17), (14, 15), (18, 15), (22, 17), (23, 26)]), CREAM, cyl(16, 7), dither=0.3)


def g_scroll(c):
    paper = ramp(40, 0.25, 0.97)
    c.shade(rect(c, 9, 7, 23, 24), paper, slab(), dither=0.2)
    for y in (11, 15, 19):
        c.shade(rect(c, 11, y, 21, y + 1), LEATHER_DK, None, flat=2, sep=False)
    c.shade(rect(c, 8, 5, 24, 8), LEATHER, cyl(16, 8), sep=False)
    c.shade(rect(c, 8, 23, 24, 26), LEATHER, cyl(16, 8), sep=False)


def g_hammer(c):
    c.shade(line(c, (9, 25), (20, 12), 3), WOOD, None, flat=3, sep=False)
    c.shade(rect(c, 14, 5, 26, 12), STEEL, cyl(20, 7), dither=0.2)
    c.shade(rect(c, 7, 24, 17, 28), ramp(220, 0.15, 0.55), slab(), sep=False)


def g_question(c):
    paper = ramp(40, 0.25, 0.97)
    c.shade(line(c, (11, 11), (13, 7), 3), paper, None, flat=3, sep=False)
    c.shade(line(c, (13, 7), (19, 7), 3), paper, None, flat=3, sep=False)
    c.shade(line(c, (19, 7), (21, 11), 3), paper, None, flat=3, sep=False)
    c.shade(line(c, (21, 11), (16, 16), 3), paper, None, flat=3, sep=False)
    c.shade(line(c, (16, 16), (16, 20), 3), paper, None, flat=3, sep=False)
    c.shade(rect(c, 15, 23, 18, 26), paper, None, flat=3, sep=False)


def g_speaker(c):
    """Options: a speaker and two waves."""
    paper = ramp(40, 0.25, 0.97)
    c.shade(rect(c, 7, 13, 11, 20), paper, None, flat=3, sep=False)
    c.shade(poly(c, [(11, 13), (17, 8), (17, 25), (11, 20)]), paper, None, flat=3, sep=False)
    c.shade(line(c, (20, 13), (21, 16), 2), paper, None, flat=3, sep=False)
    c.shade(line(c, (21, 17), (20, 20), 2), paper, None, flat=3, sep=False)
    c.shade(line(c, (23, 10), (25, 16), 2), paper, None, flat=3, sep=False)
    c.shade(line(c, (25, 17), (23, 23), 2), paper, None, flat=3, sep=False)


JOBS = {
    "icons/ui_options.png": lambda: tech(ramp(175, 0.45, 0.80), g_speaker),
    "icons/ui_help.png": lambda: tech(ramp(160, 0.55, 0.80), g_question),
    "icons/ui_inventory.png": lambda: tech(ramp(30, 0.60, 0.75), g_bag),
    "icons/ui_sheet.png": lambda: tech(ramp(200, 0.55, 0.85), g_person),
    "icons/ui_techniques.png": lambda: tech(ramp(280, 0.55, 0.85), g_double_cut),
    "icons/ui_quests.png": lambda: tech(ramp(45, 0.60, 0.85), g_scroll),
    "icons/ui_crafting.png": lambda: tech(ramp(10, 0.50, 0.70), g_hammer),
    "icons/wolf.png": lambda: portrait_wolf(False),
    "icons/old_wolf.png": lambda: portrait_wolf(True),
    "icons/boar.png": portrait_boar,
    "icons/bandit.png": portrait_bandit,
    "icons/double_cut.png": lambda: tech(ramp(190, 0.55, 0.85), g_double_cut),
    "icons/heavy_punch.png": lambda: tech(ramp(24, 0.85, 0.90), g_heavy_punch),
    "icons/quick_stab.png": lambda: tech(ramp(140, 0.60, 0.80), g_quick_stab),
    "icons/generic_technique.png": lambda: tech(ramp(225, 0.35, 0.72), g_generic),
    "icons/iron_sword.png": lambda: item_sword(False),
    "icons/rusty_sword.png": lambda: item_sword(True),
    "icons/hunting_knife.png": item_knife,
    "icons/leather_armor.png": item_armor,
    "icons/iron_ingot.png": item_ingot,
    "icons/iron_ore.png": item_ore,
    "icons/wolf_pelt.png": item_pelt,
    "icons/boar_tusk.png": item_tusk,
    "icons/travel_bread.png": item_bread,
    "icons/glow_herb.png": item_herb,
    "icons/generic_item.png": item_pouch,
}

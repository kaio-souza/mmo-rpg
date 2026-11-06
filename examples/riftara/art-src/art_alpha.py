"""Alpha world provisional art: new floors and walls, the creatures, NPCs, nodes and drops of the five new zones."""
import numpy as np
from artlib import Canvas, ramp, ell, rect, poly, line, cyl, ellipsoid, hash2, pnoise, BAYER
from art_chars import humanoid, quadruped_legs, staff, beard, apron, basket, dagger, SKIN, SKIN_TAN, EYE, GOLD, STEEL, LEATHER, LEATHER_DK, CREAM, BLUE, GREEN, RED, CYAN, WOOD, STRAW, HERB
from art_icons import new, badge, slab, BONE
from art_tiles import uv, paint_ground, scatter, GRASS, GRASS_DK, DIRT, STONE, MOSS, FLOWER_W, FLOWER_Y, FLOWER_P, PEBBLE, WATER
from art_phase10 import portrait, goblin_ears, club, SPIDER, GOBLIN, PURPLE

MUD = ramp(36, 0.45, 0.42)
MUD_WET = ramp(80, 0.35, 0.38)
CAVE = ramp(240, 0.10, 0.42)
PLANK = ramp(30, 0.50, 0.66)
TILLED = ramp(24, 0.55, 0.50)
PINE = ramp(150, 0.50, 0.42)
BUSH = ramp(115, 0.55, 0.55)
REED = ramp(70, 0.50, 0.62)
TIMBER = ramp(28, 0.50, 0.55)
TIMBER_TOP = ramp(32, 0.35, 0.72)
ROCK = ramp(230, 0.12, 0.58)
ROCK_DK = ramp(235, 0.14, 0.40)
COPPER = ramp(18, 0.70, 0.80)
SILVER = ramp(210, 0.06, 0.95)
FROG = ramp(95, 0.55, 0.60)
SLIME = ramp(110, 0.55, 0.85)
WISP = ramp(186, 0.55, 1.0)
BAT = ramp(285, 0.30, 0.38)
KOBOLD = ramp(14, 0.55, 0.62)
HAG = ramp(90, 0.18, 0.62)
DIRE = ramp(240, 0.20, 0.36)


# --- Floors (64x32) ------------------------------------------------------------

def flowers():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    f = 0.5 * pnoise(u, v, 3, 211) + 0.5 * pnoise(u, v, 8, 212)
    paint_ground(c, inside, GRASS, 0.25 + 0.5 * f, lo=2.2, hi=3.6, dither=0.35)
    for x, y, i in scatter(c, inside, 14, 221):
        k = hash2(i, 6, 221)
        r = FLOWER_W if k < 0.33 else FLOWER_Y if k < 0.66 else FLOWER_P
        c.px(x, y, r, 4)
        c.px(x + 1, y, r, 5)
        c.px(x, y - 1, r, 3)
        c.px(x, y + 1, GRASS, 2)
    return c


def mud():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    f = 0.55 * pnoise(u, v, 3, 231) + 0.45 * pnoise(u, v, 7, 232)
    paint_ground(c, inside, MUD, 0.2 + 0.5 * f, lo=1.5, hi=3.4, dither=0.45)
    wet = inside & (pnoise(u, v, 4, 233) > 0.62)
    c.rid[wet] = c._rid(MUD_WET)
    for x, y, i in scatter(c, inside, 5, 241):
        c.px(x, y, ramp(204, 0.4, 0.7), 4)
        c.px(x + 1, y, ramp(204, 0.4, 0.7), 3)
    return c


def cave_floor():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    f = 0.5 * pnoise(u, v, 4, 251) + 0.5 * pnoise(u, v, 10, 252)
    paint_ground(c, inside, CAVE, 0.2 + 0.6 * f, lo=1.4, hi=3.6, dither=0.5)
    for x, y, i in scatter(c, inside, 7, 261):
        c.px(x, y, PEBBLE, 3)
        c.px(x + 1, y + 1, CAVE, 1)
    return c


def wood_floor():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    n = 5
    board = np.floor(v * n).astype(int)
    fv = (v * n) % 1.0
    tone = 2.6 + 1.2 * hash2(board, board * 3, 271) + 0.6 * (pnoise(u, v, 12, 272) - 0.5)
    paint_ground(c, inside, PLANK, (tone - 1.0) / 4.0, lo=1.0, hi=5.0, dither=0.3)
    seam = inside & (fv < 0.09)
    c.tone[seam] = 1
    ends = inside & (((u * 2 + hash2(board, 1, 273)) % 1.0) < 0.03)
    c.tone[ends] = 1
    return c


def farmland():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    rows = (u * 4) % 1.0
    f = 0.3 + 0.4 * (rows > 0.5) + 0.2 * pnoise(u, v, 8, 281)
    paint_ground(c, inside, TILLED, f, lo=1.4, hi=3.8, dither=0.35)
    for x, y, i in scatter(c, inside, 10, 291):
        c.px(x, y, HERB, 3)
        c.px(x, y - 1, HERB, 4)
    return c


# --- Walls (64x72, anchor (32, 56)) ----------------------------------------------

def block(face, top, course, brick, seed, moss=False, rough=0.25):
    """A full tile block like the stone wall, with its own colors and joints."""
    c = Canvas(64, 72)
    left = poly(c, [(0, 16), (32, 32), (32, 72), (0, 56)])
    right = poly(c, [(32, 32), (64, 16), (64, 56), (32, 72)])
    topm = poly(c, [(32, 0), (64, 16), (32, 32), (0, 16)])
    for mask, top_y, x_local, base, s in [
        (left, lambda xs: 16 + xs / 2.0, lambda xs: xs, 3.4, seed),
        (right, lambda xs: 32 - (xs - 32) / 2.0, lambda xs: xs - 32, 2.2, seed + 1),
    ]:
        ys, xs = np.nonzero(mask)
        rel = ys - top_y(xs)
        crs = np.floor(rel / course).astype(int)
        xl = x_local(xs) + (brick // 2) * (crs % 2)
        b = np.floor(xl / brick).astype(int)
        t = base + 1.0 * (hash2(b, crs, s) - 0.5) + rough * (hash2(xs // 2, ys // 3, s + 3) - 0.5)
        t = t + (BAYER[ys % 4, xs % 4] - 0.5) * 0.25
        idx = np.clip(np.floor(t + 0.5), 1, 5).astype(int)
        idx = np.where(((rel % course) == 0) | ((xl % brick) == 0), 1, idx)
        c.rid[ys, xs] = c._rid(face)
        c.tone[ys, xs] = idx
        c.alpha[ys, xs] = 255
        if moss:
            m = (rel > 30) & (hash2(xs // 2, ys // 2, s + 7) * (rel / 40.0) > 0.8)
            c.rid[ys[m], xs[m]] = c._rid(MOSS)
    ys, xs = np.nonzero(topm)
    t = 4.0 + 0.8 * (hash2(xs // 4, ys // 4, seed + 9) - 0.5) + (BAYER[ys % 4, xs % 4] - 0.5) * 0.4
    c.rid[ys, xs] = c._rid(top)
    c.tone[ys, xs] = np.clip(np.floor(t + 0.5), 1, 5).astype(int)
    c.alpha[ys, xs] = 255
    lip = (line(c, (0, 16), (32, 32), 1) | line(c, (32, 32), (64, 16), 1)) & topm
    c.tone[lip] = 1
    c.outline()
    return c


def timber_wall():
    return block(TIMBER, TIMBER_TOP, 72, 6, 301)


def rock_wall():
    return block(ROCK_DK, ROCK, 11, 21, 311, rough=0.8)


def pine():
    c = Canvas(64, 72)
    bark = ramp(24, 0.45, 0.38)
    c.shadow(32, 57, 14, 4)
    c.shade(poly(c, [(29, 58), (35, 58), (34, 44), (30, 44)]), bark, cyl(32, 3), dither=0.3)
    for i, (y0, w) in enumerate(((46, 17), (34, 14), (23, 11), (13, 7))):
        tier = poly(c, [(32 - w, y0), (32, y0 - 16), (32 + w, y0)])
        c.shade(tier, PINE, cyl(29, w), dither=0.45, jitter=0.4, seed=7 + i)
    c.outline()
    return c


def boulder():
    c = Canvas(64, 72)
    c.shadow(32, 57, 18, 5)
    for cx, cy, rx, ry, s in ((24, 51, 10, 7, 1), (41, 52, 9, 6, 2), (32, 43, 14, 12, 3)):
        c.shade(ell(c, cx, cy, rx, ry), ROCK, ellipsoid(cx - 2, cy - 4, rx * 1.1, ry * 1.2), dither=0.35, jitter=0.4, seed=s)
    c.adjust(line(c, (27, 38), (33, 47), 1) & ell(c, 32, 43, 14, 12), -1)
    c.outline()
    return c


def bush():
    c = Canvas(64, 72)
    c.shadow(32, 57, 16, 4)
    clumps = ell(c, 24, 49, 10, 8) | ell(c, 40, 49, 10, 8) | ell(c, 32, 42, 12, 10)
    c.shade(clumps, BUSH, ellipsoid(29, 40, 16, 14), dither=0.5, jitter=0.5, seed=12)
    for x, y in ((25, 44), (36, 40), (41, 48), (29, 50)):
        c.px(x, y, RED, 4)
        c.px(x + 1, y, RED, 3)
    c.outline()
    return c


def reeds():
    c = Canvas(64, 72)
    c.shadow(32, 57, 15, 3)
    for i in range(11):
        x = 18 + i * 2.8 + 2 * hash2(i, 1, 401)
        top = 22 + 14 * hash2(i, 2, 401)
        lean = 4 * (hash2(i, 3, 401) - 0.5)
        c.shade(line(c, (x, 57), (x + lean, top), 1), REED, None, flat=3 + i % 2, sep=False)
        if i % 3 == 0:
            c.shade(ell(c, x + lean, top + 2, 1.4, 3.2), LEATHER_DK, None, flat=3, sep=False)
    c.outline()
    return c


def brazier_unlit():
    c = Canvas(64, 72)
    stone = ramp(235, 0.12, 0.70)
    iron = ramp(220, 0.15, 0.45)
    ash = ramp(20, 0.10, 0.35)
    c.shadow(32, 57, 12, 4)
    c.shade(poly(c, [(26, 58), (38, 58), (36, 40), (28, 40)]), stone, cyl(32, 6), dither=0.3)
    c.shade(ell(c, 32, 38, 12, 4.5), iron, ellipsoid(31, 36, 12, 6), dither=0.2)
    c.shade(ell(c, 32, 36.5, 9, 2.6), ash, ellipsoid(32, 35, 9, 4), dither=0.4, jitter=0.4, sep=False)
    for x in (28, 32, 35):
        c.px(x, 36, ramp(20, 0.6, 0.55), 3)
    c.outline()
    return c


# --- Gathering nodes (64x64) ------------------------------------------------------

def vein(ore):
    c = Canvas(64, 64)
    rock = ramp(235, 0.14, 0.72)
    c.shadow(32, 57, 20, 5)
    for cx, cy, rx, ry, seed in ((19, 51, 8, 6, 1), (45, 52, 9.5, 7, 2), (32, 43, 15, 13, 3)):
        c.shade(ell(c, cx, cy, rx, ry), rock, ellipsoid(cx - 2, cy - 4, rx * 1.1, ry * 1.2), dither=0.35, jitter=0.4, seed=seed)
    for a, b in (((24, 35), (30, 41)), ((34, 33), (38, 44)), ((26, 48), (33, 50)), ((40, 40), (45, 47)), ((17, 49), (21, 54)), ((44, 49), (50, 54))):
        c.shade(line(c, a, b, 2) & (c.alpha == 255), ore, None, flat=3, sep=False)
        c.shade(line(c, (a[0] - 1, a[1] - 1), (b[0] - 1, b[1] - 1), 1) & (c.alpha == 255), ore, None, flat=5, sep=False)
    c.outline()
    return c


def herb_patch():
    c = Canvas(64, 64)
    c.shadow(32, 57, 15, 4)
    leaf = ramp(100, 0.70, 0.72)
    for i in range(9):
        x = 20 + i * 3
        top = 38 + 8 * hash2(i, 5, 421)
        c.shade(poly(c, [(x - 3, 57), (x + 1, top), (x + 4, 57)]), leaf, slab(), dither=0.3, seed=i)
    for x, y in ((22, 40), (31, 37), (40, 41), (27, 45), (36, 46)):
        c.shade(ell(c, x, y, 2.2, 2.2), FLOWER_Y, ellipsoid(x, y - 1, 2.2, 2.2), sep=False)
        c.px(x, y, ramp(20, 0.8, 0.9), 4)
    c.outline()
    return c


# --- Creatures (64x64) ------------------------------------------------------------

def dire_wolf():
    c = Canvas(64, 64)
    fur, dark, belly = DIRE, ramp(250, 0.25, 0.22), ramp(230, 0.12, 0.55)
    c.shadow(31, 58, 19, 5)
    cx, cy, rx, ry = 29, 41, 16.5, 9.5
    quadruped_legs(c, fur, dark, (26, 35), (19, 39), 46, 57, belly)
    c.shade(poly(c, [(14, 38), (8, 37), (3, 31), (2, 25), (6, 26), (10, 31), (15, 34)]), fur, cyl(9, 7), dither=0.4, jitter=0.3)
    body = ell(c, cx, cy, rx, ry)
    c.shade(body, fur, ellipsoid(cx, cy - 2, rx, ry + 2), dither=0.45, jitter=0.4, seed=8)
    for x in range(16, 40, 4):
        c.shade(poly(c, [(x, cy - ry + 3), (x + 2, cy - ry - 3), (x + 4, cy - ry + 3)]), dark, None, flat=2, sep=False)
    for pts in ([(41, 30), (42, 20), (47, 29)], [(46, 29), (50, 20), (53, 30)]):
        c.shade(poly(c, pts), dark, None, flat=2)
    c.shade(ell(c, 47, 34, 8, 6.8), fur, ellipsoid(47, 32, 8, 8), dither=0.4, jitter=0.3)
    snout = poly(c, [(50, 32), (59, 36), (59, 41), (50, 41)])
    c.shade(snout, fur, ellipsoid(52, 35, 8, 7), dither=0.35)
    c.shade(rect(c, 58, 35, 61, 38), ramp(250, 0.25, 0.2), None, flat=3, sep=False)
    for x in (52, 55):
        c.px(x, 41, BONE, 5)
        c.px(x, 42, BONE, 4)
    c.px(50, 32, RED, 5)
    c.px(51, 32, RED, 4)
    c.outline()
    return c


def bandit_scout():
    def bow(c):
        c.shade(line(c, (47, 18), (50, 34), 1) | line(c, (50, 34), (47, 50), 1), WOOD, None, flat=3, sep=False)
        c.shade(line(c, (47, 18), (47, 50), 1), CREAM, None, flat=4, sep=False)
    return humanoid(Canvas(64, 64), dict(tunic=ramp(110, 0.35, 0.40), sleeve=ramp(110, 0.25, 0.32), pants=ramp(30, 0.25, 0.38), boots=LEATHER_DK, belt=True, head="hood", hood=ramp(120, 0.35, 0.34), skin=SKIN_TAN, mask=True, extra=[bow]))


def goblin_brute():
    def spikes(c):
        for x in (22, 42):
            c.shade(poly(c, [(x - 4, 31), (x, 24), (x + 4, 31)]), STEEL, None, flat=3)
    p = dict(tunic=ramp(15, 0.45, 0.40), pants=ramp(30, 0.35, 0.30), boots=LEATHER_DK, belt=True, hair=ramp(100, 0.4, 0.20), head="cap", skin=ramp(95, 0.45, 0.55), extra=[goblin_ears, club, spikes])
    return humanoid(Canvas(64, 64), p)


def bog_frog():
    c = Canvas(64, 64)
    belly = ramp(60, 0.35, 0.85)
    c.shadow(32, 57, 15, 4)
    for x in (18, 40):
        c.shade(ell(c, x + 3, 52, 7, 4), FROG, ellipsoid(x + 3, 50, 7, 5), dither=0.3)
    body = ell(c, 32, 45, 13, 10)
    c.shade(body, FROG, ellipsoid(30, 41, 13, 12), dither=0.45, jitter=0.4, seed=3)
    c.shade(ell(c, 32, 50, 9, 5), belly, ellipsoid(32, 48, 9, 6), dither=0.3, sep=False)
    for x in (25, 39):
        c.shade(ell(c, x, 35, 4.2, 4.2), FROG, ellipsoid(x - 1, 33, 4.5, 4.5), dither=0.2)
        c.shade(ell(c, x, 35, 2.4, 2.4), ramp(50, 0.9, 1.0), None, flat=4, sep=False)
        c.px(x, 35, EYE, 0)
    for x, y in ((28, 42), (36, 44), (31, 47)):
        c.px(x, y, ramp(100, 0.6, 0.35), 2)
    c.adjust(line(c, (25, 46), (39, 46), 1) & body, -2)
    c.outline()
    return c


def marsh_slime():
    c = Canvas(64, 64)
    c.shadow(32, 57, 15, 3)
    blob = ell(c, 32, 47, 15, 11) | ell(c, 32, 39, 10, 9)
    c.shade(blob, SLIME, ellipsoid(28, 38, 15, 15), dither=0.4, jitter=0.2)
    c.alpha[blob] = 215
    c.shade(ell(c, 26, 38, 3, 2), ramp(110, 0.15, 1.0), None, flat=5, sep=False)
    for x in (28, 36):
        c.px(x, 44, EYE, 0)
        c.px(x, 45, EYE, 0)
    c.shade(ell(c, 34, 51, 3, 2), ramp(30, 0.2, 0.9), None, flat=3, sep=False)
    c.outline()
    return c


def fen_wisp():
    c = Canvas(64, 64)
    halo = ell(c, 32, 34, 14, 14)
    c.rid[halo] = c._rid(WISP)
    c.tone[halo] = 4
    c.alpha[halo] = 70
    tail = poly(c, [(27, 38), (37, 38), (34, 52), (32, 58), (30, 52)])
    c.shade(tail, WISP, cyl(32, 5), dither=0.4, jitter=0.4)
    c.alpha[tail] = 170
    c.shade(ell(c, 32, 34, 7, 7), WISP, ellipsoid(30, 32, 7, 7), dither=0.2)
    c.shade(ell(c, 31, 33, 3, 3), ramp(186, 0.1, 1.0), None, flat=5, sep=False)
    return c


def swamp_hag():
    return humanoid(Canvas(64, 64), dict(robe=True, tunic=ramp(140, 0.35, 0.30), trim=ramp(80, 0.4, 0.5), hood=ramp(150, 0.30, 0.25), head="hood", skin=HAG, boots=LEATHER_DK, extra=[staff]))


def cave_bat():
    c = Canvas(64, 64)
    c.shadow(32, 58, 10, 2.5)
    for side in (-1, 1):
        wing = poly(c, [(32, 30), (32 + side * 24, 18), (32 + side * 20, 30), (32 + side * 26, 34), (32 + side * 14, 40), (32, 38)])
        c.shade(wing, BAT, slab(), dither=0.35, jitter=0.3)
        c.adjust(line(c, (32, 32), (32 + side * 22, 22), 1) & wing, -1)
    c.shade(ell(c, 32, 34, 6, 7), BAT, ellipsoid(31, 32, 6, 7), dither=0.3)
    for pts in ([(27, 29), (28, 21), (31, 28)], [(33, 28), (36, 21), (37, 29)]):
        c.shade(poly(c, pts), BAT, None, flat=3)
    c.px(30, 32, RED, 5)
    c.px(34, 32, RED, 5)
    c.px(31, 37, BONE, 5)
    c.px(33, 37, BONE, 5)
    c.outline()
    return c


def kobold(foreman):
    def helmet(c):
        hat = STEEL if foreman else ramp(45, 0.70, 0.85)
        c.shade(ell(c, 32, 14, 9.5, 5.5), hat, ellipsoid(31, 12, 10, 7), dither=0.25)
        if not foreman:
            c.shade(rect(c, 31, 6, 33, 10), CREAM, None, flat=4, sep=False)
            c.px(32, 5, ramp(45, 0.9, 1.0), 5)

    def pick(c):
        c.shade(line(c, (45, 52), (45, 24), 2), WOOD, cyl(45.5, 1.2), dither=0.2, sep=False)
        c.shade(poly(c, [(37, 26), (45, 22), (53, 26), (45, 25)]), STEEL, None, flat=3)

    def whip(c):
        c.shade(line(c, (45, 46), (50, 36), 1) | line(c, (50, 36), (54, 44), 1), LEATHER, None, flat=3, sep=False)

    p = dict(tunic=ramp(30, 0.45, 0.42) if not foreman else ramp(355, 0.5, 0.45), pants=ramp(30, 0.3, 0.32), boots=LEATHER_DK, belt=True, hair=KOBOLD, head="cap", skin=KOBOLD, extra=[helmet, whip if foreman else pick])
    return humanoid(Canvas(64, 64), p)


def cave_spider():
    c = Canvas(64, 64)
    body_r = ramp(215, 0.20, 0.40)
    leg = ramp(220, 0.18, 0.25)
    c.shadow(32, 57, 20, 4)
    for side in (-1, 1):
        for i, (kx, ky) in enumerate(((10, 34), (14, 40), (15, 47), (12, 53))):
            hip = (32 + side * 4, 44 + i)
            knee = (32 + side * (kx + 5), ky - 5)
            foot = (32 + side * (kx + 11), ky + 10 - i)
            c.shade(line(c, hip, knee, 2) | line(c, knee, foot, 1), leg, None, flat=2)
    c.shade(ell(c, 30, 39, 12, 9), body_r, ellipsoid(28, 36, 12, 11), dither=0.4, jitter=0.3)
    for x, y in ((25, 36), (30, 34), (35, 37), (29, 41)):
        c.px(x, y, ramp(186, 0.4, 0.9), 4)
    c.shade(ell(c, 33, 48, 6.5, 5), body_r, ellipsoid(32, 46, 6.5, 6), dither=0.3)
    for x in (30, 32, 34, 36):
        c.px(x, 47, ramp(186, 0.6, 1.0), 5)
    c.outline()
    return c


def rock_golem():
    c = Canvas(64, 64)
    c.shadow(32, 59, 20, 5)
    for x0 in (22, 35):
        c.shade(rect(c, x0, 46, x0 + 8, 58), ROCK_DK, cyl(x0 + 4, 5), dither=0.3, jitter=0.3)
    torso = poly(c, [(16, 22), (48, 22), (51, 36), (44, 48), (20, 48), (13, 36)])
    c.shade(torso, ROCK, ellipsoid(30, 30, 20, 18), dither=0.4, jitter=0.5, seed=21)
    for x in (9, 55):
        c.shade(ell(c, x, 36, 6, 11), ROCK_DK, ellipsoid(x - 1, 33, 6, 11), dither=0.4, jitter=0.4)
    c.shade(ell(c, 32, 17, 8, 6.5), ROCK, ellipsoid(31, 15, 8, 7), dither=0.3, jitter=0.4)
    c.adjust(line(c, (22, 28), (30, 40), 1) & torso, -2)
    c.adjust(line(c, (40, 26), (36, 44), 1) & torso, -2)
    c.shade(ell(c, 32, 33, 3.5, 3.5), ramp(30, 0.85, 1.0), None, flat=5, sep=False)
    c.px(29, 17, ramp(30, 0.9, 1.0), 5)
    c.px(35, 17, ramp(30, 0.9, 1.0), 5)
    c.outline()
    return c


# --- NPCs --------------------------------------------------------------------------

def helm(c):
    c.shade(ell(c, 32, 15, 9, 6), STEEL, ellipsoid(31, 13, 9.5, 7), dither=0.25)
    c.shade(rect(c, 31, 6, 33, 10), RED, None, flat=3, sep=False)


def spear(c):
    c.shade(line(c, (45, 56), (45, 10), 1), WOOD, None, flat=3, sep=False)
    c.shade(poly(c, [(43, 11), (45, 3), (47, 11)]), STEEL, None, flat=4)


def captain():
    return humanoid(Canvas(64, 64), dict(tunic=STEEL, sleeve=ramp(212, 0.2, 0.6), trim=RED, pants=ramp(355, 0.4, 0.4), boots=LEATHER_DK, belt=True, hair=ramp(24, 0.3, 0.3), head="cap", skin=SKIN_TAN, extra=[helm, spear]))


def ranger():
    def bow(c):
        c.shade(line(c, (16, 16), (12, 34), 1) | line(c, (12, 34), (16, 52), 1), WOOD, None, flat=3, sep=False)
        c.shade(line(c, (16, 16), (16, 52), 1), CREAM, None, flat=4, sep=False)
    return humanoid(Canvas(64, 64), dict(tunic=ramp(125, 0.45, 0.45), sleeve=LEATHER, pants=ramp(30, 0.3, 0.4), boots=LEATHER_DK, belt=True, head="hood", hood=ramp(125, 0.5, 0.38), skin=SKIN, extra=[bow]))


def foreman():
    def hard_hat(c):
        c.shade(ell(c, 32, 14, 10, 5.5), ramp(45, 0.80, 0.90), ellipsoid(31, 12, 10, 7), dither=0.25)
    return humanoid(Canvas(64, 64), dict(tunic=ramp(30, 0.45, 0.50), pants=ramp(215, 0.35, 0.40), boots=LEATHER_DK, belt=True, hair=ramp(24, 0.3, 0.3), head="cap", skin=SKIN_TAN, extra=[beard, hard_hat]))


def quartermaster():
    return humanoid(Canvas(64, 64), dict(tunic=ramp(0, 0.45, 0.45), pants=ramp(30, 0.3, 0.35), boots=LEATHER_DK, belt=False, hair=ramp(30, 0.2, 0.25), head="cap", skin=SKIN, extra=[apron, beard]))


def fen_elder():
    return humanoid(Canvas(64, 64), dict(robe=True, tunic=ramp(30, 0.35, 0.45), trim=ramp(90, 0.4, 0.6), hood=ramp(30, 0.35, 0.40), head="hood", skin=SKIN_TAN, boots=LEATHER_DK, extra=[beard, staff]))


def fen_trader():
    return humanoid(Canvas(64, 64), dict(robe=True, tunic=ramp(175, 0.40, 0.55), hood=GREEN, head="hat", skin=SKIN, boots=LEATHER_DK, extra=[basket]))


# --- Portraits (32x32) -------------------------------------------------------------

def face(body, eyes, extra=None):
    def f(c, _):
        if extra:
            extra(c)
        c.shade(ell(c, 16, 17, 9, 8.5), body, ellipsoid(15, 15, 10, 10), dither=0.35, jitter=0.3)
        for x in (12, 20):
            c.px(x, 16, eyes, 5)
            c.px(x, 17, eyes, 4)
    return f


def ears(c):
    for pts in ([(8, 13), (9, 4), (14, 10)], [(18, 10), (23, 4), (24, 13)]):
        c.shade(poly(c, pts), DIRE, None, flat=3)


def wings(c):
    for pts in ([(16, 14), (3, 8), (6, 22)], [(16, 14), (29, 8), (26, 22)]):
        c.shade(poly(c, pts), BAT, slab(), dither=0.3)


def hood(color):
    def f(c):
        c.shade(ell(c, 16, 14, 11, 11), color, ellipsoid(15, 12, 11, 11), dither=0.3)
    return f


def hat(color):
    def f(c):
        c.shade(ell(c, 16, 10, 10, 5), color, ellipsoid(15, 8, 10, 6), dither=0.25)
    return f


PORTRAITS = {
    "dire_wolf": (ramp(240, 0.3, 0.5), DIRE, RED, ears),
    "bandit_scout": (ramp(120, 0.4, 0.5), SKIN_TAN, EYE, hood(ramp(120, 0.35, 0.34))),
    "goblin_brute": (ramp(15, 0.5, 0.6), ramp(95, 0.45, 0.55), RED, None),
    "bog_frog": (ramp(95, 0.5, 0.6), FROG, ramp(50, 0.9, 1.0), None),
    "marsh_slime": (ramp(110, 0.5, 0.7), SLIME, EYE, None),
    "fen_wisp": (ramp(186, 0.4, 0.8), WISP, ramp(186, 0.1, 1.0), None),
    "swamp_hag": (ramp(140, 0.4, 0.5), HAG, ramp(60, 0.9, 1.0), hood(ramp(150, 0.3, 0.25))),
    "cave_bat": (ramp(285, 0.4, 0.5), BAT, RED, wings),
    "kobold_miner": (ramp(45, 0.6, 0.8), KOBOLD, ramp(50, 0.9, 1.0), hat(ramp(45, 0.7, 0.85))),
    "kobold_foreman": (ramp(355, 0.5, 0.6), KOBOLD, ramp(50, 0.9, 1.0), hat(STEEL)),
    "cave_spider": (ramp(215, 0.3, 0.6), ramp(215, 0.2, 0.4), ramp(186, 0.6, 1.0), None),
    "rock_golem": (ramp(30, 0.5, 0.7), ROCK, ramp(30, 0.9, 1.0), None),
}


# --- Items (32x32) -----------------------------------------------------------------

def item(shape_fn):
    def f():
        c = new()
        shape_fn(c)
        c.outline()
        return c
    return f


def i_badge(c):
    c.shade(poly(c, [(16, 4), (27, 9), (25, 22), (16, 28), (7, 22), (5, 9)]), ramp(30, 0.35, 0.70), slab(), dither=0.3)
    c.shade(ell(c, 16, 15, 4, 4), RED, ellipsoid(15, 14, 4, 4), sep=False)


def i_fang(c):
    c.shade(poly(c, [(10, 6), (20, 6), (18, 18), (14, 28), (12, 18)]), BONE, slab(), dither=0.3)


def i_wing(c):
    w = poly(c, [(4, 10), (28, 6), (24, 14), (28, 20), (16, 22), (12, 28)])
    c.shade(w, BAT, slab(), dither=0.35)
    c.adjust(line(c, (4, 10), (22, 18), 1) & w, -1)


def i_candle(c):
    c.shade(rect(c, 12, 12, 20, 28), CREAM, cyl(16, 4), dither=0.2)
    c.shade(poly(c, [(14, 11), (16, 3), (18, 11)]), ramp(40, 0.9, 1.0), None, flat=5)


def i_core(c):
    c.shade(ell(c, 16, 16, 11, 11), ROCK, ellipsoid(15, 14, 11, 11), dither=0.4, jitter=0.3)
    c.shade(poly(c, [(16, 8), (22, 16), (16, 24), (10, 16)]), ramp(30, 0.85, 1.0), slab(), sep=False)


def i_frog_leg(c):
    c.shade(line(c, (6, 26), (16, 14), 4) | line(c, (16, 14), (24, 20), 3), FROG, None, flat=3)
    c.shade(poly(c, [(22, 18), (29, 16), (28, 24)]), FROG, None, flat=4)


def i_gel(c):
    c.shade(ell(c, 16, 19, 11, 8) | ell(c, 16, 13, 6, 6), SLIME, ellipsoid(14, 13, 11, 12), dither=0.3)
    c.shade(ell(c, 12, 12, 2, 2), ramp(110, 0.1, 1.0), None, flat=5, sep=False)


def i_charm(c):
    c.shade(line(c, (8, 4), (16, 14), 1) | line(c, (24, 4), (16, 14), 1), LEATHER, None, flat=3, sep=False)
    c.shade(poly(c, [(16, 12), (24, 20), (16, 28), (8, 20)]), BONE, slab(), dither=0.3)
    c.px(16, 20, ramp(150, 0.7, 0.8), 5)


def i_essence(c):
    c.shade(ell(c, 16, 20, 8, 8), ramp(200, 0.15, 0.85), ellipsoid(15, 18, 8, 8), dither=0.2)
    c.shade(ell(c, 16, 21, 6, 6), WISP, ellipsoid(15, 19, 6, 6), sep=False)
    c.shade(rect(c, 13, 6, 19, 12), ramp(200, 0.15, 0.85), cyl(16, 3), sep=False)
    c.shade(rect(c, 13, 4, 19, 6), WOOD, None, flat=3, sep=False)


ITEMS = {
    "bandit_badge": i_badge,
    "dire_fang": i_fang,
    "bat_wing": i_wing,
    "kobold_candle": i_candle,
    "golem_core": i_core,
    "frog_leg": i_frog_leg,
    "slime_gel": i_gel,
    "hag_charm": i_charm,
    "wisp_essence": i_essence,
}


JOBS = {
    "sprites/tiles/flowers.png": flowers,
    "sprites/tiles/mud.png": mud,
    "sprites/tiles/cave_floor.png": cave_floor,
    "sprites/tiles/wood_floor.png": wood_floor,
    "sprites/tiles/farmland.png": farmland,
    "sprites/walls/timber_wall.png": timber_wall,
    "sprites/walls/rock_wall.png": rock_wall,
    "sprites/walls/pine.png": pine,
    "sprites/walls/boulder.png": boulder,
    "sprites/walls/bush.png": bush,
    "sprites/walls/reeds.png": reeds,
    "sprites/walls/brazier_unlit.png": brazier_unlit,
    "sprites/creatures/copper_vein.png": lambda: vein(COPPER),
    "sprites/creatures/silver_vein.png": lambda: vein(SILVER),
    "sprites/creatures/herb_patch.png": herb_patch,
    "sprites/creatures/dire_wolf.png": dire_wolf,
    "sprites/creatures/bandit_scout.png": bandit_scout,
    "sprites/creatures/goblin_brute.png": goblin_brute,
    "sprites/creatures/bog_frog.png": bog_frog,
    "sprites/creatures/marsh_slime.png": marsh_slime,
    "sprites/creatures/fen_wisp.png": fen_wisp,
    "sprites/creatures/swamp_hag.png": swamp_hag,
    "sprites/creatures/cave_bat.png": cave_bat,
    "sprites/creatures/kobold_miner.png": lambda: kobold(False),
    "sprites/creatures/kobold_foreman.png": lambda: kobold(True),
    "sprites/creatures/cave_spider.png": cave_spider,
    "sprites/creatures/rock_golem.png": rock_golem,
    "sprites/creatures/captain.png": captain,
    "sprites/creatures/ranger.png": ranger,
    "sprites/creatures/foreman.png": foreman,
    "sprites/creatures/quartermaster.png": quartermaster,
    "sprites/creatures/fen_elder.png": fen_elder,
    "sprites/creatures/fen_trader.png": fen_trader,
}
for _name, (_ring, _body, _eyes, _extra) in PORTRAITS.items():
    JOBS["icons/" + _name + ".png"] = (lambda r=_ring, b=_body, e=_eyes, x=_extra: portrait(r, b, face(b, e, x)))
for _name, _fn in ITEMS.items():
    JOBS["icons/" + _name + ".png"] = item(_fn)

"""Creatures, NPCs, the player body and equipment layers (64x64 frame, feet at (32,58))."""
import numpy as np
from artlib import Canvas, ramp, ell, rect, poly, line, cyl, ellipsoid, blade, hash2

SKIN = ramp(24, 0.42, 1.0)
SKIN_TAN = ramp(26, 0.50, 0.86)
EYE = ramp(250, 0.3, 0.45)
BLUSH = ramp(8, 0.45, 1.0)
GOLD = ramp(46, 0.80, 0.96)
STEEL = ramp(212, 0.16, 0.86)
RUST = ramp(22, 0.55, 0.62)
WOOD = ramp(26, 0.55, 0.60)
LEATHER = ramp(22, 0.58, 0.62)
LEATHER_DK = ramp(24, 0.45, 0.40)
CREAM = ramp(45, 0.22, 1.0)
STRAW = ramp(42, 0.58, 0.94)
BLUE = ramp(222, 0.62, 0.78)
GREEN = ramp(140, 0.50, 0.62)
RED = ramp(355, 0.68, 0.78)
CYAN = ramp(186, 0.60, 1.0)
HERB = ramp(100, 0.70, 0.80)


# Pose of the next humanoid drawn (phase 11 strips, art_anim.py): a
# function that never asks for a view still draws front, side and back.
POSE = {"view": "front", "step": 0}


def front_only(fn):
    """An extra seen only from the front (beard, apron): left out from the
    side and the back."""
    fn.front_only = True
    return fn


def humanoid(c, p):
    c.shadow(32, 58, 13, 4)
    robe = p.get("robe", False)
    sleeve = p.get("sleeve", p["tunic"])
    # Phase 11 strips: view "front" | "back" | "side" (facing right) and a
    # walk step (-2..2) that lifts one foot.
    view = p.get("view", POSE["view"])
    step = p.get("step", POSE["step"])
    legs = (29, 31) if view == "side" else (26, 34)
    if not robe:
        for i, x0 in enumerate(legs):
            lift = max(0, step if i == 0 else -step)
            c.shade(rect(c, x0, 45, x0 + 4, 56 - lift), p["pants"], cyl(x0 + 2, 2.4), dither=0.3)
            c.shade(rect(c, x0 - 1, 53 - lift, x0 + 5, 58 - lift), p["boots"], cyl(x0 + 2, 3.4), dither=0.25)
    if robe:
        torso = poly(c, [(24, 27), (40, 27), (42, 40), (46, 57), (18, 57), (22, 40)])
        c.shade(torso, p["tunic"], cyl(32, 14), dither=0.4, jitter=0.2)
        for fx, d in ((27, -2), (37, 2), (32, 0)):
            c.adjust(line(c, (fx, 41), (fx + d, 56), 1) & torso, -1)
        if p.get("trim"):
            c.shade(rect(c, 17, 54, 48, 57) & torso, p["trim"], cyl(32, 15), dither=0.2, sep=False)
        for fx in (28, 36):
            c.shade(ell(c, fx, 56.5, 3.2, 1.8), p["boots"], ellipsoid(fx, 55, 3.5, 3), dither=0.2)
    else:
        torso = poly(c, [(24, 27), (40, 27), (41, 34), (40, 46), (24, 46), (23, 34)])
        c.shade(torso, p["tunic"], cyl(32, 9.5), dither=0.4, jitter=0.2)
    if p.get("belt"):
        belt = rect(c, 24, 40, 40, 43) & torso
        c.shade(belt, LEATHER, cyl(32, 9.5), dither=0.2, sep=False)
        if view == "front":
            c.shade(rect(c, 30, 40, 34, 43), GOLD, cyl(32, 2.5), dither=0.1, sep=False)
    for x in ((31,) if view == "side" else (22, 42)):
        arm = rect(c, x - 2, 29, x + 3, 43)
        c.shade(arm, sleeve, cyl(x + 0.5, 3.2), dither=0.3, jitter=0.2)
        if p.get("trim"):
            c.shade(rect(c, x - 2, 40, x + 3, 43), p["trim"], cyl(x + 0.5, 3.2), sep=False)
        c.shade(ell(c, x + 0.5, 44.8, 2.7, 2.7), p["skin"], ellipsoid(x + 0.5, 44.8, 2.7, 2.7), dither=0.2)
    style = p["head"]
    head = ellipsoid(32, 20, 7, 7.5)
    if style == "hood":
        c.shade(poly(c, [(21, 27), (43, 27), (45, 33), (19, 33)]), p["hood"], cyl(32, 13), dither=0.3)
        c.shade(ell(c, 32, 18.5, 10.5, 10.8) | poly(c, [(25, 13), (32, 3), (39, 13)]), p["hood"], ellipsoid(32, 16, 11, 12), dither=0.35, jitter=0.2)
        c.shade(ell(c, 32, 21.5, 7.6, 7.6), p["hood"], None, flat=1)
        c.shade(ell(c, 32, 21.8, 6.4, 6.8), p["skin"], head, dither=0.2)
    elif style == "hat":
        c.shade(ell(c, 32, 20.5, 6.8, 7.2), p["skin"], head, dither=0.2)
        c.shade(ell(c, 32, 15, 13.5, 4.2), STRAW, ellipsoid(32, 12, 14, 7), dither=0.35, jitter=0.2)
        c.shade(ell(c, 32, 11.5, 7, 6.2), STRAW, ellipsoid(32, 10, 7.5, 7), dither=0.3)
        c.shade(rect(c, 25, 12, 39, 14) & ell(c, 32, 11.5, 7.2, 6.4), GREEN, cyl(32, 7), sep=False)
    else:
        c.shade(ell(c, 32, 17.5, 8.6, 9.2), p["hair"], ellipsoid(32, 15, 9, 10), dither=0.4, jitter=0.3)
        c.shade(ell(c, 32, 21, 6.6, 6.9), p["skin"], head, dither=0.2)
        if style == "bandana":
            c.shade(rect(c, 22, 13, 42, 17) & ell(c, 32, 17.5, 8.8, 9.4), RED, cyl(32, 9), dither=0.25)
            c.shade(poly(c, [(38, 14), (45, 12), (44, 19), (40, 17)]), RED, None, flat=2)
    ey = 21
    if view == "back":
        # The back of the head: hair (or the hood, the hat's brim) over the face.
        cover = p.get("hood", p.get("hair", STRAW))
        c.shade(ell(c, 32, 20, 7.2, 7.8), cover, head, dither=0.4, jitter=0.3, sep=False)
    elif view == "side":
        c.shade(ell(c, 28, 17.5, 5, 7.5) | ell(c, 32, 14, 6.5, 3.5), p.get("hood", p.get("hair", STRAW)), head, dither=0.4, sep=False)
        c.px(36, ey, EYE, 0)
        c.px(36, ey + 1, EYE, 0)
        c.px(37, ey + 2, BLUSH, 3)
        c.px(39, ey + 3, p["skin"], 2)
    else:
        for ex in (29, 35):
            c.px(ex, ey, EYE, 0)
            c.px(ex, ey + 1, EYE, 0)
        c.px(28, ey + 2, BLUSH, 3)
        c.px(36, ey + 2, BLUSH, 3)
        c.px(32, ey + 4, p["skin"], 2)
    if p.get("mask") and view == "front":
        c.shade(poly(c, [(26, 23), (38, 23), (36, 27), (28, 27)]), RED, cyl(32, 6), dither=0.2)
    for fn in p.get("extra", []):
        if view != "front" and getattr(fn, "front_only", False):
            continue
        fn(c)
    c.outline()
    return c


def staff(c):
    c.shade(line(c, (45, 18), (45, 56), 2), WOOD, cyl(45.5, 1.4), dither=0.2, sep=False)
    c.shade(ell(c, 45.5, 15, 3.4, 3.4), CYAN, ellipsoid(45.5, 15, 3.4, 3.4), dither=0.1)
    c.px(44, 14, CYAN, 5)
    c.shade(ell(c, 45.5, 44.8, 2.7, 2.7), SKIN, ellipsoid(45.5, 44.8, 2.7, 2.7))


@front_only
def beard(c):
    c.shade(poly(c, [(27, 24), (37, 24), (35, 31), (32, 33), (29, 31)]), ramp(210, 0.05, 1.0), cyl(32, 5), dither=0.4, sep=False)


def basket(c):
    c.shade(ell(c, 19.5, 48, 5.5, 4), STRAW, ellipsoid(19.5, 46, 6, 5), dither=0.4, jitter=0.2)
    for i, x in enumerate((16, 19, 22)):
        c.px(x, 43, HERB, 3 + i % 2)
        c.px(x, 44, HERB, 4)
        c.px(x + 1, 43, HERB, 5)
    c.adjust(line(c, (15, 49), (24, 49), 1), -1)


@front_only
def apron(c):
    c.shade(poly(c, [(27, 36), (37, 36), (39, 55), (25, 55)]), CREAM, cyl(32, 8), dither=0.3, sep=False)
    c.shade(line(c, (28, 36), (28, 30), 1) | line(c, (36, 36), (36, 30), 1), CREAM, None, flat=3, sep=False)


def dagger(c):
    blade(c, (43.5, 44), (47.5, 34), 3, STEEL)
    c.shade(line(c, (41, 44), (46, 43), 2), GOLD, None, flat=3, sep=False)
    c.shade(rect(c, 42, 44, 44, 48), LEATHER_DK, None, flat=3, sep=False)


def guide():
    return humanoid(Canvas(64, 64), dict(robe=True, tunic=BLUE, trim=GOLD, hood=BLUE, head="hood", skin=SKIN, boots=LEATHER_DK, extra=[beard, staff]))


def herbalist():
    return humanoid(Canvas(64, 64), dict(robe=True, tunic=GREEN, hood=GREEN, head="hat", skin=SKIN_TAN, boots=LEATHER_DK, extra=[apron, basket]))


def bandit():
    return humanoid(Canvas(64, 64), dict(tunic=ramp(20, 0.5, 0.42), sleeve=ramp(30, 0.30, 0.46), pants=ramp(30, 0.25, 0.42), boots=LEATHER_DK, belt=True, hair=ramp(24, 0.4, 0.38), head="bandana", mask=True, skin=SKIN_TAN, extra=[dagger]))


def player():
    return humanoid(Canvas(64, 64), dict(tunic=ramp(42, 0.20, 0.97), pants=ramp(30, 0.18, 0.88), boots=ramp(26, 0.25, 0.74), belt=True, hair=ramp(28, 0.30, 0.86), head="cap", skin=SKIN))


def quadruped_legs(c, fur, dark, xs_far, xs_near, y0, y1, paw):
    for x in xs_far:
        c.shade(rect(c, x, y0, x + 3, y1), dark, cyl(x + 1.5, 1.8), dither=0.2)
    for x in xs_near:
        c.shade(rect(c, x, y0, x + 4, y1), fur, cyl(x + 2, 2.4), dither=0.3, jitter=0.2, lift=-0.8)
        c.shade(rect(c, x, y1 - 2, x + 4, y1), paw, cyl(x + 2, 2.4), sep=False, lift=-0.5)


def wolf(old=False):
    c = Canvas(64, 64)
    fur = ramp(210, 0.05, 0.84) if old else ramp(218, 0.16, 0.66)
    dark = ramp(240, 0.12, 0.50) if old else ramp(246, 0.20, 0.42)
    belly = ramp(40, 0.05, 1.0) if old else ramp(210, 0.08, 0.92)
    k = 1.08 if old else 1.0
    c.shadow(31, 58, 17 * k, 4.5)
    cx, cy, rx, ry = 29, 42, 15 * k, 8.5 * k
    quadruped_legs(c, fur, dark, (26, 35), (19, 39), 46, 57, belly)
    tail = poly(c, [(15, 39), (9, 38), (4, 33), (3, 27), (7, 28), (10, 33), (16, 35)])
    c.shade(tail, fur, cyl(9, 7), dither=0.4, jitter=0.3)
    c.shade(ell(c, 4.5, 28, 2, 2.6), belly, None, flat=4, sep=False)
    body = ell(c, cx, cy, rx, ry)
    c.shade(body, fur, ellipsoid(cx, cy - 2, rx, ry + 2), dither=0.45, jitter=0.35, seed=3)
    c.shade(body & (c.ys > cy + ry * 0.3), belly, ellipsoid(cx, cy - 2, rx, ry + 2), dither=0.4, sep=False)
    c.shade(ell(c, 41, 40, 6.5, 7) & ~rect(c, 0, 0, 0, 0), fur, ellipsoid(41, 38, 7, 8), dither=0.4, jitter=0.3, sep=False)
    c.shade(ell(c, 40.5, 43, 4.2, 4.6), belly, ellipsoid(40, 41, 5, 6), dither=0.4, sep=False)
    for pts in ([(41, 31), (42, 22), (47, 30)], [(46, 30), (50, 22), (53, 31)]):
        c.shade(poly(c, pts), dark if pts[0][0] == 46 else fur, None, flat=2 if pts[0][0] == 46 else 3)
        c.shade(poly(c, [(pts[0][0] + 1.5, pts[0][1] - 1), (pts[1][0], pts[1][1] + 4), (pts[2][0] - 1.5, pts[2][1] - 1)]), belly, None, flat=2, sep=False)
    head = ell(c, 47, 35, 7.5, 6.3)
    c.shade(head, fur, ellipsoid(47, 33, 8, 7.5), dither=0.4, jitter=0.3, sep=True)
    snout = poly(c, [(50, 33), (58, 37), (58, 41), (50, 41)])
    c.shade(snout, belly if not old else fur, ellipsoid(52, 36, 8, 7), dither=0.35)
    c.shade(rect(c, 57, 36, 60, 39), ramp(250, 0.25, 0.25), None, flat=3, sep=False)
    c.px(57, 36, ramp(250, 0.25, 0.25), 5)
    c.adjust(line(c, (51, 40), (57, 40), 1) & snout, -2)
    eye = ramp(48, 0.9, 1.0)
    c.px(50, 33, EYE, 0)
    c.px(51, 33, eye, 5)
    c.px(51, 32, EYE, 0)
    if old:
        sc = ramp(350, 0.5, 0.85)
        for x, y in ((49, 30), (50, 31), (51, 32), (52, 33)):
            c.px(x, y, sc, 3)
        c.px(51, 33, ramp(200, 0.1, 0.95), 4)
    c.outline()
    return c


def boar():
    c = Canvas(64, 64)
    fur = ramp(18, 0.52, 0.52)
    dark = ramp(20, 0.40, 0.30)
    snout_r = ramp(10, 0.38, 0.80)
    c.shadow(31, 58, 19, 5)
    cx, cy, rx, ry = 28, 41, 17, 11
    quadruped_legs(c, fur, dark, (24, 35), (17, 38), 50, 57, dark)
    xs = np.arange(14, 40, 3)
    for x in xs:
        ytop = cy - ry * np.sqrt(max(0, 1 - ((x + 1.5 - cx) / rx) ** 2))
        c.shade(poly(c, [(x, ytop + 2), (x + 1.2, ytop - 4), (x + 3, ytop + 2)]), dark, None, flat=2, sep=False)
    c.shade(ell(c, 12, 38, 3.2, 3.2) & ~rect(c, 0, 0, 0, 0), dark, ellipsoid(12, 38, 3.2, 3.2), sep=True)
    body = ell(c, cx, cy, rx, ry)
    c.shade(body, fur, ellipsoid(cx, cy - 3, rx, ry + 3), dither=0.45, jitter=0.4, seed=5)
    c.adjust(body & (c.ys > cy + 5), -1)
    head = ell(c, 46, 42, 9, 8.2)
    c.shade(head, fur, ellipsoid(46, 39, 10, 10), dither=0.4, jitter=0.3)
    c.shade(poly(c, [(43, 35), (44, 29), (50, 36)]), dark, None, flat=2)
    snout = ell(c, 54.5, 45, 5.2, 4.4)
    c.shade(snout, snout_r, ellipsoid(54, 43, 6, 6), dither=0.3)
    c.shade(ell(c, 58.3, 45, 2.0, 2.6), ramp(350, 0.30, 0.38), None, flat=3, sep=False)
    c.px(58, 44, ramp(350, 0.3, 0.38), 1)
    c.px(58, 46, ramp(350, 0.3, 0.38), 1)
    tusk = ramp(48, 0.2, 1.0)
    c.shade(poly(c, [(52, 48), (53, 43), (56, 41), (55, 44), (54, 48)]), tusk, None, flat=4, sep=True)
    c.px(55, 41, tusk, 5)
    c.px(49, 40, EYE, 0)
    c.px(50, 40, ramp(48, 0.9, 1.0), 5)
    c.outline()
    return c


def iron_vein():
    c = Canvas(64, 64)
    rock = ramp(235, 0.14, 0.72)
    c.shadow(32, 57, 20, 5)
    for cx, cy, rx, ry, seed in ((19, 51, 8, 6, 1), (45, 52, 9.5, 7, 2), (32, 43, 15, 13, 3)):
        m = ell(c, cx, cy, rx, ry)
        c.shade(m, rock, ellipsoid(cx - 2, cy - 4, rx * 1.1, ry * 1.2), dither=0.35, jitter=0.4, seed=seed)
    ore = poly(c, [(26, 33), (30, 31), (36, 40), (34, 46), (30, 44)])
    c.shade(ore & ell(c, 32, 43, 15, 13), RUST, slab(), dither=0.3, jitter=0.4, seed=9, sep=False) if False else None
    rng_pts = [((24, 35), (30, 41)), ((34, 33), (38, 44)), ((26, 48), (33, 50)), ((40, 40), (45, 47)), ((17, 49), (21, 54)), ((44, 49), (50, 54))]
    for a, b in rng_pts:
        c.shade(line(c, a, b, 2) & (c.alpha == 255), RUST, None, flat=3, sep=False)
        c.shade(line(c, (a[0] - 1, a[1] - 1), (b[0] - 1, b[1] - 1), 1) & (c.alpha == 255), RUST, None, flat=5, sep=False)
    for x, y in ((29, 38), (37, 40), (31, 49), (43, 44), (20, 52), (47, 52)):
        c.px(x, y, ramp(40, 0.2, 1.0), 5)
    c.outline()
    return c


def slab():
    def f(x, y):
        return np.full_like(x, -0.3), np.full_like(x, -0.3), np.ones_like(x)
    return f


def leather_armor_layer():
    c = Canvas(64, 64)
    torso = poly(c, [(24, 27), (40, 27), (41, 34), (39, 45), (25, 45), (23, 34)])
    vest = torso & ~poly(c, [(29, 26), (35, 26), (32, 33)])
    c.shade(vest, LEATHER, cyl(32, 9.5), dither=0.4, jitter=0.25, seed=7)
    for x in (22.5, 41.5):
        c.shade(ell(c, x, 29.5, 3.8, 3.2), LEATHER, ellipsoid(x - 1, 28, 4.2, 4), dither=0.3)
        c.shade(ell(c, x, 29.5, 2.2, 1.6), LEATHER_DK, None, flat=3, sep=False)
    c.adjust(line(c, (32, 33), (32, 45), 1) & vest, -2)
    for y in (36, 39, 42):
        c.px(30, y, CREAM, 4)
        c.px(34, y, CREAM, 4)
    c.shade(rect(c, 24, 42, 40, 45) & vest, LEATHER_DK, cyl(32, 9.5), sep=False)
    c.shade(rect(c, 30, 42, 34, 45), GOLD, cyl(32, 2.5), sep=False)
    c.outline()
    return c


def rusty_sword_layer():
    c = Canvas(64, 64)
    blade(c, (43.5, 41), (50, 22), 4, STEEL)
    m = c.alpha == 255
    h = hash2(c.xs, c.ys, 17)
    c.tone[m & (h > 0.70)] = 2
    c.rid[m & (h > 0.70)] = c._rid(RUST)
    c.tone[m & (h > 0.90)] = 4
    c.shade(line(c, (40.5, 41), (46.5, 43), 2), ramp(24, 0.35, 0.50), None, flat=3, sep=False)
    c.shade(rect(c, 42, 42, 44, 48), LEATHER_DK, None, flat=3, sep=False)
    c.px(43, 48, GOLD, 4)
    c.outline()
    return c


def hunting_knife_layer():
    c = Canvas(64, 64)
    blade(c, (43.5, 43), (48, 34), 3, STEEL)
    c.shade(line(c, (41.5, 43.5), (45.5, 44), 2), GOLD, None, flat=3, sep=False)
    c.shade(rect(c, 42, 44, 44, 48), WOOD, None, flat=3, sep=False)
    c.outline()
    return c


JOBS = {
    "sprites/creatures/guide.png": guide,
    "sprites/creatures/herbalist.png": herbalist,
    "sprites/creatures/bandit.png": bandit,
    "sprites/creatures/wolf.png": lambda: wolf(False),
    "sprites/creatures/old_wolf.png": lambda: wolf(True),
    "sprites/creatures/boar.png": boar,
    "sprites/creatures/iron_vein.png": iron_vein,
    "sprites/equipment/leather_armor.png": leather_armor_layer,
    "sprites/equipment/rusty_sword.png": rusty_sword_layer,
    "sprites/equipment/hunting_knife.png": hunting_knife_layer,
}

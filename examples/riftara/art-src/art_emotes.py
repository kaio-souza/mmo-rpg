"""Emote pictures (32x32): the bubble over the character and the emote list.
A round yellow face per gesture, like the emote icons of other games."""
from artlib import Canvas, ramp, ell, rect, poly, line, ellipsoid
from art_chars import SKIN_TAN, EYE

FACE = ramp(50, 0.80, 1.0)
MOUTH = ramp(355, 0.65, 0.55)
TEAR = ramp(200, 0.55, 1.0)
SPARK = ramp(52, 0.45, 1.0)
BLUSH = ramp(350, 0.45, 0.95)
MOTION = ramp(210, 0.10, 0.95)


def face(c, cx=16, cy=17, r=11):
    c.shade(ell(c, cx, cy, r, r), FACE, ellipsoid(cx - 1, cy - 2, r, r), dither=0.3, lift=0.8)


def eye_arc(c, x, y, up=True):
    """A closed, happy eye (^) or a calm closed eye (u)."""
    d = -1 if up else 1
    c.px(x - 1, y, EYE, 0)
    c.px(x, y + d, EYE, 0)
    c.px(x + 1, y, EYE, 0)


def wave():
    c = Canvas(32, 32)
    # Open hand, palm out, raised.
    c.shade(ell(c, 16, 19, 6.5, 7), SKIN_TAN, ellipsoid(15, 17, 7, 7), dither=0.3)
    for fx, top in ((11, 7), (14, 5), (17, 5), (20, 7)):
        c.shade(rect(c, fx, top, fx + 2, 15), SKIN_TAN, None, flat=4)
    c.shade(poly(c, [(21, 18), (26, 13), (27, 15), (23, 21)]), SKIN_TAN, None, flat=3)
    c.shade(rect(c, 13, 25, 19, 29), SKIN_TAN, None, flat=2)
    # Motion arcs on both sides.
    for x0, x1 in ((5, 7), (25, 27)):
        c.shade(line(c, (x0, 8), (x1 if x0 < 16 else x0, 12), 1), MOTION, None, flat=5, sep=False)
    for pts in (((6, 14), (5, 18)), ((26, 14), (27, 18)), ((7, 6), (5, 10)), ((25, 6), (27, 10))):
        c.shade(line(c, pts[0], pts[1], 1), MOTION, None, flat=5, sep=False)
    c.outline()
    return c


def bow():
    c = Canvas(32, 32)
    face(c, 16, 18)
    eye_arc(c, 12, 17, up=False)
    eye_arc(c, 20, 17, up=False)
    c.shade(rect(c, 14, 23, 18, 24), MOUTH, None, flat=2, sep=False)
    # A bowing motion over the head.
    c.shade(line(c, (8, 4), (16, 2), 1), MOTION, None, flat=5, sep=False)
    c.shade(line(c, (16, 2), (24, 4), 1), MOTION, None, flat=5, sep=False)
    c.shade(poly(c, [(22, 6), (26, 4), (24, 8)]), MOTION, None, flat=5, sep=False)
    c.outline()
    return c


def cheer():
    c = Canvas(32, 32)
    face(c, 16, 19, 10)
    eye_arc(c, 12, 16, up=True)
    eye_arc(c, 20, 16, up=True)
    c.shade(ell(c, 16, 23, 4.5, 3) & (c.ys >= 22), MOUTH, None, flat=2)
    c.px(10, 20, BLUSH, 3)
    c.px(22, 20, BLUSH, 3)
    # Sparkles around.
    for sx, sy in ((5, 6), (26, 5), (4, 22), (28, 20)):
        for dx, dy in ((0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)):
            c.px(sx + dx, sy + dy, SPARK, 5 if (dx, dy) == (0, 0) else 4)
    c.outline()
    return c


def laugh():
    c = Canvas(32, 32)
    face(c, 16, 17)
    # Squeezed eyes: > <
    for x, d in ((12, 1), (20, -1)):
        c.px(x - d, 13, EYE, 0)
        c.px(x, 14, EYE, 0)
        c.px(x - d, 15, EYE, 0)
    c.shade(ell(c, 16, 21, 6, 4.5) & (c.ys >= 20), MOUTH, None, flat=2)
    c.shade(rect(c, 12, 20, 20, 20), ramp(0, 0.0, 1.0), None, flat=5, sep=False)
    # Tears of laughter.
    for tx in (8, 24):
        c.shade(ell(c, tx, 17, 1.6, 2.4), TEAR, None, flat=4)
    c.outline()
    return c


JOBS = {
    "icons/emote_wave.png": wave,
    "icons/emote_bow.png": bow,
    "icons/emote_cheer.png": cheer,
    "icons/emote_laugh.png": laugh,
}

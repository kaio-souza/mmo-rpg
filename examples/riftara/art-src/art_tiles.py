"""Ground tiles (64x32) and the stone wall block (64x72). Textures are periodic in tile space so tiles join."""
import numpy as np
from PIL import Image
from artlib import Canvas, ramp, pnoise, hash2, BAYER, poly, line, rect

GRASS = ramp(108, 0.62, 0.80)
GRASS_DK = ramp(150, 0.55, 0.62)
DIRT = ramp(28, 0.50, 0.74)
STONE = ramp(228, 0.14, 0.80)
STONE_WARM = ramp(38, 0.16, 0.82)
WATER = ramp(204, 0.68, 0.86)
MOSS = ramp(100, 0.55, 0.60)
FLOWER_W = ramp(50, 0.10, 1.0)
FLOWER_Y = ramp(48, 0.85, 1.0)
FLOWER_P = ramp(330, 0.55, 0.95)
PEBBLE = ramp(30, 0.18, 0.78)


def uv(c):
    dx = c.xs + 0.5 - 32
    dy = c.ys + 0.5 - 16
    inside = np.abs(dx) / 32 + np.abs(dy) / 16 <= 1.0
    u = (dx / 32 + dy / 16) / 2 + 0.5
    v = (dy / 16 - dx / 32) / 2 + 0.5
    return inside, u % 1.0, v % 1.0


def to_px(u, v):
    return 32 + (u - v) * 32, 16 + (u + v - 1) * 16


def paint_ground(c, inside, ramp_, field, lo=1.0, hi=5.0, dither=0.55):
    ys, xs = np.nonzero(inside)
    val = lo + (hi - lo) * np.clip(field[ys, xs], 0, 1) + (BAYER[ys % 4, xs % 4] - 0.5) * dither
    idx = np.clip(np.floor(val + 0.5), 1, 5).astype(int)
    rid = c._rid(ramp_)
    c.rid[ys, xs] = rid
    c.tone[ys, xs] = idx
    c.alpha[ys, xs] = 255


def scatter(c, inside, n, seed):
    """Wrapped random spots so neighbouring tiles continue each other's clipped details."""
    pts = []
    for i in range(n):
        u, v = hash2(i, 1, seed), hash2(i, 2, seed)
        for du in (-1, 0, 1):
            for dv in (-1, 0, 1):
                x, y = to_px(u + du, v + dv)
                xi, yi = int(x), int(y)
                if 0 <= xi < 64 and 0 <= yi < 32 and inside[yi, xi]:
                    pts.append((xi, yi, i))
    return pts


def grass():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    f = 0.5 * pnoise(u, v, 3, 11) + 0.3 * pnoise(u, v, 6, 12) + 0.2 * pnoise(u, v, 12, 13)
    paint_ground(c, inside, GRASS, 0.2 + 0.55 * f, lo=2.0, hi=3.5, dither=0.35)
    dark = pnoise(u, v, 3, 21) > 0.66
    c.rid[inside & dark] = c._rid(GRASS_DK)
    for x, y, i in scatter(c, inside, 12, 31):
        c.px(x, y, GRASS, 2)
        c.px(x, y - 1, GRASS, 4)
        if hash2(i, 5, 31) > 0.6:
            c.px(x, y - 2, GRASS, 4)
    for x, y, i in scatter(c, inside, 2, 41):
        k = hash2(i, 6, 41)
        r = FLOWER_W if k < 0.4 else FLOWER_Y if k < 0.75 else FLOWER_P
        c.px(x, y, r, 4)
        c.px(x + 1, y, r, 5)
        c.px(x, y - 1, GRASS, 2)
    return c


def dirt_path():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    f = 0.45 * pnoise(u, v, 3, 51) + 0.35 * pnoise(u, v, 7, 52) + 0.2 * pnoise(u, v, 14, 53)
    paint_ground(c, inside, DIRT, 0.15 + 0.6 * f, lo=1.6, hi=3.8, dither=0.4)
    for x, y, i in scatter(c, inside, 6, 61):
        c.px(x, y, PEBBLE, 4)
        c.px(x + 1, y, PEBBLE, 3)
        c.px(x, y + 1, PEBBLE, 2)
        c.px(x + 1, y + 1, PEBBLE, 1)
    return c


def stone_floor():
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    n = 2
    fu, fv = (u * n) % 1.0, (v * n) % 1.0
    cu, cv = np.floor(u * n).astype(int), np.floor(v * n).astype(int)
    tone = 2.3 + 1.6 * hash2(cu, cv, 81) + 0.8 * (pnoise(u, v, 8, 82) - 0.5)
    w = 0.075
    mortar = (fu < w) | (fv < w)
    hi = ((fu >= w) & (fu < w + 0.10)) | ((fv >= w) & (fv < w + 0.10))
    lo = (fu > 1 - 0.10) | (fv > 1 - 0.10)
    field = (tone - 1.0) / 4.0
    paint_ground(c, inside, STONE, field, lo=1.0, hi=5.0, dither=0.5)
    c.tone[inside & hi & ~mortar] = np.minimum(c.tone[inside & hi & ~mortar] + 1, 5)
    c.tone[inside & lo & ~mortar & ~hi] = np.maximum(c.tone[inside & lo & ~mortar & ~hi] - 1, 1)
    c.tone[inside & mortar] = 1
    c.rid[inside & mortar] = c._rid(STONE)
    for x, y, i in scatter(c, inside, 4, 91):
        c.px(x, y, STONE, 1)
        c.px(x + 1, y + 1, STONE, 1)
    for x, y, i in scatter(c, inside, 3, 95):
        c.px(x, y, MOSS, 3)
        c.px(x + 1, y, MOSS, 2)
    return c


def water(k=0):
    """Frame k of 4: the waves move a quarter turn each frame and loop."""
    c = Canvas(64, 32)
    inside, u, v = uv(c)
    n = pnoise(u, v, 4, 101)
    ph = np.pi * k / 2
    wave = 0.5 + 0.5 * np.sin(2 * np.pi * (2 * u + 1 * v) + 3.0 * n + ph)
    wave2 = 0.5 + 0.5 * np.sin(2 * np.pi * (1 * u - 2 * v) + 4.0 * pnoise(u, v, 5, 102) - ph)
    base = 0.40 + 0.22 * n + 0.22 * wave
    paint_ground(c, inside, WATER, base, lo=1.0, hi=3.6, dither=0.45)
    glint = inside & (wave * wave2 > 0.80)
    c.tone[glint] = 5
    foam = inside & (wave * wave2 > 0.90)
    c.tone[foam] = 5
    c.rid[foam] = c._rid(ramp(190, 0.25, 1.0))
    return c


def stone_wall():
    c = Canvas(64, 72)
    X, Y = c.xs + 0.5, c.ys + 0.5
    left = poly(c, [(0, 16), (32, 32), (32, 72), (0, 56)])
    right = poly(c, [(32, 32), (64, 16), (64, 56), (32, 72)])
    top = poly(c, [(32, 0), (64, 16), (32, 32), (0, 16)])
    rid_l, rid_r, rid_t = c._rid(STONE), c._rid(STONE), c._rid(STONE_WARM)

    def faces(mask, top_y, x_local, base_tone, seed):
        ys, xs = np.nonzero(mask)
        rel = ys - top_y(xs)
        course = np.floor(rel / 8).astype(int)
        xl = x_local(xs) + 8 * (course % 2)
        brick = np.floor(xl / 16).astype(int)
        t = base_tone + 1.0 * (hash2(brick, course, seed) - 0.5) + 0.25 * (hash2(xs // 3, ys // 3, seed + 3) - 0.5)
        t = t + (BAYER[ys % 4, xs % 4] - 0.5) * 0.2
        idx = np.clip(np.floor(t + 0.5), 1, 5).astype(int)
        mort_h = (rel % 8) == 0
        mort_v = (xl % 16) == 0
        idx = np.where(mort_h | mort_v, 1, idx)
        bevel = ((rel % 8) == 1) & ~mort_v
        idx = np.where(bevel, np.minimum(idx + 1, 5), idx)
        shade_row = ((rel % 8) == 7) & ~mort_v
        idx = np.where(shade_row, np.maximum(idx - 1, 1), idx)
        low = rel > 33
        moss = low & (hash2(xs // 2, ys // 2, seed + 7) * (rel / 40.0) > 0.78)
        return ys, xs, idx, moss

    for mask, top_y, x_local, base, seed, rid in [
        (left, lambda xs: 16 + xs / 2.0, lambda xs: xs, 3.4, 111, rid_l),
        (right, lambda xs: 32 - (xs - 32) / 2.0, lambda xs: xs - 32, 2.2, 112, rid_r),
    ]:
        ys, xs, idx, moss = faces(mask, top_y, x_local, base, seed)
        c.rid[ys, xs] = rid
        c.tone[ys, xs] = idx
        c.alpha[ys, xs] = 255
        c.rid[ys[moss], xs[moss]] = c._rid(MOSS)
        c.tone[ys[moss], xs[moss]] = np.clip(idx[moss] - 1, 1, 4)

    ys, xs = np.nonzero(top)
    u = ((xs + 0.5 - 32) / 32 + (ys + 0.5 - 16) / 16) / 2 + 0.5
    v = ((ys + 0.5 - 16) / 16 - (xs + 0.5 - 32) / 32) / 2 + 0.5
    cu, cv = np.floor(u * 2).astype(int), np.floor(v * 2).astype(int)
    fu, fv = (u * 2) % 1, (v * 2) % 1
    t = 4.2 + 0.9 * (hash2(cu, cv, 121) - 0.5) + (BAYER[ys % 4, xs % 4] - 0.5) * 0.4
    idx = np.clip(np.floor(t + 0.5), 1, 5).astype(int)
    seam = (fu < 0.06) | (fv < 0.06)
    idx = np.where(seam, 2, idx)
    edge = (u > 0.93) | (v > 0.93)
    idx = np.where(edge & ~seam, 5, idx)
    c.rid[ys, xs] = rid_t
    c.tone[ys, xs] = idx
    c.alpha[ys, xs] = 255
    ridge = rect(c, 32, 33, 33, 72) & (c.alpha == 255)
    c.tone[ridge] = np.minimum(c.tone[ridge] + 2, 5)
    lip = line(c, (0, 16), (32, 32), 1) | line(c, (32, 32), (64, 16), 1)
    lipm = lip & top
    c.tone[lipm] = 1
    c.outline()
    return c


JOBS = {
    "sprites/tiles/grass.png": grass,
    "sprites/tiles/dirt_path.png": dirt_path,
    "sprites/tiles/stone_floor.png": stone_floor,
    "sprites/walls/stone_wall.png": stone_wall,
}

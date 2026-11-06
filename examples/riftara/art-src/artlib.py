"""Shared toolkit for the Riftara art generator: one palette method, one light, one outline rule.

Light comes from the top-left. Every shape is
shaded into a 5-tone ramp (+1 outline tone) with ordered dithering, then outlined 1 px.
"""
import colorsys
import numpy as np
from PIL import Image, ImageDraw

LIGHT = np.array([-0.50, -0.62, 0.60])
LIGHT = LIGHT / np.linalg.norm(LIGHT)
BAYER = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]) / 16.0
_VS = [0.20, 0.34, 0.52, 0.72, 0.90, 1.0]
_SS = [0.85, 1.0, 1.0, 0.88, 0.62, 0.38]


def _shift(h, target, w):
    d = ((target - h + 540) % 360) - 180
    return (h + d * w) % 360


def ramp(h, s, v=1.0):
    """6 colors, index 0 darkest (outline) to 5 lightest; shadows go cool, lights go warm."""
    out = []
    for i in range(6):
        t = i / 5
        hue = _shift(_shift(h, 255, (1 - t) * 0.22), 55, t * 0.18)
        r, g, b = colorsys.hsv_to_rgb(hue / 360, min(1, _SS[i] * s), min(1, _VS[i] * v))
        out.append((round(r * 255), round(g * 255), round(b * 255)))
    return tuple(out)


def hash2(x, y, seed=0):
    n = (np.asarray(x, dtype=np.int64) * 374761393 + np.asarray(y, dtype=np.int64) * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    n = n ^ (n >> 16)
    return (n & 0xFFFF) / 65535.0


def pnoise(u, v, n, seed):
    """Value noise periodic in u and v with period 1 (n lattice cells): tiles join seamlessly."""
    x, y = u * n, v * n
    x0, y0 = np.floor(x).astype(int), np.floor(y).astype(int)
    fx, fy = x - x0, y - y0
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    g = lambda i, j: hash2(i % n, j % n, seed)
    return (g(x0, y0) * (1 - fx) + g(x0 + 1, y0) * fx) * (1 - fy) + (g(x0, y0 + 1) * (1 - fx) + g(x0 + 1, y0 + 1) * fx) * fy


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.rid = -np.ones((h, w), int)
        self.tone = np.zeros((h, w), int)
        self.alpha = np.zeros((h, w), np.uint8)
        self.ramps = []
        self.ys, self.xs = np.mgrid[0:h, 0:w]

    def _rid(self, r):
        if r not in self.ramps:
            self.ramps.append(r)
        return self.ramps.index(r)

    def shadow(self, cx, cy, rx, ry, a=90):
        m = ell(self, cx, cy, rx, ry) & (self.alpha == 0)
        self.rid[m] = self._rid(ramp(250, 0.4, 0.5))
        self.tone[m] = 0
        self.alpha[m] = a

    def shade(self, mask, r, normal=None, flat=3, bias=0.0, dither=0.0, jitter=0.0, seed=1, sep=True, lift=0.0):
        if not mask.any():
            return
        prev = self.alpha == 255
        ys, xs = np.nonzero(mask)
        if normal is None:
            val = np.full(xs.shape, float(flat))
        else:
            nx, ny, nz = normal(xs + 0.5, ys + 0.5)
            ln = np.sqrt(nx * nx + ny * ny + nz * nz) + 1e-9
            i = (nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2]) / ln
            val = np.clip(0.12 + 0.9 * i + bias, 0, 1) * 4 + 1
        val = val + lift
        if dither:
            val = val + (BAYER[ys % 4, xs % 4] - 0.5) * dither
        if jitter:
            val = val + (hash2(xs, ys, seed) - 0.5) * jitter
        idxmap = np.zeros((self.h, self.w), int)
        idxmap[ys, xs] = np.clip(np.floor(val + 0.5), 1, 5).astype(int)
        rid = self._rid(r)
        inner = mask
        if sep:
            er = mask.copy()
            er[1:-1, 1:-1] = mask[1:-1, 1:-1] & mask[:-2, 1:-1] & mask[2:, 1:-1] & mask[1:-1, :-2] & mask[1:-1, 2:]
            edge = mask & ~er & prev
            self.rid[edge] = rid
            self.tone[edge] = 0
            self.alpha[edge] = 255
            inner = mask & ~edge
        self.rid[inner] = rid
        self.tone[inner] = idxmap[inner]
        self.alpha[inner] = 255

    def flat(self, mask, r, idx=3, sep=False):
        self.shade(mask, r, None, flat=idx, sep=sep)

    def adjust(self, mask, delta):
        m = mask & (self.alpha == 255) & (self.tone > 0)
        self.tone[m] = np.clip(self.tone[m] + delta, 1, 5)

    def px(self, x, y, r, idx=3):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.rid[y, x] = self._rid(r)
            self.tone[y, x] = idx
            self.alpha[y, x] = 255

    def outline(self):
        opaque = self.alpha == 255
        h, w = self.h, self.w
        new_rid = self.rid.copy()
        new_tone = self.tone.copy()
        new_alpha = self.alpha.copy()
        dark = np.zeros((h, w), bool)
        best = {}
        for dy, dx in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            sy = np.roll(np.roll(opaque, -dy, 0), -dx, 1)
            if dy == 1:
                sy[-1, :] = False
            if dy == -1:
                sy[0, :] = False
            if dx == 1:
                sy[:, -1] = False
            if dx == -1:
                sy[:, 0] = False
            cand = sy & ~opaque
            nr = np.roll(np.roll(self.rid, -dy, 0), -dx, 1)
            light_side = (dy == 1) or (dx == 1)
            for y, x in zip(*np.nonzero(cand)):
                key = (y, x)
                t = 1 if light_side else 0
                if key not in best or t < best[key][1]:
                    best[key] = (nr[y, x], t)
        for (y, x), (r, t) in best.items():
            new_rid[y, x] = r
            new_tone[y, x] = t
            new_alpha[y, x] = 255
        self.rid, self.tone, self.alpha = new_rid, new_tone, new_alpha

    def image(self):
        out = np.zeros((self.h, self.w, 4), np.uint8)
        for k, r in enumerate(self.ramps):
            m = self.rid == k
            tab = np.array(r, np.uint8)
            out[m, :3] = tab[self.tone[m]]
        out[..., 3] = self.alpha
        out[self.alpha == 0, :3] = 0
        return Image.fromarray(out, "RGBA")


def ell(c, cx, cy, rx, ry):
    return (((c.xs + 0.5 - cx) / rx) ** 2 + ((c.ys + 0.5 - cy) / ry) ** 2) <= 1


def rect(c, x0, y0, x1, y1):
    return (c.xs >= x0) & (c.xs < x1) & (c.ys >= y0) & (c.ys < y1)


def _pil(c, fn):
    im = Image.new("L", (c.w, c.h), 0)
    fn(ImageDraw.Draw(im))
    return np.array(im) > 0


def poly(c, pts):
    return _pil(c, lambda d: d.polygon([(float(x), float(y)) for x, y in pts], fill=1, outline=1))


def line(c, p0, p1, w=1):
    return _pil(c, lambda d: d.line([p0, p1], fill=1, width=w))


def ellipsoid(cx, cy, rx, ry):
    def f(x, y):
        nx, ny = (x - cx) / rx, (y - cy) / ry
        r2 = np.minimum(nx * nx + ny * ny, 1)
        return nx, ny, np.sqrt(1 - r2)
    return f


def cyl(xc, hw, tilt=-0.2):
    def f(x, y):
        nx = np.clip((x - xc) / hw, -1, 1)
        return nx, np.full_like(nx, tilt), np.sqrt(1 - nx * nx)
    return f


def slab(top=-0.35):
    """Mostly facing up-left: for flat things like blades and leaves."""
    def f(x, y):
        z = np.full_like(x, 1.0)
        return np.full_like(x, top), np.full_like(x, top), z
    return f


def blade(c, p0, p1, w, r, tip=True):
    a, b = np.array(p0, float), np.array(p1, float)
    d = b - a
    d /= np.linalg.norm(d)
    n = np.array([-d[1], d[0]])
    if n[0] + n[1] > 0:
        n = -n
    b2 = b - d * w * 1.3 if tip else b
    full_pts = [a + n * w / 2, b2 + n * w / 2, b, b2 - n * w / 2, a - n * w / 2] if tip else [a + n * w / 2, b + n * w / 2, b - n * w / 2, a - n * w / 2]
    full = poly(c, full_pts)
    light = poly(c, [a, a + n * w / 2, b2 + n * w / 2, b])
    c.shade(full, r, None, flat=2, sep=False)
    c.shade(light & full, r, None, flat=4, sep=False)
    c.shade(line(c, tuple(a), tuple(b), 1) & full, r, None, flat=5, sep=False)

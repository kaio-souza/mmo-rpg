"""Phase 11 strips: frames side by side, one row per direction (S, W, N, E),
as assets/sprites/sprites.ron says.

Every still image of the other modules is wrapped here into a strip:
- humanoids are drawn again in front, side and back views with a step;
- four-legged beasts are drawn in profile, so S and W look left, N and E
  right, with a stride and a bob;
- beasts drawn from the front (spiders, frog, slime, golem, bat, wisp) keep
  one direction and get a motion of their own;
- ore veins glint, plants and trees sway, fire flickers, water flows;
- equipment layers follow the body's direction.
Provisional art: the real art replaces these files by the same names.
"""
import numpy as np
from PIL import Image, ImageOps

import art_alpha
import art_chars
import art_phase10
import art_tiles

WALK = (0, 2, 0, -2)
FRAMES = 4


class Strip:
    """Rows of frames (each a Canvas or an image) packed into one image."""

    def __init__(self, rows):
        self.rows = rows

    def image(self):
        frames = [[f.image() if hasattr(f, "image") else f for f in row] for row in self.rows]
        w, h = frames[0][0].size
        out = Image.new("RGBA", (w * len(frames[0]), h * len(frames)))
        for y, row in enumerate(frames):
            for x, im in enumerate(row):
                out.paste(im, (x * w, y * h))
        return out


def still(fn):
    im = fn()
    return im.image() if hasattr(im, "image") else im


# ---------- pixel motions on a finished image ----------

def arr(im):
    return np.array(im.convert("RGBA"))


def img(a):
    return Image.fromarray(a.astype(np.uint8), "RGBA")


def body_mask(a):
    """What moves: everything but the ground shadow (drawn at alpha 90)."""
    return (a[..., 3] > 0) & (a[..., 3] != 90)


def move(a, mask, dx, dy):
    """Moves the masked pixels by (dx, dy) over the rest."""
    out = a.copy()
    out[mask] = 0
    ys, xs = np.nonzero(mask)
    ny, nx = ys + dy, xs + dx
    ok = (ny >= 0) & (ny < a.shape[0]) & (nx >= 0) & (nx < a.shape[1])
    out[ny[ok], nx[ok]] = a[ys[ok], xs[ok]]
    return out


def band(a, frac_from, frac_to):
    """Body pixels in a horizontal band of the body's height (0 top, 1 feet)."""
    m = body_mask(a)
    ys = np.nonzero(m)[0]
    if not len(ys):
        return m
    top, bot = ys.min(), ys.max()
    y0, y1 = top + (bot - top) * frac_from, top + (bot - top) * frac_to
    rows = np.arange(a.shape[0])[:, None]
    return m & (rows >= y0) & (rows <= y1)


def walk_frames(im, stride=1, bob=1):
    """Legs shuffle (the lowest quarter slides) and the body bobs."""
    a = arr(im)
    out = []
    for k in range(FRAMES):
        f = a
        s = (0, stride, 0, -stride)[k]
        if s:
            legs = band(f, 0.78, 1.0)
            f = move(f, legs, s, 0)
        b = (0, -bob, 0, -bob)[k]
        if b:
            f = move(f, band(f, 0.0, 0.78), 0, b)
        out.append(img(f))
    return out


def scuttle_frames(im):
    """Spiders: the legs of each side lift in turn."""
    a = arr(im)
    m = body_mask(a)
    cols = np.arange(a.shape[1])[None, :]
    left, right = m & (cols < 24), m & (cols > 40)
    out = []
    for k in range(FRAMES):
        f = a
        d = (0, -1, 0, 1)[k]
        if d:
            f = move(f, left, 0, d)
            f = move(f, right & body_mask(f), 0, -d)
        out.append(img(f))
    return out


def hop_frames(im, heights=(0, -2, -4, -1)):
    a = arr(im)
    return [img(move(a, body_mask(a), 0, h)) if h else img(a) for h in heights]


def squash_frames(im, scales=(1.0, 0.9, 1.0, 1.08)):
    """Slime: squashes and stretches on its base (wider when lower)."""
    a = arr(im)
    m = body_mask(a)
    body = a.copy()
    body[~m] = 0
    rest = a.copy()
    rest[m] = 0
    ys, xs = np.nonzero(m)
    x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
    part = img(body).crop((x0, y0, x1, y1))
    out = []
    for s in scales:
        w, h = max(1, round((x1 - x0) / s)), max(1, round((y1 - y0) * s))
        r = part.resize((w, h), Image.NEAREST)
        f = img(rest)
        f.alpha_composite(r, (round((x0 + x1) / 2 - w / 2), y1 - h))
        out.append(f)
    return out


def flap_frames(im, cx=32, core=7):
    """Bat: the wings lift and drop more the farther from the body."""
    a = arr(im)
    m = body_mask(a)
    out = []
    for k in range(FRAMES):
        lift = (0, -3, 0, 3)[k]
        f = a.copy()
        if lift:
            ys, xs = np.nonzero(m)
            far = np.abs(xs - cx) > core
            f[ys[far], xs[far]] = 0
            dy = np.round(lift * (np.abs(xs[far] - cx) - core) / 12).astype(int)
            ny = np.clip(ys[far] + dy, 0, a.shape[0] - 1)
            f[ny, xs[far]] = a[ys[far], xs[far]]
        out.append(img(f))
    return out


def sway_frames(im, amp=1, top=0.0, bottom=0.7):
    """Plants and trees: the upper part leans one way, then the other."""
    a = arr(im)
    out = []
    for k in range(FRAMES):
        s = (0, amp, 0, -amp)[k]
        f = a
        if s:
            f = move(f, band(f, top, bottom * 0.5), s, 0)
            f = move(f, band(f, bottom * 0.5, bottom), int(np.sign(s)) if abs(s) > 1 else 0, 0)
        out.append(img(f))
    return out


def glint_frames(im, period=24):
    """Ore veins: a band of light runs across the bright pixels."""
    a = arr(im)
    m = body_mask(a)
    ys, xs = np.mgrid[0:a.shape[0], 0:a.shape[1]]
    lum = a[..., :3].mean(axis=2)
    out = []
    for k in range(FRAMES):
        f = a.copy().astype(int)
        phase = (xs + ys - k * period // FRAMES) % period
        hit = m & (phase < 3) & (lum > 90)
        f[hit, :3] = np.minimum(255, f[hit, :3] + 70)
        out.append(img(f))
    return out


def bob_frames(im, heights=(0, -1, -2, -1)):
    return hop_frames(im, heights)


def rows(frames, mirror_left=False):
    """4 rows S, W, N, E from one set of frames that faces right
    (`mirror_left`: S and W look left)."""
    left = [ImageOps.mirror(f) for f in frames] if mirror_left else frames
    return [left, left, frames, frames]


# ---------- humanoids ----------

def posed(fn, view, step):
    art_chars.POSE.update(view=view, step=step)
    try:
        return still(fn)
    finally:
        art_chars.POSE.update(view="front", step=0)


def is_humanoid(fn):
    return np.array_equal(arr(posed(fn, "front", 0)), arr(posed(fn, "back", 0))) is False


def humanoid_strip(fn):
    side = [posed(fn, "side", s) for s in WALK]
    return Strip([
        [posed(fn, "front", s) for s in WALK],
        [ImageOps.mirror(im) for im in side],
        # From behind, the right hand is on the left of the screen.
        [ImageOps.mirror(posed(fn, "back", s)) for s in WALK],
        side,
    ])


# ---------- equipment: one frame, four directions ----------

def weapon_strip(fn):
    """The weapon sits in the right hand: from the side the hand is at the
    middle of the body (13 px to the left), from behind on the left."""
    a = arr(still(fn))
    side = img(move(a, body_mask(a), -13, 0))
    return Strip([[img(a)], [ImageOps.mirror(side)], [ImageOps.mirror(img(a))], [side]])


def armor_strip(fn):
    """From the side the shoulder pads beside the body are hidden."""
    a = arr(still(fn))
    cols = np.arange(a.shape[1])[None, :]
    side = a.copy()
    side[(np.abs(cols - 32) > 8) & np.ones_like(side[..., 0], bool)] = 0
    return Strip([[img(a)], [img(side)], [img(a)], [img(side)]])


# ---------- what each sprite becomes ----------

QUADRUPEDS = {"wolf", "old_wolf", "dire_wolf", "boar", "field_rat"}
SPIDERS = {"briar_spider", "cave_spider"}
VEINS = {"iron_vein", "copper_vein", "silver_vein"}
PLANTS = {"herb_patch": 1, "bush": 1, "reeds": 1, "tree": 1, "pine": 1}


def wrap(path, fn):
    name = path.rsplit("/", 1)[-1][:-4]
    if not path.startswith("sprites/"):
        return None  # icons and portraits stay still
    if path.startswith("sprites/equipment/"):
        return lambda: (armor_strip if "armor" in name else weapon_strip)(fn)
    if name in QUADRUPEDS:
        return lambda: Strip(rows(walk_frames(still(fn)), mirror_left=True))
    if name in SPIDERS:
        return lambda: Strip([scuttle_frames(still(fn))])
    if name in VEINS:
        return lambda: Strip([glint_frames(still(fn))])
    if name in PLANTS:
        return lambda: Strip([sway_frames(still(fn), PLANTS[name])])
    if name == "bog_frog":
        return lambda: Strip([hop_frames(still(fn))])
    if name == "marsh_slime":
        return lambda: Strip([squash_frames(still(fn))])
    if name == "cave_bat":
        return lambda: Strip([flap_frames(still(fn))])
    if name == "fen_wisp":
        return lambda: Strip([bob_frames(still(fn))])
    if name == "rock_golem":
        return lambda: Strip([walk_frames(still(fn), stride=1, bob=1)])
    if path.startswith("sprites/creatures/") and is_humanoid(fn):
        return lambda: humanoid_strip(fn)
    return None


# ---------- the torch (phase 11) ----------

def torch_frame(k, x=21.0, y=46.0):
    """The torch held in the other hand at (x, y): stick, wrap, flame k."""
    from artlib import Canvas, ramp, ell, poly, line, cyl
    sway = (0, 1, -1, 1)[k % 4]
    lick = (0, -2, 1, -1)[k % 4]
    c = Canvas(64, 64)
    c.shade(line(c, (x, y), (x - 2, y - 13), 2), art_chars.WOOD, None, flat=3)
    c.shade(ell(c, x - 2, y - 14, 2.6, 2.2), art_chars.LEATHER_DK, None, flat=3, sep=False)
    tip = y - 15
    flame = poly(c, [(x - 5, tip), (x - 4 - sway, tip - 5 + lick), (x - 2 + sway, tip - 10 + lick), (x, tip - 5), (x + 1, tip)])
    c.shade(flame, ramp(30, 0.85, 1.0), cyl(x - 2, 3), dither=0.3, jitter=0.3, seed=7 + k)
    c.shade(poly(c, [(x - 3.5, tip), (x - 2 + sway / 2, tip - 6 + lick / 2), (x - 0.5, tip)]), ramp(48, 0.6, 1.0), None, flat=5, sep=False)
    c.outline()
    return c


def torch_layer():
    """4 flame frames x 4 rows: S in the left hand, W and E beside the
    body (the hand at its middle), N on the other side."""
    front = [torch_frame(k).image() for k in range(FRAMES)]
    side = [torch_frame(k, x=36.0).image() for k in range(FRAMES)]
    return Strip([front, [ImageOps.mirror(f) for f in side], [ImageOps.mirror(f) for f in front], side])


def torch_icon():
    from artlib import Canvas, ramp, ell, poly, line, cyl
    c = Canvas(32, 32)
    c.shade(line(c, (20, 29), (13, 13), 3), art_chars.WOOD, None, flat=3)
    c.shade(ell(c, 13, 12, 3.6, 3), art_chars.LEATHER_DK, None, flat=3, sep=False)
    c.shade(poly(c, [(8, 11), (9, 5), (12, 1), (15, 6), (18, 11)]), ramp(30, 0.85, 1.0), cyl(13, 5), dither=0.3)
    c.shade(poly(c, [(10, 11), (13, 4), (16, 11)]), ramp(48, 0.6, 1.0), None, flat=5, sep=False)
    c.outline()
    return c


def brazier():
    return Strip([[art_phase10.brazier(k) for k in range(4)]])


def water():
    return Strip([[art_tiles.water(k) for k in range(4)]])


def player():
    return humanoid_strip(art_chars.player)


JOBS = {
    "sprites/walls/brazier.png": brazier,
    "sprites/tiles/water.png": water,
    "sprites/creatures/player.png": player,
    "sprites/equipment/torch.png": torch_layer,
    "icons/torch.png": torch_icon,
}
for _mod in (art_tiles, art_chars, art_phase10, art_alpha):
    for _path, _fn in getattr(_mod, "JOBS", {}).items():
        _w = wrap(_path, _fn)
        if _w is not None:
            JOBS[_path] = _w

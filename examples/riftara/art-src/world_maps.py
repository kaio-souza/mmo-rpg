#!/usr/bin/env python3
"""Lays out the alpha world: writes
content/zones/<zone>/map.ron for the seven zones and checks that every gate,
NPC, spawn and node can be reached on foot from the summoning circle.

Deterministic (fixed seeds). The editor's map painter is the long-term home of
these maps; this script is how the first layout was drawn:
    python3 art-src/world_maps.py [--content content] [--check-only]
"""
import argparse
import os
import random
from collections import deque

# Characters are layout, not game text. Floors: assets/tiles.ron.
LEGEND = [
    ('.', 1, False, False),   # grass
    (',', 2, False, False),   # dirt path
    ('#', 3, True, True),     # stone wall
    ('~', 4, True, False),    # water
    ('_', 5, False, False),   # stone floor
    ('*', 6, True, False),    # brazier
    ('T', 7, True, True),     # tree
    ('=', 8, False, False),   # wood floor (boardwalk, bridge, house)
    ('f', 9, False, False),   # flowers
    (';', 10, False, False),  # mud
    (':', 11, False, False),  # cave floor
    ('o', 12, True, False),   # boulder
    ('b', 13, True, False),   # bush, bramble
    ('H', 14, True, True),    # timber wall
    ('r', 15, True, False),   # reeds
    ('%', 16, False, False),  # farmland
    ('P', 17, True, True),    # pine
    ('R', 18, True, True),    # rock wall
    ('i', 19, True, False),   # brazier, put out
]
# A player switches these by clicking: lit <-> put out.
SWITCH = {'*': 19, 'i': 6}
BLOCKS = {c for c, _, m, _ in LEGEND if m}


class Map:
    def __init__(self, name, origin, w, h, fill='.', seed=1):
        self.name, self.origin, self.w, self.h = name, origin, w, h
        self.g = [[fill] * w for _ in range(h)]
        self.rng = random.Random(seed)
        self.keep = []  # (x0, y0, x1, y1): no scatter here

    def inside(self, x, y):
        return 0 <= x < self.w and 0 <= y < self.h

    def set(self, x, y, ch):
        if self.inside(x, y):
            self.g[y][x] = ch

    def get(self, x, y):
        return self.g[y][x] if self.inside(x, y) else None

    def fill(self, x0, y0, x1, y1, ch):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.set(x, y, ch)

    def frame(self, x0, y0, x1, y1, ch):
        for x in range(x0, x1 + 1):
            self.set(x, y0, ch)
            self.set(x, y1, ch)
        for y in range(y0, y1 + 1):
            self.set(x0, y, ch)
            self.set(x1, y, ch)

    def border(self, ch, gaps=()):
        self.frame(0, 0, self.w - 1, self.h - 1, ch)
        for x0, y0, x1, y1 in gaps:
            self.fill(x0, y0, x1, y1, ',')

    def ell(self, cx, cy, rx, ry, ch, rough=0.0, on=None):
        for y in range(int(cy - ry - 2), int(cy + ry + 3)):
            for x in range(int(cx - rx - 2), int(cx + rx + 3)):
                d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
                if d <= 1.0 + rough * (self.rng.random() - 0.5) and (on is None or self.get(x, y) in on):
                    self.set(x, y, ch)

    def path(self, pts, width, ch, on=None):
        """Thick path through the points, straight legs (x first, then y)."""
        for (ax, ay), (bx, by) in zip(pts, pts[1:]):
            for x in range(min(ax, bx), max(ax, bx) + width):
                for k in range(width):
                    if on is None or self.get(x, ay + k) in on:
                        self.set(x, ay + k, ch)
            for y in range(min(ay, by), max(ay, by) + width):
                for k in range(width):
                    if on is None or self.get(bx + k, y) in on:
                        self.set(bx + k, y, ch)

    def wander(self, a, b, width, ch, wobble=3):
        """A winding tunnel or trail from a to b."""
        x, y = a
        while (x, y) != b:
            if self.rng.random() < 0.7:
                x += (b[0] > x) - (b[0] < x)
                y += (b[1] > y) - (b[1] < y)
            else:
                x += self.rng.randint(-1, 1)
                y += self.rng.randint(-1, 1)
            x = max(1, min(self.w - 2, x))
            y = max(1, min(self.h - 2, y))
            r = width / 2
            for dy in range(-int(r), int(r) + 1):
                for dx in range(-int(r), int(r) + 1):
                    if self.get(x + dx, y + dy) not in (None,) and 0 < x + dx < self.w - 1 and 0 < y + dy < self.h - 1:
                        self.set(x + dx, y + dy, ch)

    def house(self, x0, y0, x1, y1, wall, floor, doors):
        self.fill(x0, y0, x1, y1, floor)
        self.frame(x0, y0, x1, y1, wall)
        for x, y in doors:
            self.set(x, y, floor)

    def kept(self, x, y):
        return any(a <= x <= c and b <= y <= d for a, b, c, d in self.keep)

    def scatter(self, ch, density, area=None, on=('.',)):
        x0, y0, x1, y1 = area or (1, 1, self.w - 2, self.h - 2)
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                if self.get(x, y) in on and not self.kept(x, y) and self.rng.random() < density:
                    self.set(x, y, ch)

    def clumps(self, ch, n, rmin, rmax, area=None, on=('.',), rough=0.6):
        x0, y0, x1, y1 = area or (1, 1, self.w - 2, self.h - 2)
        for _ in range(n):
            cx, cy = self.rng.randint(x0, x1), self.rng.randint(y0, y1)
            r = self.rng.uniform(rmin, rmax)
            for y in range(int(cy - r - 1), int(cy + r + 2)):
                for x in range(int(cx - r - 1), int(cx + r + 2)):
                    if x0 <= x <= x1 and y0 <= y <= y1 and not self.kept(x, y):
                        d = ((x - cx) ** 2 + (y - cy) ** 2) / (r * r)
                        if d <= 1.0 + rough * (self.rng.random() - 0.5) and self.get(x, y) in on:
                            self.set(x, y, ch)

    def glob(self, x, y):
        return (self.origin[0] + x, self.origin[1] + y)

    def ron(self, header):
        used = {c for row in self.g for c in row}
        lines = ['// ' + l if l else '//' for l in header.strip().split('\n')]
        lines.append('// Generated by art-src/world_maps.py. Characters are layout, not game text.')
        lines.append('(')
        lines.append('    legend: {')
        for c, floor, bm, bs in LEGEND:
            if c not in used:
                continue
            extra = ''
            if bm:
                extra += ', blocks_movement: true'
            if bs:
                extra += ', blocks_sight: true'
            if c in SWITCH:
                extra += f', switch: Some({SWITCH[c]})'
            lines.append(f"        '{c}': (floor: {floor}{extra}),")
        lines.append('    },')
        lines.append('    rows: [')
        for row in self.g:
            lines.append('        "' + ''.join(row) + '",')
        lines.append('    ],')
        lines.append(')')
        return '\n'.join(lines) + '\n'


# --- Aurelia: the starting city -------------------------------------------------

def aurelia():
    m = Map('aurelia', (-32, -96), 128, 96, seed=11)
    m.border('T', gaps=[(62, 95, 65, 95), (16, 95, 19, 95)])
    # Wall and gates.
    m.fill(9, 7, 118, 67, '.')
    m.frame(8, 6, 119, 68, '#')
    # Streets.
    m.fill(62, 30, 65, 68, '_')           # avenue: guild, plaza, south gate
    m.fill(8, 50, 88, 53, '_')            # main street: west gate to the yard
    m.fill(50, 42, 78, 62, '_')           # plaza
    for x, y in ((50, 42), (78, 42), (50, 62), (78, 62), (59, 47), (69, 47), (59, 57), (69, 57)):
        m.set(x, y, '*')
    for x0, y0 in ((52, 44), (73, 44), (52, 58), (73, 58)):
        m.fill(x0, y0, x0 + 3, y0 + 2, 'f')
    # Guild hall (north).
    m.house(50, 12, 78, 30, '#', '=', [(x, 30) for x in range(62, 66)])
    for x, y in ((52, 14), (76, 14), (52, 28), (76, 28)):
        m.set(x, y, '*')
    # Training yard (east).
    m.fill(89, 41, 113, 61, ',')
    m.frame(88, 40, 114, 62, 'H')
    m.fill(88, 50, 88, 53, ',')
    for x, y in ((95, 45), (107, 45), (95, 57), (107, 57)):
        m.set(x, y, 'o')
    # Market quarter (west): smithy, herbalist, stalls.
    m.house(30, 34, 44, 46, '#', '_', [(36, 46), (37, 46), (38, 46)])
    m.fill(36, 47, 38, 49, '_')
    m.set(32, 36, '*')
    m.set(42, 36, '*')
    m.house(12, 34, 26, 46, 'H', '=', [(18, 46), (19, 46), (20, 46)])
    m.fill(18, 47, 20, 49, ',')
    m.fill(12, 26, 26, 31, 'f')
    m.fill(18, 56, 44, 66, ',')
    for x0 in (20, 27, 34, 41):
        m.fill(x0, 57, x0 + 1, 57, 'H')
    # North east houses and lanes.
    m.fill(79, 36, 117, 37, ',')
    m.fill(80, 36, 81, 49, ',')
    m.fill(97, 8, 98, 36, ',')
    for x0, y0, x1, y1 in ((84, 10, 95, 20), (101, 10, 114, 20), (84, 25, 95, 33), (101, 25, 114, 33)):
        m.house(x0, y0, x1, y1, 'H', '=', [((x0 + x1) // 2, y1)])
        m.fill((x0 + x1) // 2, y1 + 1, (x0 + x1) // 2, 35, ',')
    # North west park with a pond, and houses by the guild.
    m.ell(22, 15, 7, 4.5, '~', rough=0.3)
    m.clumps('f', 6, 1.5, 3, area=(10, 8, 32, 24))
    m.house(34, 10, 46, 20, 'H', '=', [(40, 20)])
    m.fill(40, 21, 49, 22, ',')
    m.fill(48, 21, 49, 49, ',')
    m.house(34, 25, 46, 31, 'H', '=', [(46, 28)])
    m.fill(47, 28, 47, 28, ',')
    # South quarter: houses between the gate and the market.
    m.house(70, 63, 84, 67, 'H', '=', [(77, 63)])
    m.house(88, 63, 104, 67, 'H', '=', [(88, 65)])
    m.keep += [(9, 50, 88, 53), (50, 42, 78, 62), (62, 30, 65, 68), (89, 41, 113, 61), (18, 56, 44, 66), (56, 63, 69, 67)]
    m.scatter('T', 0.025, area=(9, 7, 118, 67))
    m.scatter('b', 0.012, area=(9, 7, 118, 67))
    # Outside the walls: farms, the south road and the west road.
    m.fill(62, 69, 65, 95, ',')
    m.fill(1, 50, 7, 53, ',')
    m.path([(3, 50), (3, 81), (16, 81), (16, 94)], 4, ',')
    m.fill(16, 95, 19, 95, ',')
    m.fill(40, 74, 57, 88, '%')
    m.fill(71, 74, 92, 88, '%')
    m.keep += [(40, 70, 92, 90), (56, 69, 72, 95), (1, 50, 7, 53), (3, 50, 6, 84), (3, 81, 19, 95)]
    m.scatter('T', 0.10, area=(1, 1, 126, 5))
    m.scatter('T', 0.10, area=(120, 1, 126, 94))
    m.scatter('T', 0.10, area=(1, 1, 7, 94))
    m.scatter('T', 0.05, area=(8, 69, 119, 94))
    m.scatter('b', 0.02, area=(8, 69, 119, 94))
    m.clumps('f', 8, 1.5, 3, area=(8, 70, 119, 94))
    return m


# --- The field: first hunting ground ---------------------------------------------

def camp(m, x0, y0, x1, y1):
    """Safe camp around a zone's revive point: cleared ground, a brazier on
    each corner. The zone's `Safe` region covers the same box."""
    m.fill(x0, y0, x1, y1, ',')
    for x, y in ((x0, y0), (x1, y0), (x0, y1), (x1, y1)):
        m.set(x, y, '*')
    m.keep.append((x0, y0, x1, y1))


def field():
    m = Map('dev_field', (0, 0), 64, 64, seed=21)
    m.border('T', gaps=[(30, 0, 33, 0), (63, 20, 63, 23), (0, 40, 0, 43), (30, 63, 33, 63)])
    m.fill(30, 0, 33, 63, ',')
    m.fill(33, 20, 63, 23, ',')
    m.fill(0, 40, 30, 43, ',')
    # Duel ring (PvP) beside the crossroads.
    m.fill(34, 26, 39, 37, '_')
    for x, y in ((34, 26), (39, 26), (34, 37), (39, 37)):
        m.set(x, y, 'o')
    # Pond, ruined farmstead, flowers.
    m.ell(14, 54, 6, 4, '~', rough=0.4)
    m.ell(52, 34, 4, 3, '~', rough=0.4)
    for x0, y0, x1, y1 in ((44, 44, 56, 44), (44, 44, 44, 52), (56, 44, 56, 49), (48, 52, 56, 52)):
        m.fill(x0, y0, x1, y1, '#')
    m.fill(45, 45, 55, 51, ',')
    m.keep += [(30, 0, 33, 63), (33, 20, 63, 23), (0, 40, 30, 43), (34, 26, 39, 37), (29, 31, 35, 37), (45, 45, 55, 51)]
    camp(m, 25, 27, 32, 34)
    m.clumps('f', 10, 1.5, 3)
    m.scatter('T', 0.05)
    m.scatter('b', 0.02)
    m.scatter('o', 0.008)
    return m


# --- Briar hills ------------------------------------------------------------------

def briar():
    m = Map('briar_hills', (64, 0), 64, 64, seed=31)
    m.border('T', gaps=[(0, 20, 0, 23), (63, 40, 63, 43), (36, 63, 39, 63)])
    m.path([(0, 20), (36, 20), (36, 63)], 4, ',')
    m.fill(36, 40, 63, 43, ',')
    # Goblin camp east of the road, behind a palisade.
    m.fill(43, 25, 57, 37, ',')
    m.frame(42, 24, 58, 38, 'H')
    m.fill(42, 29, 42, 32, ',')
    m.fill(40, 29, 41, 32, ',')
    for x, y in ((46, 27), (54, 27), (46, 35), (54, 35)):
        m.set(x, y, '*')
    # Shaman stones (south east) and the spider hollow (north west).
    import math
    for i in range(8):
        a = i * math.pi / 4
        m.set(int(54 + 5 * math.cos(a)), int(53 + 4 * math.sin(a)), 'o')
    m.ell(18, 11, 9, 7, ',', rough=0.5, on=('.',))
    m.keep += [(0, 20, 39, 23), (36, 20, 39, 63), (36, 40, 63, 43), (42, 24, 58, 38), (49, 49, 59, 57), (10, 5, 26, 17)]
    camp(m, 24, 32, 31, 39)
    m.scatter('b', 0.09)
    m.scatter('o', 0.03)
    m.scatter('T', 0.04)
    return m


# --- Stonebrook: the mining town --------------------------------------------------

def stonebrook():
    m = Map('stonebrook', (128, -32), 64, 96, seed=41)
    m.border('P', gaps=[(0, 72, 0, 75), (30, 0, 33, 0)])
    # Cliffs along the north, a stream down the east.
    m.fill(1, 1, 62, 3, 'R')
    m.clumps('R', 10, 2, 4, area=(1, 1, 62, 10))
    m.path([(30, 0), (30, 30), (40, 30), (40, 40)], 4, ',')
    m.fill(61, 4, 62, 94, '~')
    # Town behind a palisade.
    m.fill(27, 41, 59, 87, '.')
    m.frame(26, 40, 60, 88, 'H')
    m.fill(40, 40, 43, 40, ',')
    m.fill(26, 72, 26, 75, ',')
    m.fill(0, 72, 26, 75, ',')
    m.fill(27, 72, 59, 75, ',')
    m.fill(40, 41, 43, 87, ',')
    m.fill(36, 68, 48, 80, '_')
    for x, y in ((36, 68), (48, 68), (36, 80), (48, 80)):
        m.set(x, y, '*')
    m.house(28, 44, 38, 54, '#', '=', [(33, 54), (34, 54)])
    m.fill(33, 55, 34, 71, ',')
    m.house(46, 44, 58, 54, 'H', '=', [(52, 54)])
    m.fill(52, 55, 52, 71, ',')
    m.house(28, 80, 36, 87, 'H', '=', [(32, 80)])
    m.house(50, 80, 58, 87, 'H', '=', [(54, 80)])
    m.house(50, 58, 58, 66, 'H', '=', [(50, 62)])
    m.fill(44, 62, 49, 62, ',')
    m.set(57, 46, '*')
    m.keep += [(26, 40, 60, 88), (0, 72, 26, 75), (28, 0, 45, 41)]
    m.scatter('P', 0.07, area=(1, 4, 58, 94))
    m.scatter('o', 0.04, area=(1, 4, 58, 94))
    m.scatter('T', 0.02, area=(27, 41, 59, 87))
    m.clumps('f', 4, 1, 2, area=(27, 41, 59, 87))
    return m


# --- Ironridge mines ---------------------------------------------------------------

IRON_ROOMS = {
    'hall': (32, 36, 7, 4),
    'bats': (13, 28, 8, 6),
    'kobolds': (48, 27, 9, 7),
    'spiders': (13, 9, 7, 5),
    'overseer': (49, 9, 7, 5),
    'golem': (31, 6, 8, 4),
}


def ironridge():
    m = Map('ironridge', (128, -96), 64, 64, fill='R', seed=51)
    # Outdoor slope below the mine mouth.
    m.fill(1, 42, 62, 62, '.')
    m.fill(30, 41, 33, 63, ',')
    m.set(28, 42, '*')
    m.set(35, 42, '*')
    m.fill(30, 40, 33, 41, ':')
    for name, (cx, cy, rx, ry) in IRON_ROOMS.items():
        m.ell(cx, cy, rx, ry, ':', rough=0.5)
    for a, b in (('hall', 'bats'), ('hall', 'kobolds'), ('bats', 'spiders'), ('kobolds', 'overseer'), ('spiders', 'golem'), ('overseer', 'golem')):
        m.wander(IRON_ROOMS[a][:2], IRON_ROOMS[b][:2], 3, ':')
    m.wander((32, 36), (32, 40), 3, ':')
    m.ell(9, 31, 2.5, 2, '~', on=(':',))
    m.ell(53, 31, 2, 1.5, '~', on=(':',))
    m.keep += [(30, 34, 34, 42), (28, 2, 34, 9)]
    camp(m, 27, 32, 34, 39)
    m.scatter('o', 0.03, area=(1, 1, 62, 40), on=(':',))
    m.scatter('o', 0.04, area=(1, 43, 62, 62))
    m.scatter('P', 0.08, area=(1, 43, 62, 62))
    m.border('R', gaps=[(30, 63, 33, 63)])
    return m


# --- Whisperwood -------------------------------------------------------------------

def whisperwood():
    m = Map('whisperwood', (-64, 0), 64, 96, seed=61)
    m.border('P', gaps=[(48, 0, 51, 0), (63, 40, 63, 43)])
    m.clumps('P', 70, 2, 5, rough=0.8)
    m.clumps('T', 40, 2, 4, rough=0.8)
    m.scatter('P', 0.18)
    m.scatter('b', 0.05)
    # Clearings.
    for cx, cy, rx, ry in ((16, 36, 10, 8), (30, 56, 8, 6), (18, 77, 13, 11), (50, 18, 7, 6), (8, 52, 4, 3), (24, 90, 6, 4)):
        m.ell(cx, cy, rx, ry, '.', rough=0.4)
    m.clumps('f', 8, 1, 2, area=(4, 26, 40, 94), on=('.',))
    # Trails.
    m.path([(48, 0), (48, 40), (63, 40)], 4, ',')
    m.path([(16, 40), (48, 40)], 3, ',')
    m.path([(16, 40), (16, 76)], 3, ',')
    m.path([(16, 58), (30, 58)], 3, ',')
    # Ranger lodge.
    m.house(10, 29, 20, 35, 'H', '=', [(15, 35)])
    m.fill(15, 36, 15, 40, ',')
    # Bandit camp: broken palisade, tents, a fire.
    m.frame(7, 68, 29, 87, 'H')
    for x0, y0, x1, y1 in ((15, 68, 18, 68), (29, 74, 29, 79), (7, 80, 7, 83), (20, 87, 23, 87)):
        m.fill(x0, y0, x1, y1, ',')
    for x0, y0 in ((10, 71), (22, 71), (10, 82), (23, 81)):
        m.house(x0, y0, x0 + 3, y0 + 2, 'H', '=', [(x0 + 1, y0 + 2)])
    m.set(18, 77, '*')
    m.keep = []
    return m


# --- Fenmoor and the village of Fenwick ----------------------------------------------

def fenmoor():
    m = Map('fenmoor', (0, 64), 128, 96, fill=';', seed=71)
    m.border('P', gaps=[(30, 0, 33, 0), (100, 0, 103, 0)])
    m.clumps('.', 25, 3, 7, on=(';',))
    m.clumps('~', 30, 2, 6, area=(2, 2, 125, 93), on=(';', '.'))
    # Reeds along the water.
    for y in range(1, m.h - 1):
        for x in range(1, m.w - 1):
            if m.get(x, y) in (';', '.') and m.rng.random() < 0.35 and any(m.get(x + dx, y + dy) == '~' for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                m.set(x, y, 'r')
    # Village on dry ground.
    m.ell(78, 76, 23, 16, '.', rough=0.2)
    m.house(70, 64, 84, 72, 'H', '=', [(77, 72)])
    m.house(88, 70, 96, 78, 'H', '=', [(88, 74)])
    m.house(60, 72, 67, 79, 'H', '=', [(67, 75)])
    m.house(68, 84, 76, 90, 'H', '=', [(72, 84)])
    m.fill(70, 76, 86, 82, '=')
    for x, y in ((70, 76), (86, 76), (70, 82), (86, 82)):
        m.set(x, y, '*')
    m.fill(77, 73, 77, 75, '=')
    m.fill(87, 74, 87, 75, '=')
    m.fill(68, 75, 69, 75, '=')
    m.fill(72, 83, 72, 83, '=')
    m.clumps('f', 5, 1, 2, area=(58, 62, 98, 90), on=('.',))
    # Boardwalks from the two northern gates and out to the hag's island.
    m.path([(30, 0), (30, 40), (60, 40), (60, 76), (70, 76)], 4, '=')
    m.path([(100, 0), (100, 52), (90, 52), (90, 66)], 4, '=')
    m.fill(88, 66, 93, 69, '=')
    m.path([(24, 86), (60, 86)], 2, '=')
    m.fill(60, 80, 61, 87, '=')
    m.ell(14, 87, 9, 6, '~', rough=0.2)
    m.ell(14, 87, 6, 4, '.', rough=0.2)
    m.fill(20, 86, 24, 87, '=')
    m.house(10, 84, 15, 88, 'H', '=', [(15, 86)])
    return m


ZONES = [
    (aurelia, 'Aurelia, the starting city: walls, the plaza with the summoning circle,\nthe guild hall (north), the training yard (east), the market, smithy and\nherbalist (west), farms and rats outside the south gate, a road west to\nWhisperwood.'),
    (field, 'The fields south of Aurelia (levels 1 to 5): crossroads to Aurelia (north),\nthe briar hills (east), Whisperwood (west) and Fenmoor (south); duel ring\nbeside the crossroads, a ruined farmstead.'),
    (briar, 'Briar hills (levels 3 to 7): spider hollow, goblin camp behind a palisade,\nthe shaman stones; roads west to the fields, east to Stonebrook, south to\nFenmoor.'),
    (stonebrook, 'Stonebrook, the mining town (second city): palisade, foreman, quartermaster\nand inn; the road north to the Ironridge mines.'),
    (ironridge, 'Ironridge mines (levels 10 to 14): bat cavern, kobold diggings, spider\nnest, the overseers, and the golem at the deep end. Veins of iron, copper and\nsilver.'),
    (whisperwood, 'Whisperwood (levels 5 to 9): ranger lodge, the dire wolf den and the\nbandit camp in a dense pine forest; trails to Aurelia and the fields.'),
    (fenmoor, 'Fenmoor (levels 8 to 12) and the village of Fenwick (third city): mud,\nreeds and boardwalks; frogs, slimes, wisps, and the swamp hag on her island.'),
]

# Points that must be reachable from the circle, in global coordinates
# (NPCs, monster spawn centers, nodes); filled by the content files.
POINTS = []


def check(maps):
    tiles = {}
    for m in maps:
        for y in range(m.h):
            for x in range(m.w):
                tiles[m.glob(x, y)] = m.g[y][x]
    start = (32, -44)
    seen = {start}
    q = deque([start])
    while q:
        x, y = q.popleft()
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            n = (x + dx, y + dy)
            if n not in seen and tiles.get(n, '#') not in BLOCKS:
                seen.add(n)
                q.append(n)
    bad = []
    for name, (x, y) in POINTS:
        near = [(x + dx, y + dy) for dx in range(-1, 2) for dy in range(-1, 2)]
        if not any(p in seen for p in near):
            bad.append((name, (x, y), tiles.get((x, y))))
    return bad, len(seen)


def load_points(content):
    import re
    for zone in os.listdir(os.path.join(content, 'zones')):
        for f in ('npcs.ron', 'monsters.ron', 'nodes.ron'):
            p = os.path.join(content, 'zones', zone, f)
            if not os.path.exists(p):
                continue
            for mm in re.finditer(r'\((?:npc|monster|node): "(\w+)", pos: \(x: (-?\d+), y: (-?\d+)\)', open(p).read()):
                POINTS.append((f'{zone}/{mm.group(1)}', (int(mm.group(2)), int(mm.group(3)))))


def main():
    a = argparse.ArgumentParser()
    a.add_argument('--content', default='content')
    a.add_argument('--check-only', action='store_true')
    args = a.parse_args()
    maps = []
    for fn, header in ZONES:
        m = fn()
        maps.append(m)
        if not args.check_only:
            d = os.path.join(args.content, 'zones', m.name)
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, 'map.ron'), 'w') as f:
                f.write(m.ron(header))
    load_points(args.content)
    bad, n = check(maps)
    print(f'{n} tiles reachable from the circle')
    for b in bad:
        print('UNREACHABLE', b)
    return 1 if bad else 0


if __name__ == '__main__':
    raise SystemExit(main())

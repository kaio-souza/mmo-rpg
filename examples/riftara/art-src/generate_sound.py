#!/usr/bin/env python3
"""Provisional Riftara sounds and zone music.

    python3 art-src/generate_sound.py [--out assets] [--bindings]

Synthesizes each sound with the standard library, then encodes it to OGG
Vorbis, mono, 44 100 Hz, with ffmpeg (libvorbis). ffmpeg is a content tool:
it runs here, never in `cargo build` or CI. Needs ffmpeg 6 or
newer with libvorbis.

Writes assets/sounds/<folder>/<name>.ogg, assets/music/<mood>.ogg (short
loops that end where they start, bound to no zone) and the "Generated sounds" part of
assets/CREDITS.md. assets/sounds/sounds.ron (which sound plays for each
event) belongs to the editor's Sounds screen: it is written only when it
does not exist yet, or with --bindings, which overwrites it.
"""
import argparse, math, os, random, shutil, subprocess, sys, tempfile, wave

RATE = 44100
LICENSE = "CC0"
CREDIT = "Riftara (art-src/generate_sound.py)"


def env(n, attack, decay_power=3.0):
    """Fast rise over `attack` samples, then a fall to silence."""
    out = []
    for i in range(n):
        if i < attack:
            out.append(i / max(1, attack))
        else:
            t = (i - attack) / max(1, n - attack)
            out.append((1.0 - t) ** decay_power)
    return out


def noise(n, seed, smooth=0.0):
    """White noise; `smooth` (0 to 1) is a one-pole low-pass."""
    rng = random.Random(seed)
    out, last = [], 0.0
    for _ in range(n):
        v = rng.uniform(-1.0, 1.0)
        last = smooth * last + (1.0 - smooth) * v
        out.append(last)
    return out


def tone(n, f0, f1=None, square=0.0):
    """A sine sliding from f0 to f1 Hz; `square` mixes in a harder edge."""
    f1 = f0 if f1 is None else f1
    out, phase = [], 0.0
    for i in range(n):
        f = f0 + (f1 - f0) * i / max(1, n)
        phase += 2.0 * math.pi * f / RATE
        s = math.sin(phase)
        hard = 1.0 if s >= 0 else -1.0
        out.append((1.0 - square) * s + square * hard)
    return out


def mix(*parts):
    n = max(len(p) for _, p in parts)
    out = [0.0] * n
    for gain, p in parts:
        for i, v in enumerate(p):
            out[i] += gain * v
    return out


def shaped(samples, e):
    return [s * k for s, k in zip(samples, e)]


def ms(x):
    return int(RATE * x / 1000)


def hit(seed):
    n = ms(130)
    body = shaped(noise(n, seed, 0.55), env(n, ms(2), 4.0))
    thump = shaped(tone(n, 160, 70), env(n, ms(3), 3.0))
    return mix((0.55, body), (0.6, thump))


def crit():
    n = ms(260)
    base = hit(11) + [0.0] * (n - ms(130))
    ring = shaped(tone(n, 1250, 1180), env(n, ms(4), 2.0))
    return mix((1.0, base), (0.25, ring))


def miss():
    n = ms(190)
    whoosh = noise(n, 21, 0.85)
    e = [math.sin(math.pi * i / n) ** 2 for i in range(n)]
    return [0.7 * s * k for s, k in zip(whoosh, e)]


def hurt():
    n = ms(170)
    low = shaped(tone(n, 110, 60), env(n, ms(3), 2.5))
    grit = shaped(noise(n, 31, 0.7), env(n, ms(2), 5.0))
    return mix((0.7, low), (0.35, grit))


def click():
    n = ms(28)
    return shaped(tone(n, 2100, 1700), env(n, ms(1), 3.0))


def error():
    a = shaped(tone(ms(110), 330, 300, 0.3), env(ms(110), ms(3), 1.5))
    b = shaped(tone(ms(150), 230, 200, 0.3), env(ms(150), ms(3), 2.0))
    return [0.45 * v for v in a + b]


def notice():
    """The System's own voice: clean and "digital"."""
    a = shaped(tone(ms(120), 880, 880, 0.15), env(ms(120), ms(4), 1.2))
    b = shaped(tone(ms(260), 1320, 1320, 0.15), env(ms(260), ms(4), 2.0))
    return [0.4 * v for v in a + b]


def cast():
    """A technique: a rising swish with a bright shimmer on top."""
    n = ms(320)
    swish = noise(n, 41, 0.8)
    e = [math.sin(math.pi * min(1.0, i / (0.7 * n))) ** 2 * (1.0 - i / n) for i in range(n)]
    shimmer = shaped(tone(n, 520, 1040), env(n, ms(40), 2.0))
    return mix((0.6, [a * b for a, b in zip(swish, e)]), (0.3, shimmer))


def note_freq(root, semis):
    return root * 2.0 ** (semis / 12.0)


def music(root, chords, beats, bpm, pluck, seed, warmth=0.5):
    """A loop: a pad on each chord plus a plucked line, written into a
    circular buffer so the tails wrap around and the loop has no seam."""
    beat = 60.0 / bpm
    total = int(RATE * beat * beats * len(chords))
    out = [0.0] * total
    rng = random.Random(seed)
    span = int(RATE * beat * beats)
    for c, chord in enumerate(chords):
        start = c * span
        length = int(span * 1.5)
        for semis in chord:
            for det in (-0.12, 0.12):
                f = note_freq(root, semis + det)
                phase = rng.uniform(0, 6.28)
                step = 2.0 * math.pi * f / RATE
                for i in range(length):
                    a = min(1.0, i / (0.35 * span)) * max(0.0, 1.0 - max(0, i - span) / (0.5 * span))
                    v = math.sin(phase + step * i)
                    v += warmth * 0.3 * math.sin(2 * (phase + step * i))
                    out[(start + i) % total] += 0.06 * a * v
        # pluck: one chord note per beat, an octave up
        for b in range(beats):
            semis = chord[rng.randrange(len(chord))] + 12
            f = note_freq(root, semis)
            n = int(RATE * beat * 0.9)
            at = start + int(b * RATE * beat)
            step = 2.0 * math.pi * f / RATE
            for i in range(n):
                k = (1.0 - i / n) ** 4
                out[(at + i) % total] += pluck * k * math.sin(step * i)
    return out


# (folder, name) -> samples; folder "music" writes assets/music/<name>.ogg
SOUNDS = {
    ("combat", "hit_1"): lambda: hit(1),
    ("combat", "hit_2"): lambda: hit(2),
    ("combat", "crit_1"): crit,
    ("combat", "miss_1"): miss,
    ("combat", "hurt_1"): hurt,
    ("ui", "click_1"): click,
    ("ui", "error_1"): error,
    ("system", "notice_1"): notice,
    ("technique", "cast_1"): cast,
    # Music by mood: key, chords (semitones over the root), beats
    # per chord, tempo, pluck level. No zone gets one here: the designer
    # picks which zones have music on the editor's Sounds screen.
    ("music", "town_calm"): lambda: music(220.0, [[0, 4, 7], [5, 9, 12], [7, 11, 14], [0, 4, 7]], 4, 84, 0.10, 1, 0.6),
    ("music", "village_calm"): lambda: music(196.0, [[0, 4, 7], [9, 12, 16], [5, 9, 12], [7, 11, 14]], 4, 76, 0.09, 2, 0.5),
    ("music", "forest_soft"): lambda: music(174.6, [[0, 3, 7], [8, 12, 15], [3, 7, 10], [10, 14, 17]], 4, 66, 0.07, 3, 0.3),
    ("music", "hills_light"): lambda: music(207.7, [[0, 7, 12], [2, 9, 14], [5, 12, 17], [7, 14, 19]], 4, 80, 0.08, 4, 0.4),
    ("music", "tense_low"): lambda: music(130.8, [[0, 3, 7], [1, 5, 8], [0, 3, 7], [-2, 2, 5]], 4, 56, 0.05, 5, 0.2),
    ("music", "dark_deep"): lambda: music(110.0, [[0, 7, 10], [0, 5, 10], [-2, 5, 8], [0, 7, 10]], 4, 60, 0.06, 6, 0.8),
}

# zone -> music; empty on purpose (music is a choice per zone)
ZONE_MUSIC = {}


def rel(folder, name):
    return f"music/{name}.ogg" if folder == "music" else f"sounds/{folder}/{name}.ogg"

# event -> (sounds, volume per mille, priority, positional)
BINDINGS = {
    "combat_hit": (["hit_1", "hit_2"], 800, 100, True),
    "combat_crit": (["crit_1"], 900, 150, True),
    "combat_miss": (["miss_1"], 600, 60, True),
    "combat_hurt": (["hurt_1"], 900, 200, False),
    "ui_click": (["click_1"], 500, 80, False),
    "ui_error": (["error_1"], 700, 180, False),
    "system_notice": (["notice_1"], 800, 220, False),
    "technique_use": (["cast_1"], 800, 120, True),
}


def write_wav(path, samples):
    peak = max(1e-6, max(abs(s) for s in samples))
    k = 0.9 / peak if peak > 0.9 else 1.0
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(int(max(-1.0, min(1.0, s * k)) * 32767).to_bytes(2, "little", signed=True) for s in samples))


def encode(wav, ogg):
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", wav, "-c:a", "libvorbis", "-q:a", "4",
         "-ar", str(RATE), "-ac", "1", "-fflags", "+bitexact", "-flags:a", "+bitexact", ogg],
        check=True,
    )


def ffmpeg_version():
    out = subprocess.run(["ffmpeg", "-version"], capture_output=True, text=True, check=True).stdout
    return out.splitlines()[0].split(" Copyright")[0]


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def sounds_ron():
    """Same fixed layout as defs::SoundConfig::to_ron."""
    lines = ["// Which sound plays for each event; edit on the editor's Sounds screen.", "(", "    sounds: {"]
    for (folder, name) in sorted(SOUNDS, key=lambda k: k[1]):
        lines.append(f"        {q(name)}: (file: {q(rel(folder, name))}, license: {q(LICENSE)}, credit: {q(CREDIT)}),")
    lines += ["    },", "    events: {"]
    for ev in sorted(BINDINGS):
        names, vol, pri, pos = BINDINGS[ev]
        lines.append(f"        {q(ev)}: (sounds: [{', '.join(q(n) for n in names)}], volume: {vol}, priority: {pri}, positional: {'true' if pos else 'false'}),")
    lines += ["    },", "    techniques: {", "    },", "    zones: {"]
    for zone in sorted(ZONE_MUSIC):
        lines.append(f"        {q(zone)}: (music: {q(ZONE_MUSIC[zone])}, volume: 700, fade: 24),")
    lines += ["    },", ")", ""]
    return "\n".join(lines)


START, END = "<!-- generate_sound.py: start -->", "<!-- generate_sound.py: end -->"


def credits(path, version):
    rows = "\n".join(f"| `{rel(f, n)}` | {LICENSE} | {CREDIT} |" for (f, n) in sorted(SOUNDS))
    block = (f"{START}\n## Generated sounds\n\nSynthesized by `art-src/generate_sound.py`, encoded with {version} "
             f"(libvorbis, quality 4, mono, {RATE} Hz).\n\n| File | License | Credit |\n|---|---|---|\n{rows}\n{END}\n")
    text = open(path, encoding="utf-8").read() if os.path.exists(path) else (
        "# Asset credits\n\nAuthor and license of every file under `assets/` that needs one "
        "(licences allowed: `CC0`, `CC-BY-3.0`, `CC-BY-4.0`, `commissioned`).\n\n")
    if START in text and END in text:
        head, rest = text.split(START, 1)
        text = head + block + rest.split(END, 1)[1].lstrip("\n")
    else:
        text = text.rstrip("\n") + "\n\n" + block
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", default="assets")
    p.add_argument("--bindings", action="store_true", help="overwrite sounds/sounds.ron")
    a = p.parse_args()
    if shutil.which("ffmpeg") is None:
        sys.exit("ffmpeg not found: install ffmpeg 6+ with libvorbis")
    version = ffmpeg_version()
    with tempfile.TemporaryDirectory() as tmp:
        for (folder, name), fn in sorted(SOUNDS.items()):
            dest = os.path.join(a.out, rel(folder, name))
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            wav = os.path.join(tmp, f"{name}.wav")
            write_wav(wav, fn())
            encode(wav, dest)
            print("wrote", dest)
    ron = os.path.join(a.out, "sounds", "sounds.ron")
    if a.bindings or not os.path.exists(ron):
        with open(ron, "w", encoding="utf-8") as f:
            f.write(sounds_ron())
        print("wrote", ron)
    else:
        print("kept", ron, "(the editor's; --bindings overwrites it)")
    credits(os.path.join(a.out, "CREDITS.md"), version)
    print("wrote", os.path.join(a.out, "CREDITS.md"))


if __name__ == "__main__":
    main()

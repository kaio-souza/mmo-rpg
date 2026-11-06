#!/usr/bin/env python3
"""Regenerates the provisional Riftara art (same names and sizes as assets/): python3 art-src/generate_art.py [--out assets]"""
import argparse, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import art_tiles, art_chars, art_icons, art_phase10, art_alpha, art_emotes, art_anim  # noqa: E402


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", default="assets")
    p.add_argument("--only", default="")
    a = p.parse_args()
    jobs = {}
    for mod in (art_tiles, art_chars, art_icons, art_phase10, art_alpha, art_emotes, art_anim):
        jobs.update(getattr(mod, "JOBS", {}))
    for path, fn in sorted(jobs.items()):
        if a.only and a.only not in path:
            continue
        dest = os.path.join(a.out, path)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        fn().image().save(dest)
        print("wrote", dest)


if __name__ == "__main__":
    main()

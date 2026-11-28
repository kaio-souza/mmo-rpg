# Overview

## What it is

A kit for building **2D isometric, tile-based MMOs**, written from scratch in Rust, MIT licensed.

- **Server:** authoritative, 20 ticks per second, zones that run in one process or many, built to hold 2000 players per world.
- **Client:** Linux and Windows, `wgpu` rendering, isometric projection, themes, sound, ten languages.
- **Editor:** a local web page to make items, NPCs, dialogues, quests, monsters, shops, recipes, maps, art and sounds without touching a file.
- **Tools:** `validate` (checks content and crate boundaries), `admin` (moderation and maintenance), `bots` (load tests), `replay` (re-runs a zone tick by tick), `pack` (release packaging).

A game is a folder with `content/` (rules as data) and `assets/` (images, fonts, sounds, themes, interface text). The fastest way to start is to copy [`templates/new-game/`](../templates/new-game/README.md), a small playable game.

## Who it is for

Small teams and solo developers who want an MMO of the classic kind (Astonia, Tibia, Ragnarok, Old School RuneScape) and would rather design the game than write a server.

## Principles

1. **Light and fast.** 2000 players per world with every tick under 41.7 ms (Astonia's tick). One server binary; a client with nothing to install. See [performance](performance.md).
2. **Content is data.** Classes, items, quests, dialogue, techniques, titles and the look of the game live in files, built from closed blocks. There is no scripting language.
3. **Content grows without hurting players.** New classes, titles, quests and items are added, never removed; a requirement counts when earning something, not for keeping it.
4. **Secure by default.** The server decides everything; the client sends intentions. TLS, argon2 passwords, no `unsafe`. See [security](security.md).
5. **Modular.** Each crate understands only its own context. See [architecture](architecture.md).
6. **Every text is a translation.** Keys in the data, text per language; ten languages ship with the template.
7. **Swappable look.** Theme, art and sound are called by name; the kit fixes no aesthetic.

## What is fixed

| Area | Choice |
|---|---|
| Camera | 2D isometric, tile grid ([concepts](concepts.md#the-isometric-world)) |
| Tile | 64×32 px diamond; characters on a 64×64 frame |
| Movement | Tile by tile, one creature per tile |
| Tick | 20 per second (50 ms) |
| Platforms | Client on Linux and Windows; server on Linux; browser later |
| Code | Rust only, nothing to install beyond `rustup` and the default C compiler |

## What it is not

- Not a general game engine: no 3D, no physics, no free movement.
- Not a hosted service: you run your own server ([operations](operations.md)).
- Not finished: see the [roadmap](roadmap.md) for what is missing (no stable content format version yet, no prebuilt binaries).

## The example game

**Riftara** (`examples/riftara/`) is a complete game made with the kit: seven zones, 20 monsters, 16 quests, 24 classes. It shows what the kit can do and is the data the tests run on. It is not part of the kit.

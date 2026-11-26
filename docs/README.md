# Documentation

This is a **kit for building 2D isometric MMOs**: a server, a client, an editor and tools. A game made with the kit is a folder of data (`content/` and `assets/`); nobody writes code to make one.

Start here, then read only the page you need.

| Page | What it answers |
|---|---|
| [Overview](overview.md) | What the kit is, who it is for, its principles, and what it is not |
| [Getting started](getting-started.md) | Make a game from the template, run it, edit it |
| [Concepts](concepts.md) | Game folder, content as data, the isometric world, ticks, zones, regions |
| [Content](content/README.md) | Every content file type, with its fields and an example to copy |
| [Assets](assets.md) | Images, animation strips, tiles, icons, themes, fonts, sounds |
| [Client](client.md) | Screens, controls, chat commands, what the client keeps |
| [Browser client](web.md) | Build the client for the browser, serve it, and the server's WebSocket port |
| [Character parts](character-parts.md) | Planned: a look built from parts and colours, and its editor screen |
| [Mechanics](mechanics.md) | What the engine does today, and the mechanics a game could add later |
| [Translation](translation.md) | How texts are translated: rules, glossary, tone per language |
| [Tools](tools.md) | Editor, `validate`, `admin`, `bots`, `replay`, `pack` |
| [Architecture](architecture.md) | Crates, who may depend on whom, and where new code goes |
| [Runtime](runtime.md) | Server modes, the zone tick, overload, interest, seams, saving |
| [Security](security.md) | Threats and answers, accounts, TLS, moderation |
| [Performance](performance.md) | Targets, measured benchmarks, how to measure |
| [Operations](operations.md) | Release, deploy, backup, updates |
| [Contributing](contributing.md) | Rules for code, content and translations; `./ci.sh` |
| [Decisions](decisions.md) | The decisions in force and why |
| [Roadmap](roadmap.md) | What is next and what is still open |
| [Glossary](glossary.md) | Short definitions |

## Repository layout

```text
server/  client/  shared/  tools/   the kit (Rust crates)
templates/new-game/                  the game you copy to start
examples/riftara/                    a complete example game (not part of the kit)
docs/                                this documentation
ops/                                 systemd units and scripts for a public server
arch.ron                             allowed crate dependencies, checked by `validate arch`
```

Paths such as `content/items/` in these pages are relative to a **game folder**: in the template, `templates/new-game/content/items/`.

# Instructions for Claude

Project: a **kit for building 2D isometric MMOs**, with an editor, in Rust, MIT licensed. A game is a folder with `content/` and `assets/`; a new game starts by copying `templates/new-game/`. **Riftara** (`examples/riftara/`) is only an example game and the tests' data: nothing of it belongs in the kit.

**Everything in the repository is in English**: code, identifiers, content ids, RON keys, database, logs, comments, commits and documentation. Replies to the user follow the user's language (Portuguese).

## How to navigate

1. [`docs/README.md`](docs/README.md) is the map. Read only the page your task needs.
2. To change a crate: [architecture](docs/architecture.md), then that crate's code and only the public API of the crates it depends on.
3. Content formats: [`docs/content/`](docs/content/README.md); to make content, copy a file of the same kind from the template or the example.
4. Unknown term: [glossary](docs/glossary.md). "Why is it like this?": [decisions](docs/decisions.md). What is next: [roadmap](docs/roadmap.md).

## Rules

- Before delivering any code change: `./ci.sh` (fmt, clippy, tests, `validate arch`, `validate` of the template and the example, `cargo deny`). All green.
- Respect crate boundaries ([architecture](docs/architecture.md#rules)). A change that crosses them updates the doc **and `arch.ron`** first.
- Deterministic simulation: in `sim` and `shared/`, no default `HashMap`, no floating point in rules, no clock or randomness outside the seed. Items never get `Clone`.
- Game content (abilities, classes, items, NPCs, texts, visual theme) is **data** in `content/` and `assets/`, never code. The engine only gains generic types.
- A new content type is ready only with its form and validation in the editor. Art is called by name, never by number or colour in code.
- Content only grows: ids are forever; requirements count for earning, not keeping; new enum variants are appended. Every item has a source in the game. Texts never promise a mechanic the engine lacks ([mechanics](docs/mechanics.md)).
- **No literal game text** in code, RON or images: only keys. English goes in with the change and the AI agent translates every other required language in the same change ([text](docs/content/text.md)).
- Keep the template small, playable and valid; keep the example valid.
- Do not copy code or assets from Astonia or any other game; they are references only.
- When a decision or contract changes, update its doc and [decisions](docs/decisions.md) in the same task. Undecided points are marked `⚠ Open` and listed in the [roadmap](docs/roadmap.md#open-questions).
- Docs: short pages (aim under 150 lines), mermaid diagrams, no broken links, every new page linked from `docs/README.md`.
- Generated or AI art goes to `testes-arte/` for review first, never straight into a game's `assets/`.

# Contributing

## Before every change

```sh
./ci.sh   # fmt, clippy -D warnings, tests, validate arch, validate the template and the example, cargo deny
```

Everything green. The same checks run in CI on Linux, Windows and a clean container.

## Language

- **Everything in English:** code, identifiers, content ids, RON keys, tables, logs, comments, commits and this documentation.
- **No literal game text** in code, RON or images: only text keys. A change that adds text adds every required language in the same change.

## Code

- Respect the crate boundaries ([architecture](architecture.md)). A change that needs a new edge updates `arch.ron` and the architecture page first.
- **Determinism** in `sim` and `shared/`: no default `HashMap`, no floating point in rules, no clock and no randomness outside the zone's seed.
- **Items never get `Clone`.**
- No `unsafe`. No `unwrap`, `expect` or direct indexing in `combat` and `defs`.
- Validate external input at the border; inside, trust the types.
- A performance regression is a bug ([performance](performance.md)).

## Content

- Game content (abilities, classes, items, NPCs, texts, look) is **data** in `content/` and `assets/`, never code. The engine only gains generic types.
- A new kind of content is ready only with its form and validation in the editor.
- Art is called by name; never delete a name players' content uses (empty means "generic, no art").
- Content only grows: ids are forever, a requirement counts when earning and not for keeping, new enum variants go at the end.
- Every item has a source in the game (loot, shop, node, quest or recipe). Texts never promise a mechanic the engine does not have ([mechanics](mechanics.md)).

## Adding a kind of content

1. The type in `shared/defs/` (file format, resolved form, validation), appended so old files still load.
2. The rule in the crate that owns it (`sim`, `progression`, `combat`), reading the validated type.
3. Protocol and client, if players see it.
4. The editor form: the file type derives its schema for the editor; pick lists for references.
5. An example in `examples/riftara/`, and in the template if every game needs it.
6. Its page in [`docs/content/`](content/README.md) and its line in [mechanics](mechanics.md).

## Translations

English is written first; the other required languages come in the same change, translated by the project's AI agent following the [translation guide](translation.md). Check with the editor's **Texts** screen or `validate`.

## Documentation

- Short pages (aim under 150 lines); split and link when one grows.
- Diagrams in mermaid.
- A changed decision or contract updates its page in the same change, and [decisions](decisions.md) when a decision changes.
- Undecided points are marked `⚠ Open` and listed in the [roadmap](roadmap.md#open-questions).
- No broken links.

## Example and template

`templates/new-game/` is the starting point users copy: keep it small, playable and valid. `examples/riftara/` is the full example and the tests' data; changes to the engine that affect content keep both valid.

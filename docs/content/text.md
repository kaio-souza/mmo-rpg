# Text and languages

No sentence of the game is written in code, content or images: only **keys**. Each language has its own text for every key.

## Languages

`content/text/locales.ron` lists the game's languages in menu order:

```ron
[
    (code: "en", required: true),
    (code: "pt-BR", required: true),
    (code: "ja", required: false),
]
```

- English is the source and always required.
- A missing text in a **required** language stops `validate`; in an optional one it is a warning and the game shows the English text.
- Plural and number rules come from the engine's table (about 30 codes, CLDR). A new language is a line here plus folders `content/text/<code>/` and `assets/text/<code>/`.
- The template ships ten: `en`, `pt-BR`, `zh`, `it`, `id`, `de`, `ru`, `es`, `ja`, `ko`.

## Where texts live

| File | What | Who resolves it |
|---|---|---|
| `content/text/<code>/*.ron` | content texts (items, monsters, NPCs, quests, System messages) | the client, with the text pack the server sends |
| `content/text/<code>/error.ron` | error messages | built into the client |
| `assets/text/<code>/ui.ron` | interface (menus, HUD, options, game title) | built into the client |

Any number of files per language; the template splits them into `kit.ron` (System, moderation, appearance), `error.ron` and `game.ron` (its own content).

## Format

English entries carry context for translators; other languages are just text.

```ron
// en
{ "item.wolf_pelt.name": (text: "Wolf pelt", note: "Item name.", max_chars: 40) }
// pt-BR
{ "item.wolf_pelt.name": "Pele de lobo" }
```

- `note`: context for the translator. `max_chars`: the longest visible text allowed, in every language.
- `secret: true`: the text is not in the pack sent to clients; the server sends it only when the player reaches it.
- Placeholders follow a small ICU MessageFormat subset: `{name}`, `{n, number}`, `{count, plural, one {# coin} other {# coins}}` (always with `other`), `{x, select, ...}`.

## Keys

Content keys follow the id: `item.<id>.name`, `item.<id>.desc`, `monster.<id>.name`, `npc.<id>.name`, `quest.<id>.name`, `class.<id>.name`, `technique.<id>.name`, `title.<id>.name`. Dialogue texts are named in the dialogue file. Interface keys start with `ui.`, errors with `error.`, System messages with `system.`.

## Translating

Every change that adds text adds all required languages in the same change. The editor's **Texts** screen shows key × language with "missing" marks, and the **Languages** screen manages `locales.ron`. See the [translation guide](../translation.md).

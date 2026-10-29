# Translation

The project's AI agent translates every **required** language of a game (`content/text/locales.ron`) in the same change that adds or changes an English text. There is no other translation process.

## Before translating

1. Read the English entry: `note` (what the text is for, who says it, when it shows) and `max_chars`.
2. Use the glossary below, and the game's own glossary if it has one (the example's: [`examples/riftara/translation.md`](../examples/riftara/translation.md)): a term that already has a translation keeps it.
3. Keep the System's tone: neutral and slightly dry; critical messages always neutral.

## Technical rules

- **Placeholders** keep their names (`{name}`, `{count}`, `{seconds}`); their order in the sentence may change.
- **Length:** `max_chars` counts visible characters; a placeholder counts 0 and `#` counts 1; in a plural, the longest branch counts. If it does not fit, rewrite shorter without losing the meaning.
- **Plural** categories follow CLDR: `one` and `other` for pt-BR, it, es, de; `one`, `few`, `many`, `other` for ru; only `other` for id, ja, ko, zh. `other` is always present.
- **English changed?** Review the other languages of that key.
- `./ci.sh` checks presence in required languages, identical placeholders, complete plurals and length. Language mistakes that pass those checks need a review pass with this page as context.

## Interface glossary

| en | pt-BR | it | es | de | id | ru | ja | ko | zh |
|---|---|---|---|---|---|---|---|---|---|
| System (prefix) | [Sistema] | [Sistema] | [Sistema] | [System] | [Sistem] | [Система] | [システム] | [시스템] | [系统] |
| HP (label) | VIDA | PV | VIDA | LP | HP | ЗДР | HP | HP | 生命 |
| Vigor | Vigor | Vigore | Vigor | Kraft | Vigor | Бодрость | 活力 | 활력 | 活力 |
| MP | MP | MP | MP | MP | MP | МП | MP | MP | MP |
| XP | XP | XP | XP | EP | XP | XP | 経験 | 경험치 | 经验 |
| copper (coin) | cobre | rame | cobre | Kupfer | tembaga | медных | 銅貨 | 구리 | 铜 |
| Map | Mapa | Mappa | Mapa | Karte | Peta | Карта | 地図 | 지도 | 地图 |

The HP and coin labels are the ones the screens use; keep the same choice in every key of a language.

## Proper names

A name of a city, region or character stays as in English in languages with the Latin alphabet. In other scripts, use a transliteration or a translation of the meaning, always the same in every key.

## Tone per language

- **pt-BR:** "você". **it:** "tu" in dialogue, without excessive formality. **es:** "tú". **de:** "du". **ru:** "ты" in NPC lines and game messages.
- **ja, ko, id:** the tone of the character speaking. **zh:** short sentences, no courtesy markers.

## Known points for the next review

- **id** mixes "kau" (NPC lines) and "Anda" (error messages): pick one.
- **ko** mixes 반말 and 존댓말 between NPCs; **ja** mixes です/ます with plain forms for masters (smith, instructor): fix per character.
- Quest and System verbs (deliver, take, complete) were translated without a verb glossary; check they use the same word within each language.

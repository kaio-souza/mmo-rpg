# Crafting with skill (planned)

Today a craft is one click: rarity and power come from proficiency, DEX and the roll ([mechanics](mechanics.md)). This page adds an **optional minigame per recipe** so the player's hands matter, without making the roll useless and without letting a bot do better than a person.

## Principles

- **Skill moves the odds, it does not replace them.** A perfect minigame raises the quality chance and the stats inside the recipe's range; it never leaves the range the designer set.
- **The server judges.** The client only sends inputs with their tick; the server replays them against the minigame's seed. Nothing the client says about "perfect" is trusted ([D4](decisions.md)).
- **Optional per recipe.** `minigame: None` keeps today's one-click craft (good for consumables and mass crafting).
- **Short.** 5 to 30 seconds. Long crafts are split into steps, not made longer.
- **Accessible.** Every minigame has a setting for wider windows (costs a small part of the bonus), and none depends on colour alone.

## Minigame kinds (closed list, appended over time)

| Kind | The player does | Fits |
|---|---|---|
| `Timing` | hits a moving marker inside a zone, N strikes | forging, smithing |
| `Heat` | keeps a gauge inside a band while it drifts | smelting, cooking, alchemy |
| `Sequence` | repeats a shown order of steps | tailoring, enchanting, jewelry |
| `Balance` | mixes 2 to 4 quantities to a hidden target, with hints | alchemy, dyes |
| `Assembly` | places parts on a small grid; fit and order matter | engineering, siege parts, furniture |

Each is a generic engine type; its numbers are content.

## Content: in `recipes/<id>.ron`

```ron
(
    // ... today's fields: materials, result, proficiency ...
    minigame: Some(Timing(
        strikes: 6,
        speed: 3,                       // 1..10
        zone_permille: 180,             // width of the good zone
        perfect_permille: 40,           // width of the perfect core
    )),
    skill_effect: (
        quality_bonus_permille: 150,    // added to the rarity chance at a perfect score
        stat_range: (900, 1100),        // stats scale between these by score
        fail_below: 200,                // under this score: materials lost (or half, see `fail_keeps`)
        fail_keeps_permille: 500,
    ),
    masterwork: Some((score_at_least: 950, chance_permille: 100)),
)
```

**Masterwork**: a perfect run may produce a *masterwork*: the item keeps the maker's signature (already in the engine) plus a **masterwork mark** and the score. Masterworks are the pieces a [server project](server-projects.md) asks for and the ones a chronicle remembers.

## Score and anti-bot

- Score 0..1000 from the inputs, computed by the server in fixed point.
- Every run has its own seed (from the zone seed and a counter), so the pattern is never the same twice.
- Bot telemetry ([operations](operations.md)) gains *crafts per hour* and *score variance*: a human's scores vary; a constant 1000 for hours is flagged for a person to review, never banned automatically.
- Ping: windows are judged on the tick the input was **sent** for, within the usual lag tolerance.

## Editor

- Recipe form: a **Minigame** section with kind, numbers and a **"Try it"** button that plays it in the editor.
- A curve preview: score → rarity chance and stat range.
- Validation: windows not too small for the speed (warning), `fail_below < masterwork.score_at_least`.

## Engine work

1. `defs`: `minigame`, `skill_effect`, `masterwork` (all optional).
2. `sim`: a craft session state per player (start, inputs, finish), score from the seed and inputs, result.
3. Protocol: start craft → seed and parameters; inputs with ticks; result.
4. Client: one UI widget per kind, theme-aware, keyboard and mouse.
5. Item: `masterwork: Option<Score>` beside the maker signature.

## Decided

- Decided ([D57](decisions.md)): a production class **widens the windows** by its tier (content) and adds to the bonus.
- Decided ([D57](decisions.md)): **group crafts** for big pieces (up to 4 players, one step each); the piece is signed by every maker.

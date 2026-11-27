# Everyday System

A small example game in the style of The Gamer ([genre references](../../docs/genre-references.md)): every player sees the world as a game, and every skill grows by use. It is built **only from content**, with no engine code of its own, and copies nothing from the story: its own world, names and characters. It starts from the [template](../../templates/new-game/).

## What is here

| Content | Where |
|---|---|
| Bare-hands fighting that grows by use, unlocking three techniques: a quick jab (3), a sweeping kick that hits around the target (15), a stunning palm (30) | `techniques/`, `proficiencies/unarmed.ron` |
| Titles for deeds, each with a passive bonus: first kill, 25 wolves, a first death, Unarmed 25, a dungeon cleared (told to everyone) and **Player One**, the first to reach level 10, one holder on the server, passed on after 30 days away | `titles/` |
| The **instant dungeon**: a portal on the corner of the plaza opens a copy for each group, with young wolves and an alpha wolf that bites harder below half health; killing it clears the run; a portal at the back leads out | `zones/training/`, `dungeons/instant_dungeon.ron`, `monsters/alpha_wolf.ron` |
| Rankings: highest level, most monsters, fastest dungeon clears (`/ranking`), and server firsts (`/firsts`) | `rankings/` |
| The blue "system window" look | `assets/theme.ron` (`isekai`) |

Run it like the template: `mmo play examples/everyday-system`.

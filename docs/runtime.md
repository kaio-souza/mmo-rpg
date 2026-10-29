# Runtime

How the server runs a world.

## Modes

| Mode | Runs | For |
|---|---|---|
| `all` (default) | gateway and every zone in one process | development, small servers |
| `gateway` | connections, TLS, login, database, routing | production |
| `zone` | one or more zones, no database | production |

```sh
server --mode gateway --link-listen 127.0.0.1:7100
server --mode zone --zones aurelia,whisperwood --gateway 127.0.0.1:7100
```

A zone process finds the gateway by a fixed address and introduces itself with its zone names and content hash. Only the gateway writes to the database. Zones share no state, so moving a zone is configuration. `--zone-link tcp` runs the same network path inside one process.

## The zone tick

Zones run as tasks on a `rayon` pool (one thread per core) at 20 ticks per second; ticks of different zones are staggered within the 50 ms.

1. Collect the commands queued for the zone.
2. `Zone::tick(input, &mut out)` in `sim`: movement, combat, AI, progression, quests.
3. Build each player's view in parallel (read only).
4. Encode each entity once and give every player a diff of the pieces they see.
5. Send; mark changed character sheets for saving.

## Overload

Ticks are never skipped and never rushed to catch up: a late tick slows the game down (time dilation) and the zone degrades by levels.

| Level | When | What |
|---|---|---|
| N1 | tick over 35 ms | far AI thinks less often |
| N2 | over 50 ms | far entities and ghosts update less often |
| N3 | N2 for 5 s | alert; new players are refused with `ZoneBusy` |

⚠ Open: the N1 and N2 effects are the design; the code measures the levels and N3 refuses entries, but the thinning of AI and far updates has not been checked.

When the world is full (`--max-players`, default 2000), new players wait in a queue and see their position and an estimate.

## What each player receives

- **Spatial grid:** 16×16-tile cells; neighbour searches look only at cells in range.
- **View radius:** the zone's `view_radius` (up to 64 tiles).
- **Line of sight:** nothing behind a wall is sent.
- **Diffs only:** the server remembers what each player knows and sends only changes.
- **Terrain** goes in 32×32-tile chunks in global coordinates; the client keeps those around the player.

## Seams

- Each zone also simulates a 24-tile band of each neighbour as **ghosts**: snapshots with no logic, exchanged as differences every tick through the gateway (with an empty beat every 5 ticks).
- An entity belongs to the zone of its tile. Monsters and NPCs never leave their zone.
- A player 2 tiles past the border is handed over: the gateway reroutes their commands to the new zone, which takes them without a new welcome. Nothing flickers.
- If a zone goes down, neighbours see its ghosts expire within a second and close that side; its players reconnect to their last save.
- Not yet: acting across a seam (attacking a ghost, AI that sees the other side), a gate queue between two sides, groups and trades across zones.

## Saving

- Sheets that changed are written in a **batch every 30 s**, outside the tick, **one transaction per zone** with sheets from the same tick, so both sides of a trade are always saved together.
- Trades and escrow are written with the sheets in the same transaction. A crash loses at most 30 s of progress and never duplicates or loses an item.
- Shutdown (Ctrl+C or `SIGINT`) saves everyone first.
- The database is SQLite: columns for what is searched (accounts, names, level, punishments, logs) and a versioned `postcard` blob for each character sheet. Opening an older database runs migrations. Postgres is planned behind the same interface.

## Crashes and replays

Each tick runs inside `catch_unwind`. A zone that panics is dropped without saving, its players get `ZoneFailure` and come back from their last save, the zone's recording is written to `replays/`, and the zone restarts. The rest of the server stays up.

```sh
replay replays/zone-meadow-<time>.replay --content content
```

prints `ok: N ticks reproduced with identical state hashes`, or the first tick that diverges.

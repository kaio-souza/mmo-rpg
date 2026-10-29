# Performance

A performance regression is a bug.

## Targets

| Metric | Target | Status |
|---|---|---|
| Players per world | 2000 (ceiling 3000; above that, a queue and more worlds) | one zone measured; a full spread world with combat not yet |
| Full world, 2000 players with combat | mean tick < 10 ms and every tick < 41.7 ms (Astonia's tick) | not measured yet |
| One zone, 1000 walking players | tick < 10 ms | **4.0 ms** |
| Worst case, 2000 in one zone | every tick < 41.7 ms | worst **19.4 ms** |
| Memory per connected player | < 32 KB | **~24 KB** |
| Gateway round trip, full world | p99 < 50 ms | **22 ms** |
| Bandwidth per player | < 4 KB/s average | counters exist; no per-player budget in code yet |
| Client | 60 FPS on an integrated GPU (Intel Iris Xe) | |

## Load on one zone (2026-11-16)

Every bot in the same zone (`load_test`, one thread), `bots --behavior walk`, a fresh `--dev` server per step, 60 s steady after everyone joined. i7-1355U laptop (12 threads) running server and bots together. The budget is 50 ms per tick (20/s); Astonia runs 24/s, so **41.7 ms**.

| Bots | Mean tick (sim only) | Worst tick (median of 5 s windows) | Windows with a spike > 41.7 ms | Load level | Memory | RTT p99 | Failures |
|---|---|---|---|---|---|---|---|
| 200 | 0.7 ms (0.6) | 1.3 ms | 0 of 14 | N0 | 35 MB | 20.2 ms | 0 |
| 500 | 1.6 ms (1.5) | 2.6 ms | 0 of 14 | N0 | 34 MB | 20.4 ms | 0 |
| 1000 | 4.0 ms (3.5) | 6.9 ms | 0 of 14 | N0 | 43 MB | 20.5 ms | 0 |
| 2000 | 10.5 ms (9.1) | 19.4 ms | 0 of 14 | N0 | 68 MB | 22.0 ms | 0 |
| 2500 | 15.5 ms (13.4) | 28.5 ms | 2 of 14 | N0 | 83 MB | 22.5 ms | 0 |
| 3000 | 22.2 ms (19.0) | 39.5 ms | 5 of 14 | N0 | 97 MB | 22.7 ms | 0 |
| 3500 | 30.5 ms (26.1) | 56.1 ms | 12 of 13 | N0, N1 | 106 MB | 22.6 ms | 0 |
| 4000 | 42.8 ms (37.0) | 64.4 ms | 12 of 12 | up to N2 | 120 MB | 40.1 ms | 0 |
| 5000 | 56.8 ms (48.2) | 84.9 ms | 8 of 8 | up to N3 | 150 MB | 40.5 ms | 0 |
| 7500 | — | — | — | N3 | 205 MB | 42.8 ms | 962 refused (zone closed to entries; back to the queue) |
| 10000 | — | — | — | N3 | 224 MB | 43.5 ms | 3276 refused |

One zone holds **~2000** players with every tick under 41.7 ms, **~3800** with the mean under it, and **~1900** under the 10 ms mean target. Cost grows faster than players (2× players, 2.6× time from 1000 to 2000) because each sees more neighbours. Above ~4000 the overload levels hold: nobody drops, the zone stops taking entries. Zones run on different threads, so a server holds more when players spread out.

## Tick with 2000 players (2026-11-18)

`cargo run --release -p sim --example tick_bench -- 2000 <content> load_test` (sim only, 400 ticks, same machine), 4 runs each: mean tick from 5.4–6.7 ms down to **4.0–4.3 ms** (−33%), worst from 9.8–18.8 ms to 7.2–9.3 ms, after caching line of sight for players who did not move and a cheaper state hash. A replay recorded with 2000 bots gives the same hashes before and after.

## Seams (Phase 10)

Gateway and two zones in three processes on one machine, `--threads 2` each, `bots --behavior seam` across the border.

| Bots | Time | Crossings | Second welcome / dropped | RTT p50 / p99 | Mean tick of the full zone | Memory per process |
|---|---|---|---|---|---|---|
| 50 | 2 min | 287 | 0 / 0 | 20.2 / 20.4 ms | 1.8 ms (~48 players) | 15–18 MB |
| 200 | 2 min | 121 | 0 / 0 | 20.2 / 20.7 ms | 7.5 ms (200 players) | 17–21 MB |

## Compared with other MMOs

| Game | Server tick | Players per world | Players per area |
|---|---|---|---|
| Astonia 3 | 24/s (41.7 ms) | ~30 areas, one process each | 255 per area |
| Tibia | — | ~1000, then a queue | one world, one server |
| World of Warcraft (2004) | 400 ms action batches | ~2500–3500 per realm | continents on separate servers |
| Old School RuneScape | 600 ms | 2000 | whole world in one process |
| EVE Online | 1 s | one world, 54–63 thousand | 1800 per system without slowdown; 6739 with time dilation |
| Albion Online | event queue | one world per region | 300 per map |
| New World | — | 2000 | — |
| **This kit (measured)** | **50 ms** | target 2000 | **~2000** with every tick < 41.7 ms (bots walking, no combat) |

No game with separate worlds goes beyond ~3500 per world; everyone grows with more worlds. Per area this kit is above all of them, with the caveat that our numbers are walking bots, theirs real play.

## Techniques

Server: one thread pool for all zones with staggered ticks; per-player interest in parallel; cached line of sight; a cheap state hash; encode each entity once and share the bytes; no allocation in the tick (reused buffers); dense integer ids, no string lookups or `HashMap` on hot paths; seam ghosts as differences; a spatial grid; one creature per tile and a view radius; diffs only; degradation by levels; saving outside the tick in batches; argon2 on its own pool; TLS and idle connections in the gateway.

Client: one sprite atlas; light in a shader; the floor pre-rendered per chunk; CJK fonts cut to the characters in use (planned in `pack`).

Planned: lazy AI (monsters with no player near sleep), a bandwidth budget per player, CI benchmarks failing on a 5% regression.

## How to measure

```sh
server --mode all --zone load_test --dev
bots --count 1000 --behavior walk --seconds 60
```

The server logs tick statistics every 5 s (mean, sim time, worst) and each zone's load level.

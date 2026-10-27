# Decisions

The decisions in force. When one changes, edit its row and add the date; a reversed decision keeps a line saying what replaced it.

| # | Decision | Why |
|---|---|---|
| D1 | **The product is a kit** for 2D isometric MMOs with an editor, MIT licensed. Riftara is an example in `examples/riftara/`, not part of the kit. A new game starts from `templates/new-game/`. (2026-11-20) | The engine was already generic; the value is in letting others make their games |
| D2 | **Documentation in English**, like code and content. (2026-11-20; before, docs were in Portuguese) | One language for everything a contributor reads |
| D3 | **Rust only**; nothing to install but `rustup` and the default C compiler; no system libraries | Builds anywhere, clean containers included |
| D4 | **Authoritative server, tile-based movement**, one creature per tile | Cheating is impossible by construction; simple to sync and lag tolerant |
| D5 | **Content is RON data with no scripting**: closed blocks (conditions, effects, objectives, rewards, bonuses) | Designers change the game without code; content cannot crash or exploit the server |
| D6 | **Content only grows**: ids are forever, requirements count for earning, new variants are appended | Updates never take anything from players |
| D7 | **A new content type is done only with its editor form and validation** | The editor is how games are made |
| D8 | **Art and sound are called by name**; missing art falls back to a generic shape | Swapping the look is swapping files |
| D9 | **Isometric 64×32 tiles, 64×64 character frames, 4 drawn directions in strips** (south, west, north, east) | One format for tools, art and renderer |
| D10 | **20 ticks per second**; overload slows time and degrades by levels, never skips ticks | Fast enough for tiles, predictable under load |
| D11 | **One server binary, modes `all`, `gateway`, `zone`**; zones share no state | Scaling out is configuration |
| D12 | **Open world of hidden zones joined by seams** (24-tile ghost band, hand-over past the border) | One map for players, fixed partitions for the server |
| D13 | **Deterministic simulation** with replays and state hashes; `hecs` ECS in `sim` | Bugs can be replayed; crashes are isolated per zone |
| D14 | **Items have identity and no `Clone`**; one owner per item, atomic saves for trades and escrow | Duplication is impossible |
| D15 | **TCP + TLS (`rustls`)**, `postcard` frames, transport swappable | Secure now; WebSocket or QUIC later without touching messages |
| D16 | **Every text is a translation**: keys only, languages as data (`locales.ron`), English first, the AI agent translates the rest in the same change; content texts resolved by the client from a cached text pack | Ten languages from day one at no runtime cost |
| D17 | **SQLite** with versioned character sheets and migrations; Postgres behind the same interface later | Zero setup for small servers |
| D18 | **Client on `wgpu` and `winit`**, own audio mixer with per-platform backends | Linux and Windows today, browser later, no system libraries |
| D19 | **2000 players per world** (ceiling 3000), more worlds beyond that; every tick under Astonia's 41.7 ms | Matches what other MMOs do; measured on one zone ([performance](performance.md)) |
| D20 | **Login defence**: attempts count on arrival per IP, account+IP and account; growing waits stored in the database; device tokens; proof of work under attack; 8 connections per IP | Brute force and account locking without hurting real players ([security](security.md)) |
| D21 | **Interface texts are built into the client** from the game folder in `MMO_GAME_DIR` (default `templates/new-game`); that folder's name is also the client's `GAME_ID`, naming its user and cache folders and its audio stream | Simple today; to be replaced by loading at runtime ([roadmap](roadmap.md)) |
| D23 | **The editor is how a designer runs a published game**: reachable only on the server's private network (VPN), never on the public internet; login with game accounts and roles, draft → publish with hot reload, history with restore; no terminal or files. (2026-11-20) | The people who make the game do not program |
| D24 | **The private network is WireGuard, self-hosted and set up by the kit** (not a managed VPN such as Tailscale). Installed from the system's package manager, never bundled, so its GPLv2 does not touch the kit's licence; free, no user limit. (2026-11-20) | No third party, no cost, no limit; selling the kit or a game stays free of the GPL ([operations](operations.md#private-network-planned)) |
| D22 | **Kit and example in one repository** | Tests run on the example's data; no versioned API to publish yet |

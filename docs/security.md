# Security

## Goals

1. The client cannot cheat: the server decides everything and sends only what a player may see.
2. Passwords and sessions survive a stolen database or a watched network.
3. No message, file or flood can crash the server or exhaust it.
4. Items and coins are never duplicated or lost.
5. Moderators can act, and every action is audited.

## Threats and answers

| Threat | Answer |
|---|---|
| Password capture on the network | TLS (`rustls`) on every connection outside `--dev`; the password never travels in clear |
| Database leak | argon2id password hashes; session and device tokens stored only as SHA-256 |
| Modified client (speed hack, teleport) | Authoritative server; the client sends intentions only |
| Map hack, wall hack | The server sends only what is in range and line of sight |
| Malformed, huge or junk messages | Size and range checks while decoding; an error, never a panic |
| Message flood | Messages per second limited per connection; disconnect when exceeded |
| Connection flood | At most 8 open connections per IP (IPv6: per `/64`), refused on accept (`--max-conns-per-ip`; loopback exempt) |
| Slow connection holding memory | Bounded outgoing queue; the connection drops when it fills |
| Password brute force | Each attempt counts on arrival, under three keys: IP, account from that IP, and account. After the free attempts (4 per account+IP, 10 per IP and per account) waits grow as 2ⁿ s up to 1 h, stored in the database; each error-free hour removes one. The same answer for "no such account" and "wrong password" |
| Locking someone else's account on purpose | A device token lets a known computer skip the whole-account wait (not its own IP's) |
| Distributed attack, login bots | Over 30 wrong passwords per minute on the server, login and register require proof of work (18 bits) for 10 minutes; fail2ban reads `login failed ip=…` |
| Mass account creation | 3 new accounts per IP per hour |
| Login storm | argon2 runs on its own pool with a cap on simultaneous logins; resuming with a token skips it |
| Malicious or broken content | No scripting; closed blocks validated before loading |
| Memory corruption | `unsafe_code = "forbid"` for the whole workspace |
| A panic taking the server down | `unwrap` and `expect` denied in the whole workspace (tests excepted); direct indexing also denied in `shared/` crates and `sim`; arithmetic that can overflow or divide by zero denied in `sim`, `combat` and `progression` (saturating or checked operations instead); each zone tick isolated with `catch_unwind` as the last net ([runtime](runtime.md#crashes-and-replays)) |
| Item duplication | Items have no `Clone`; one owner per item id (unique key); trades and escrow saved atomically |
| Bots | Not allowed. Per-character telemetry (kills per hour, hours without a break); `admin suspects` lists them for a human to review; progress gives less for repetition and nodes empty per player |
| Compromised dependency | Few dependencies, `cargo deny` in CI, nothing downloaded during the build |

## Accounts and sessions

- Password: 8 to 128 characters, not containing the account name; argon2id; wiped from memory when dropped.
- Session token: 32 random bytes, valid 24 h, replaced on every resume.
- Device token: 32 bytes, 90 days, up to 10 per account, renewed on each login, kept by the client in a `0600` file; changing the password revokes all.
- Wrong passwords since the last login are shown to the owner when they log in.
- One character in the game per account.

## TLS certificate

Production uses `--tls-cert` and `--tls-key` (PEM, e.g. Let's Encrypt). Without them the server creates a self-signed certificate for `localhost` and its address in `data/tls/`. The client trusts Mozilla's roots and, with `--trust-cert FILE`, that file; it has no option to trust anything.

Browser clients connect to `--ws-address` with the same certificate (`wss://`) and trust what the browser trusts, so a public server needs a real certificate. `--ws-origin` limits which pages may open the WebSocket; the frames, rate limits and per-IP limits are the same as on TCP ([web](web.md)).

## Team tools

The editor and any other team tool are reachable only through the server's WireGuard private network, never on the public IP, and still require a game account with the right role. Every action goes to `admin_log` ([operations](operations.md#private-network-planned); planned).

## Development mode

`--dev`: plain TCP and login by name only, characters saved under a passwordless `dev` account. It listens only on `127.0.0.1` and refuses any other address.

The client's `--name` (used by `mmo play --name`) only sends that development login; it gives nothing on a real server. A server without `--dev`, or a connection that is not from the same machine, answers `DevLogin` with `NotAllowed`. The guard is in the server, because any client can be modified; `server/tests/account_flow.rs` tests both rules.

## Chat

Messages up to 200 characters, no control characters, a rate limit per player, mute and report. Player text is never interpreted as formatting.

## Moderation

| Action | Target | Effect |
|---|---|---|
| `mute` | account, for a time | cannot talk |
| `suspend` | account, for a time | cannot log in; sessions dropped |
| `ban` | account, permanent | cannot log in; sessions dropped |
| `ip_ban` | IP (`/64`), up to 30 days | register, login and resume refused |

Roles are `player` and `admin`, changed only with `admin`, which needs database access. Each punishment has a reason from `moderation/reasons.ron`; revoking keeps the record. Every admin action, in game, terminal or editor, goes to `admin_log` and is never deleted. Logins with IP are kept 30 days.

## Rules for code

- Validate every external input (network, config, content) at the border and turn it into project types; inside, trust the types.
- Never log a password, token or key.
- Secrets come from environment variables or files with restricted permissions, never the repository.

# Operations

Running a public server. The files are in [`ops/`](../ops).

## Machine

- Linux VPS (Debian 12 or Ubuntu 22.04 and newer). Start with 4 vCPU, 8 GB and an SSD.
- The server runs as its own user (`game`), with no shell and no sudo.
- Firewall: the game port (7000/TCP), SSH with a key, and 80/TCP for Let's Encrypt.

| Path | What |
|---|---|
| `/opt/game/<version>/` | `server`, `admin`, `content/` and `ops/` of a release |
| `/opt/game/current` | link to the version in service; rolling back is moving the link |
| `/var/lib/game/` | `--data-dir`: `game.db`, `backup/`, `replays/` |
| `/etc/game/tls/` | certificate and key, readable only by `game` |

## Service

`ops/install.sh` creates the user, folders, systemd units and timers; `ops/game.service` runs:

```text
server --address 0.0.0.0:7000 --data-dir /var/lib/game --content /opt/game/current/content
       --replay-dir /var/lib/game/replays --tls-cert ... --tls-key ...
```

`SIGINT` stops it after saving everyone (`TimeoutStopSec=60`); `Restart=always`. Logs go to journald (`journalctl -u game`), with tick statistics every 5 s.

## Release

A `vX.Y.Z` tag runs `.github/workflows/release.yml`: it builds `server` and `admin` in a glibc container and the client for Linux and Windows, and publishes the server package (binaries, `content/`, `ops/`), the client with `assets/` beside it, and `SHA256SUMS`. Today the workflow packages the example game; for your game, point it at your game folder ([roadmap](roadmap.md)).

`ops/deploy.sh VERSION HOST [WAIT]` downloads the release, checks the sums, copies it over SSH, unpacks it into `/opt/game/<version>/` and schedules the update.

## Updates

1. Deploy unpacks the new version, moves `current` and schedules a restart with the `admin` of the **running** version (the new one never opens the database before the restart).
2. Everyone gets a maintenance notice at scheduling time and at 10 min, 5 min, 1 min and 10 s.
3. At zero the server saves everyone and exits; systemd starts the new version (about a minute down).
4. Clients that got the notice retry every 10 s for up to 10 minutes and resume with the same character.

`admin maintenance 10m` (up to 24 h) or `/maintenance 10m` in game schedules; `cancel` undoes. A client of an older protocol gets `IncompatibleVersion`; an older database is migrated on open.

## Backup

- Every hour the gateway writes `VACUUM INTO backup/game-<time>.db` from a read-only connection, outside the tick.
- Kept: the latest 48, plus the first of each day for 30 days.
- `ops/game-backup-offsite.timer` copies `backup/` to another server every hour (`/etc/game/backup.env`).
- `admin verify-backup FILE` checks integrity, schema version and counts; a timer runs it.

Restore: stop the service, keep the current `game.db` aside, copy the backup over `game.db`, delete `game.db-wal` and `game.db-shm`, start.

## Private network (planned)

Team tools (the editor, later others) are reachable only on the server's private network, never from the internet ([roadmap](roadmap.md#the-editor-on-a-published-game)).

- **WireGuard**, free and open source, in the Linux kernel since 5.6. `ops/install.sh` installs it with `apt install wireguard` (never bundled with the kit), creates the server's interface and keys, and opens its UDP port in the firewall. The game port stays the only other open port.
- **Adding a person:** `admin vpn-add <name>` writes a config file and prints a QR code. They install the free WireGuard app (Windows, macOS, Linux, Android, iOS) and import it once; afterwards they switch the VPN on and open the editor's link.
- **Removing a person:** `admin vpn-remove <name>` deletes their key; access ends at once.
- The editor listens only on the server's VPN address, and still asks for login with a game account and role (`admin`, `designer`, `moderator`).
- A hosting provider's own private network works the same way: the editor listens on that address instead.

## Login protection

`--max-conns-per-ip 8` and `--shield-per-min 30` ([security](security.md)). `install.sh` adds a fail2ban filter for `login failed ip=…` and a jail. `admin failed-logins` and the editor's Moderation screen show the attempts.

## Moderation and reports

```sh
admin show kai
admin suspend kai --for 1h --reason botting
admin revoke 1
admin log
admin reset-password kai
admin report economy
admin suspects
```

The editor's Economy and Moderation screens do the same with buttons.

# Getting started

## 1. Build the kit once

Rust only: the version is pinned in `rust-toolchain.toml`, plus the default C compiler that Rust uses (SQLite and `ring` are compiled with `cc`). No system libraries.

```sh
git clone <kit repository> ~/mmo-kit
cd ~/mmo-kit && cargo build --release
export PATH="$HOME/mmo-kit/target/release:$PATH"   # mmo, server, client, editor, validate, admin, bots, replay
```

The client needs a video driver (Vulkan, DirectX 12 or OpenGL) and a desktop session. Sound uses PulseAudio or PipeWire on Linux.

## 2. Make your game

```sh
mmo new ~/mygame
cd ~/mygame
```

`mmo new` copies the template (`templates/new-game` in the kit). The template is already playable: one area with a safe plaza, wolves and potions. Every command below runs **from inside the game folder** (except `mmo new`); `content/`, `assets/`, `data/` (database, certificate) and `replays/` are the game's.

## 3. Play it

```sh
mmo play --name Ana
```

It starts `server --dev` and the client on it, and stops the server when the client closes. Two terminals do the same by hand: `server --dev`, then `client --dev --server 127.0.0.1:7000 --name Ana`.

Click a tile to walk, scroll to zoom, `Tab` targets the nearest monster, `I` `C` `K` `L` `M` `O` `G` open inventory, sheet, techniques, quests, map, social and crafting, `Enter` opens chat, `F1` is help.

## 4. Change it

```sh
mmo edit          # the editor at http://127.0.0.1:7878 (local only)
```

Create maps (a new map creates a whole zone), items, NPCs, dialogues, quests, monsters, shops, recipes, techniques, classes and titles; upload images; set texts per language. With the server running (`mmo play`), it reloads `content/` every 2 s, so a change saved in the editor shows up in the game within seconds. Content with errors is refused and the server keeps the previous version.

Check without a server:

```sh
mmo check
```

## 5. Name your game

The title on the login screen is `ui.game.title` in `assets/text/<language>/ui.ron`. The client reads the interface texts from the game folder when it starts, so editing the file needs no build: start the client again.

## 6. Accounts and TLS

Without `--dev` the server only accepts TLS with accounts. The first run creates a self-signed certificate in `data/tls/` and the database `data/game.db`.

```sh
server
GAME_PASSWORD='a long password' client --server localhost:7000 --trust-cert data/tls/cert.pem \
    --account kai --register --character Kai     # later: drop --register and --character
```

## 7. Bigger worlds

A world can split into processes: one gateway (connections, login, database) and zone processes.

```sh
server --mode gateway --link-listen 127.0.0.1:7100
server --mode zone --zones meadow --gateway 127.0.0.1:7100
```

See [runtime](runtime.md).

## 8. Package it

```sh
mmo publish       # checks the game, then writes dist/: content, assets, server, client
```

The package is a folder you can copy to a server. Uploading it is the next step of the [roadmap](roadmap.md).

## Run the example

```sh
cd ~/mmo-kit/examples/riftara
server --dev --allow-missing-text
client --dev --server 127.0.0.1:7000 --name Ana   # in another terminal
```

Riftara's title and interface texts come from its folder, like any game's.

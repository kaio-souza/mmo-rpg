#!/bin/sh
# Builds the browser client of a game: ./client/client/web/build.sh GAME_DIR OUT_DIR
# GAME_DIR holds content/ and assets/. OUT_DIR gets index.html, client.js,
# client_bg.wasm and game.bundle: serve it as static files (see docs/web.md).
# Needs: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli
# (the version in Cargo.lock).
set -eu
game=${1:?usage: build.sh GAME_DIR OUT_DIR}
out=${2:?usage: build.sh GAME_DIR OUT_DIR}
root=$(cd "$(dirname "$0")/../../.." && pwd)
web=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
cargo build --manifest-path "$root/Cargo.toml" --locked --release -p client --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir "$out" "$root/target/wasm32-unknown-unknown/release/client.wasm"
cp "$web/index.html" "$out/"
cargo run --manifest-path "$root/Cargo.toml" --locked -q --release -p client-files --bin web-bundle -- \
    "$game/content" "$game/assets" "$out/game.bundle"

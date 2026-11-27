#!/bin/sh
# Builds the browser client and runs it locally with a dev server: one command.
#   ./client/client/web/play.sh [GAME_DIR] [PAGE_PORT]
# GAME_DIR (default templates/new-game) holds content/ and assets/.
# Opens http://127.0.0.1:PAGE_PORT/ (default 8080). Ctrl+C stops both servers.
# Test two players: open the page in two windows, with ?name=alice and ?name=bob.
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd)
game=$(cd "${1:-$root/templates/new-game}" && pwd)
port=${2:-8080}
out="$root/dist/web"
data="$root/dist/server-data"

need() {
    command -v "$1" >/dev/null 2>&1 || { echo "missing: $1 ($2)" >&2; exit 1; }
}
need cargo "install Rust: https://rustup.rs"
need python3 "needed to serve the page"
if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    echo "installing the wasm32 target..."
    rustup target add wasm32-unknown-unknown
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
    want=$(grep -A1 '^name = "wasm-bindgen"$' "$root/Cargo.lock" | sed -n 's/^version = "\(.*\)"/\1/p')
    echo "installing wasm-bindgen-cli $want (a few minutes the first time)..."
    cargo install wasm-bindgen-cli --version "$want" --locked
fi

echo "== building the client for the browser"
"$root/client/client/web/build.sh" "$game" "$out"

echo "== building the server"
cargo build --manifest-path "$root/Cargo.toml" --locked -q -p server
server_bin="$root/target/debug/server"

pids=""
cleanup() { for p in $pids; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT INT TERM

echo "== starting the server (game: $game)"
(cd "$game" && exec "$server_bin" --dev --address 127.0.0.1:7000 --ws-address 127.0.0.1:7001 --data-dir "$data") &
pids="$pids $!"

echo "== serving the page"
(cd "$out" && exec python3 -m http.server "$port" --bind 127.0.0.1) >/dev/null 2>&1 &
pids="$pids $!"

echo ""
echo "Open: http://127.0.0.1:$port/?server=127.0.0.1:7001&dev&name=alice"
echo "Second player: same link with name=bob. Ctrl+C stops."
wait

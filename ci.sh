#!/bin/sh
# Same checks as .github/workflows/ci.yml, for local use: ./ci.sh
set -eu
cd "$(dirname "$0")"
cargo fmt --all --check
cmp shared/clippy.toml server/sim/clippy.toml
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -q -p validate -- arch
cargo run --locked -q -p validate -- examples/riftara/content/ --assets examples/riftara/assets/
cargo run --locked -q -p validate -- templates/new-game/content/ --assets templates/new-game/assets/
if command -v cargo-deny >/dev/null; then cargo deny check; else echo "cargo-deny not installed: skipped (cargo install cargo-deny)"; fi
echo "all checks passed"

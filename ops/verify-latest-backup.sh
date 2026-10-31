#!/bin/sh
# Checks the newest backup copy in DIR with `admin verify-backup`
# Exit code 1 when it fails.
set -eu
dir="${1:?usage: verify-latest-backup.sh DIR}"
here="$(dirname "$(readlink -f "$0")")"
newest="$(ls -1 "$dir" | grep -E '^game-[0-9]+\.db$' | sort -t- -k2 -n | tail -n 1 || true)"
if [ -z "$newest" ]; then
    echo "no backup in $dir" >&2
    exit 1
fi
exec "$here/../admin" verify-backup "$dir/$newest"

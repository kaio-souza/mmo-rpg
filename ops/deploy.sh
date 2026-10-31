#!/bin/sh
# Deploys a release to the VPS:
#   ops/deploy.sh v0.1.0 root@game.example.org [10m]
# Downloads the server package with `gh`, checks its SHA-256, copies it
# over SSH, unpacks it in /opt/game/<version>/, announces the restart with
# the admin of the version in the air (the new one never touches the live
# database before the restart), and points `current` at the new version.
# At the end of the countdown the server saves everyone and stops;
# systemd starts it again from `current`.
set -eu
version="${1:?usage: deploy.sh VERSION HOST [WAIT]}"
host="${2:?usage: deploy.sh VERSION HOST [WAIT]}"
wait="${3:-10m}"
repo="${REPO:-kaio-souza/mmo-rpg}"
name="server-$version-linux-x86_64"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
gh release download "$version" --repo "$repo" --pattern "$name.tar.gz" --pattern SHA256SUMS --dir "$tmp"
(cd "$tmp" && grep " $name.tar.gz\$" SHA256SUMS | sha256sum -c -)
scp "$tmp/$name.tar.gz" "$host:/tmp/$name.tar.gz"
ssh "$host" sh -s -- "$version" "$name" "$wait" <<'REMOTE'
set -eu
version="$1"; name="$2"; wait="$3"
mkdir -p "/opt/game/$version"
tar -C /tmp -xzf "/tmp/$name.tar.gz"
cp -a "/tmp/$name/." "/opt/game/$version/"
rm -rf "/tmp/$name" "/tmp/$name.tar.gz"
old="$(readlink -f /opt/game/current || true)"
ln -sfn "/opt/game/$version" /opt/game/current
if [ -n "$old" ] && [ "$old" != "/opt/game/$version" ] && systemctl is-active --quiet game; then
    su -s /bin/sh game -c "'$old/admin' --db sqlite:/var/lib/game/game.db --content '$old/content' maintenance '$wait'"
    echo "restart announced in $wait; the server comes back on $version"
else
    systemctl restart game
    echo "started $version"
fi
REMOTE

#!/bin/sh
# First install on the VPS, as root, from an unpacked server release
# user `game`, folders, systemd units.
# Then: certificate in /etc/game/tls/, BACKUP_TARGET in
# /etc/game/backup.env, and `systemctl enable --now game`.
set -eu
release="$(dirname "$(readlink -f "$0")")/.."
version="${1:?usage: install.sh VERSION}"
id game >/dev/null 2>&1 || useradd --system --home /var/lib/game --shell /usr/sbin/nologin game
mkdir -p "/opt/game/$version" /var/lib/game/backup /var/lib/game/replays /etc/game/tls
cp -a "$release/." "/opt/game/$version/"
ln -sfn "/opt/game/$version" /opt/game/current
chown -R game:game /var/lib/game
chown root:game /etc/game/tls
chmod 750 /etc/game/tls
for unit in game.service game-backup-offsite.service game-backup-offsite.timer \
    game-verify-backup.service game-verify-backup.timer; do
    cp "$release/ops/$unit" /etc/systemd/system/
done
systemctl daemon-reload
systemctl enable game-backup-offsite.timer game-verify-backup.timer
# Fail2ban reads the gateway's wrong-password lines, if installed.
if [ -d /etc/fail2ban ]; then
    cp "$release/ops/fail2ban/game-login.conf" /etc/fail2ban/filter.d/game-login.conf
    cp "$release/ops/fail2ban/game.jail.conf" /etc/fail2ban/jail.d/game.conf
    systemctl reload fail2ban 2>/dev/null || true
fi
echo "installed $version; next: certificate, /etc/game/backup.env, systemctl enable --now game"

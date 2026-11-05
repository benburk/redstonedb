#!/usr/bin/env bash
# Builds the committed HEAD on the server, backs up the live database,
# installs the new binaries and service files, restarts the site, and checks
# that it answers.
#
# A release with new migrations is worth trying first on a copy of the live
# data: `just setup live`, then `just serve`. To roll back, move each `.previous`
# binary in /usr/local/lib/redstonedb back over its new one and restart the
# service; if the bad release migrated the database, restore the copy taken
# below, with the site stopped.
set -euo pipefail

host=${DEPLOY_HOST:?set DEPLOY_HOST in .env, as user@host}
src=/root/build/redstonedb
lib=/usr/local/lib/redstonedb

cd "$(git rev-parse --show-toplevel)"
if ! git diff --quiet HEAD || [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo "Commit first: deploy ships HEAD, not the working tree." >&2
    exit 1
fi
commit=$(git rev-parse --short HEAD)

git archive --format=tar HEAD | ssh "$host" \
    "rm -rf $src && mkdir -p $src && tar -x -C $src"
# The target directory sits outside the source, so builds stay incremental.
# Building comes before anything changes, so a failed build leaves the site
# as it was.
ssh "$host" "set -e
    cd $src
    SQLX_OFFLINE=true CARGO_TARGET_DIR=/root/build/target \
        ~/.cargo/bin/cargo build --release --locked --bins"

# The new binary may migrate the database as it starts, so a copy is taken
# first, as the site's user and named so the daily pruning keeps it 14 days.
ssh "$host" "set -e
    runuser -u redstonedb -- sqlite3 /var/lib/redstonedb/redstone.db \
        \".backup /var/backups/redstonedb/redstone-$(date +%F-%H%M%S)-before-$commit.db\"
    install -d $lib
    for bin in redstonedb rsdb; do
        if [ -e $lib/\$bin ]; then cp -p $lib/\$bin $lib/\$bin.previous; fi
        install -m 755 /root/build/target/release/\$bin $lib/
    done
    install -m 755 $src/deploy/rsdb /usr/local/bin/rsdb
    # The site is the server's only one, so its Caddyfile is the whole file.
    if ! caddy validate --config $src/deploy/Caddyfile --adapter caddyfile >/dev/null 2>&1; then
        echo 'deploy/Caddyfile is not valid; nothing was restarted:' >&2
        caddy validate --config $src/deploy/Caddyfile --adapter caddyfile >&2 || true
        exit 1
    fi
    install -m 644 $src/deploy/Caddyfile /etc/caddy/Caddyfile
    systemctl reload-or-restart caddy
    install -m 644 $src/deploy/redstonedb.service \
        $src/deploy/redstonedb-backup.service \
        $src/deploy/redstonedb-backup.timer /etc/systemd/system/
    systemctl daemon-reload
    systemctl enable --quiet --now redstonedb-backup.timer
    systemctl enable --quiet redstonedb
    previous=\$(cat $lib/DEPLOYED 2>/dev/null || echo none)
    systemctl restart redstonedb
    for i in \$(seq 1 20); do
        if curl -fs -o /dev/null http://127.0.0.1:3100/; then
            echo $commit > $lib/DEPLOYED
            echo \"Live: \$previous -> $commit\"
            exit 0
        fi
        sleep 0.5
    done
    echo 'The site did not answer after restarting. Its log:' >&2
    journalctl -u redstonedb -n 30 --no-pager -o cat >&2
    exit 1"

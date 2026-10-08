#!/usr/bin/env bash
# Builds the committed HEAD on the server, installs it with its service
# files, and restarts the site. The server's one-time setup is in
# deploy/README.md.
set -euo pipefail

host=${DEPLOY_HOST:-root@157.180.85.191}
src=/root/build/redstone-database

cd "$(git rev-parse --show-toplevel)"
if ! git diff --quiet HEAD; then
    echo "Commit first: deploy ships HEAD, not the working tree." >&2
    exit 1
fi

git archive --format=tar HEAD | ssh "$host" \
    "rm -rf $src && mkdir -p $src && tar -x -C $src"
# The target directory sits outside the source, so builds stay incremental.
ssh "$host" "set -e
    cd $src
    SQLX_OFFLINE=true CARGO_TARGET_DIR=/root/build/target \
        ~/.cargo/bin/cargo build --release --locked --bins
    install -d /usr/local/lib/redstonedb
    install -m 755 /root/build/target/release/redstone-database \
        /root/build/target/release/rsdb /usr/local/lib/redstonedb/
    install -m 755 deploy/rsdb /usr/local/bin/rsdb
    install -m 644 deploy/redstonedb.service deploy/redstonedb-backup.service \
        deploy/redstonedb-backup.timer /etc/systemd/system/
    systemctl daemon-reload
    systemctl enable --now redstonedb-backup.timer
    systemctl enable redstonedb
    systemctl restart redstonedb
    sleep 2
    systemctl is-active redstonedb"
echo "Deployed $(git rev-parse --short HEAD)."

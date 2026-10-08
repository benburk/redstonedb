#!/usr/bin/env bash
# Replaces the local database with a copy of the live one. The local one is
# backed up to backups/ first. The copy goes in through SQLite's .restore, so
# a local server that is running sees the new data rather than having its
# file swapped out from under it.
set -euo pipefail

host=${DEPLOY_HOST:-root@157.180.85.191}
stamp=$(date +%Y%m%d-%H%M%S)
# In the site's backup directory, made by the site's user: a root-owned
# -wal or -shm beside the live database would lock the site out of it.
remote=/var/backups/redstonedb/pull-$stamp.db
copy=$(mktemp --suffix=.db)
trap 'rm -f "$copy"' EXIT

cd "$(git rev-parse --show-toplevel)"
ssh "$host" "runuser -u redstonedb -- sqlite3 /var/lib/redstonedb/redstone.db '.backup $remote'"
scp -q "$host:$remote" "$copy"
ssh "$host" "rm -f $remote"
if [ "$(sqlite3 "$copy" 'PRAGMA integrity_check')" != ok ]; then
    echo "The copy failed SQLite's integrity check; the local database is unchanged." >&2
    exit 1
fi

mkdir -p backups
if [ -f redstone.db ]; then
    sqlite3 redstone.db ".backup backups/local-$stamp.db"
    echo "Backed up the local database to backups/local-$stamp.db."
fi
sqlite3 redstone.db ".restore $copy"
echo "redstone.db now holds the live data: $(sqlite3 redstone.db 'SELECT count(*) FROM schematic') builds."

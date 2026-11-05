#!/usr/bin/env bash
# Replaces the local database with a copy of the live one. The local one is
# backed up to backups/ first. The copy goes in through SQLite's .restore, so
# a local server that is running sees the new data rather than having its
# file swapped out from under it.
set -euo pipefail

host=${DEPLOY_HOST:?run it through just, which sets DEPLOY_HOST, or set it as user@host}
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
# Live sign-ins and the Schemati sessions people gave stay on the server: a
# copy of either lets whoever holds it act as that person. What the site knew
# of each Schemati session goes with it. VACUUM rewrites the file, so the
# deleted rows don't linger in its free pages.
sqlite3 "$copy" "DELETE FROM session;
    UPDATE user SET schemati_session = NULL, schemati_player = NULL,
        schemati_checked_at = NULL, schemati_expired_at = NULL;
    VACUUM;"

mkdir -p backups
if [ -f database.sqlite3 ]; then
    sqlite3 database.sqlite3 ".backup backups/local-$stamp.db"
    echo "Backed up the local database to backups/local-$stamp.db."
fi
sqlite3 database.sqlite3 ".restore $copy"
echo "database.sqlite3 now holds the live data: $(sqlite3 database.sqlite3 'SELECT count(*) FROM schematic') builds."

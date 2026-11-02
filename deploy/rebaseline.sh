#!/bin/sh
# Moves a database made by the migrations before the baseline onto it, once:
# builds a new database from the baseline, copies every row across, and
# records the baseline as applied, as sqlx would have. The old file is kept
# beside it as <db>.before-baseline. Run it with nothing else using the
# database, as the user who owns it.
#
#   deploy/rebaseline.sh redstone.db migrations/20261102120000_baseline.sql
set -eu
db=$1
baseline=$2
new=$db.baseline
version=20261102120000

[ ! -e "$new" ] || { echo "$new is in the way" >&2; exit 1; }
applied=$(sqlite3 "$db" "SELECT count(*) FROM _sqlx_migrations WHERE version = $version")
[ "$applied" = 0 ] || { echo "$db is already on the baseline" >&2; exit 1; }

sqlite3 "$db" "PRAGMA wal_checkpoint(TRUNCATE)" >/dev/null
sqlite3 "$new" <"$baseline"
# sqlx's own table, and its record of the baseline: the checksum is the
# file's SHA-384.
checksum=$(sha384sum "$baseline" | cut -d' ' -f1)
sqlite3 "$new" <<SQL
CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);
INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
VALUES ($version, 'baseline', 1, X'$checksum', 0);
SQL

# The old database's catalogue replaces the baseline's, as it may have been
# added to since. Each table's rows go across by the columns both tables
# have; a column the old one lacks takes its default.
tables=$(sqlite3 "$new" "SELECT name FROM sqlite_master WHERE type = 'table'
    AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'")
# Written out whole before it runs: the copy holds the new database's write
# lock, so reading its columns meanwhile would wait on it.
copy="PRAGMA foreign_keys = OFF; ATTACH '$db' AS old; BEGIN;"
for table in $tables; do
    columns=$(sqlite3 "$new" "ATTACH '$db' AS old; SELECT group_concat(name, ', ')
        FROM pragma_table_info('$table') WHERE name IN
        (SELECT name FROM pragma_table_info('$table', 'old'))")
    copy="$copy DELETE FROM main.$table;
        INSERT INTO main.$table ($columns) SELECT $columns FROM old.$table;"
done
sqlite3 "$new" "$copy COMMIT;"

problems=$(sqlite3 "$new" "PRAGMA foreign_key_check" | head -5)
[ -z "$problems" ] || { echo "foreign keys broken:" >&2; echo "$problems" >&2; exit 1; }
[ "$(sqlite3 "$new" 'PRAGMA integrity_check')" = ok ] || { echo "integrity check failed" >&2; exit 1; }
for table in $tables; do
    before=$(sqlite3 "$db" "SELECT count(*) FROM $table")
    after=$(sqlite3 "$new" "SELECT count(*) FROM $table")
    [ "$before" = "$after" ] || { echo "$table: $before rows became $after" >&2; exit 1; }
done

mv "$db" "$db.before-baseline"
rm -f "$db-wal" "$db-shm"
mv "$new" "$db"
echo "$db is on the baseline; the old file is $db.before-baseline"

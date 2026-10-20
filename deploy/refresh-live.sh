#!/usr/bin/env bash
# Refreshes live builds on this machine's cores rather than the server's:
# copies the live database here, runs `rsdb simulate` on each build named,
# by id or the start of one, or of each category named with --category,
# then sends their results back. The server, its database backed up first,
# takes each build's results only while its file and details are as they
# were copied, so an upload or an edit since is never undone.
set -euo pipefail

host=${DEPLOY_HOST:-root@157.180.85.191}
cd "$(git rev-parse --show-toplevel)"

ids=()
categories=()
while [ $# -gt 0 ]; do
    case "$1" in
    --category)
        [[ ${2:-} =~ ^[-a-z0-9_]+$ ]] || {
            echo "--category takes a category's name, such as adder" >&2
            exit 2
        }
        categories+=("$2")
        shift 2
        ;;
    *)
        ids+=("$1")
        shift
        ;;
    esac
done
if [ ${#ids[@]} -eq 0 ] && [ ${#categories[@]} -eq 0 ]; then
    echo "usage: $0 [--category NAME]... [ID]..." >&2
    exit 2
fi

./deploy/pull-db.sh
cargo build -q --release --bin rsdb
export DATABASE_URL="sqlite:$PWD/redstone.db"
rsdb=target/release/rsdb
for category in "${categories[@]}"; do
    while read -r id; do ids+=("$id"); done < <(
        sqlite3 redstone.db "SELECT id FROM schematic WHERE category = '$category' ORDER BY id"
    )
done

count=0
for id in "${ids[@]}"; do
    count=$((count + 1))
    printf '%s/%s  %s\n' "$count" "${#ids[@]}" "$("$rsdb" simulate "$id" 2>&1 | head -1 | cut -c1-100)"
done

stamp=$(date +%Y%m%d-%H%M%S)
results=$(mktemp --suffix=.json)
trap 'rm -f "$results"' EXIT
"$rsdb" export-results "${ids[@]}" >"$results"
remote=/tmp/results-$stamp.json
scp -q "$results" "$host:$remote"
ssh "$host" "set -e
    chmod 644 $remote
    runuser -u redstonedb -- sqlite3 /var/lib/redstonedb/redstone.db \
        '.backup /var/backups/redstonedb/before-results-$stamp.db'
    rsdb import-results $remote
    rm -f $remote"

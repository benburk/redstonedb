set dotenv-load

# Display available recipes
default:
    just --list --unsorted

alias fmt := format

# Format code and docs
format:
    just --fmt
    dprint fmt
    cargo fmt --all
    fd -e nix -X nixfmt
    # The trailing `.` is required: with no path, ripgrep reads stdin when
    # stdin is not a TTY and blocks forever instead of searching the tree.
    rg -l '[^\n]\z' --multiline . | xargs -r sed -i -e '$a\\'

# Run linters and static analysis
check:
    just --fmt --check
    dprint check
    @fd -e md --hidden -E .git -X awk '/^[[:space:]]*[|]/ && length($0) > 80 {print FILENAME ":" FNR ": table row is " length($0) " chars (>80)"; bad=1} END {exit bad}'
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    fd -e nix -X nixfmt --check
    tailwindcss --input styles/app.css --minify | cmp - static/app.css
    ! rg -l '[^\n]\z' --multiline .

# Run the test suite, noting the files it passed on
test:
    cargo test --workspace
    @./deploy/tested.sh mark

# Run the test suite with the tests that measure whole builds, for changes to
# the simulator, probing, or measuring
test-full:
    cargo test --workspace -- --include-ignored
    @./deploy/tested.sh mark

# Build static/app.css from styles/app.css
css:
    tailwindcss --input styles/app.css --minify > static/app.css

# Build release binary
[group('podium')]
build: css
    cargo build --release

# A second line of comment would be all `just --list` shows, and this one is a
# button's tooltip, so it is stated outright.
# The default port is the first in 3000-3050, the band `tailscale serve` maps,
# with nothing listening on loopback or every address. tailscaled listens on
# the tailnet addresses for each mapped port, which is the proxy, not a server.
[doc('Serve until interrupted, on a free port')]
[group('podium')]
serve port=`ss -Hltn | awk '$4 ~ /^(127\.|0\.0\.0\.0:|\*:|\[::1?\]:)/ {sub(/.*:/, "", $4); used[$4]} END {for (p = 3000; p <= 3050; p++) if (!(p in used)) {print p; exit}}'`: css
    cargo run --bin redstonedb -- {{ port }}

# Test, unless the suite passed on exactly these files, then build the
# committed HEAD on the server and restart the site
deploy:
    @./deploy/tested.sh head || just test
    ./deploy/deploy.sh

# Replace the local database with a copy of the live one
pull-db:
    ./deploy/pull-db.sh

# Follow the live site's log
logs:
    ssh root@157.180.85.191 journalctl -u redstonedb -f -o cat

# Show the live site's commit, service state, and latest backups
status:
    ssh root@157.180.85.191 'echo "Live commit: $(cat /usr/local/lib/redstonedb/DEPLOYED)"; systemctl --no-pager status redstonedb | head -5; ls -t /var/backups/redstonedb | head -3'

# Restart the server when source files change
watch:
    watchexec --restart --watch src --watch static --watch styles --watch migrations --ignore static/app.css --exts rs,css,js,sql -- just serve

# Needs a live database; everything else does not.
[doc('Refresh the offline query metadata in .sqlx/ after changing a query')]
prepare:
    SQLX_OFFLINE=false cargo sqlx prepare -- --all-targets

# Positional arguments keep a quoted argument, such as a description, whole.
[doc("Run the rsdb CLI against this repo's database, from any directory")]
[no-cd]
[positional-arguments]
rsdb *args:
    @DATABASE_URL=sqlite:{{ justfile_directory() }}/redstone.db cargo run -q --manifest-path {{ justfile_directory() }}/Cargo.toml --bin rsdb -- "$@"

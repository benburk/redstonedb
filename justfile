set dotenv-load

# Variables are evaluated only when a recipe uses them, so the port lookup
# below runs for `serve` alone.
set lazy

# Where `setup live`, `deploy`, `logs`, and `status` reach the live site, for
# the recipes and the scripts they run. DEPLOY_HOST in .env or the
# environment, as the deploy workflow sets it, wins unless it is empty.
export DEPLOY_HOST := if env("DEPLOY_HOST", "") == "" { "root@redstonedb.com" } else { env("DEPLOY_HOST") }

# Display available recipes
default:
    just --list --unsorted

alias fmt := format

# Set up a fresh checkout to run, seeding a missing database from the test
# fixtures. `just setup live` replaces the database with a copy of the live one
setup data="seed":
    test -e .env || cp .env.example .env
    {{ if data == "seed" { "test -e database.sqlite3 || cargo run -q --bin rsdb -- seed" } else if data == "live" { "./deploy/pull-db.sh" } else { error("setup takes seed or live") } }}

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
build:
    cargo build --release

# The first port in 3000-3050 that nothing answers on at 127.0.0.1, whoever
# runs it: a listener there or on every address stops a server binding it.
free_port := `bash -c 'for p in {3000..3050}; do (exec 3<>/dev/tcp/127.0.0.1/$p) 2>/dev/null || { echo $p; exit; }; done'`

# Serve until interrupted, on a free port
serve port=free_port:
    cargo run --bin redstonedb -- {{ port }}

# Test, unless the suite passed on exactly these files, then build the
# committed HEAD on the server and restart the site
deploy:
    @./deploy/tested.sh head || just test
    ./deploy/deploy.sh

# Follow the live site's log
logs:
    ssh "$DEPLOY_HOST" journalctl -u redstonedb -f -o cat

# Show the live site's commit, service state, and latest backups
status:
    ssh "$DEPLOY_HOST" 'echo "Live commit: $(cat /usr/local/lib/redstonedb/DEPLOYED)"; systemctl --no-pager status redstonedb | head -5; ls -t /var/backups/redstonedb | head -3'

# Restart the server when source files change
watch:
    watchexec --restart --watch src --watch static --watch styles --watch migrations --ignore static/app.css --exts rs,css,js,sql -- just css serve

# Needs a live database; everything else does not.
[doc('Refresh the offline query metadata in .sqlx/ after changing a query')]
prepare:
    SQLX_OFFLINE=false DATABASE_URL=sqlite:database.sqlite3 cargo sqlx prepare -- --all-targets

# Positional arguments keep a quoted argument, such as a description, whole.
[doc("Run the rsdb CLI against this repo's database, from any directory")]
[no-cd]
[positional-arguments]
rsdb *args:
    @DATABASE_URL=sqlite:{{ justfile_directory() }}/database.sqlite3 cargo run -q --manifest-path {{ justfile_directory() }}/Cargo.toml --bin rsdb -- "$@"

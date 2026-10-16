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

# Run the test suite
test:
    cargo test --workspace

# Build static/app.css from styles/app.css
css:
    tailwindcss --input styles/app.css --minify > static/app.css

# Build release binary
[group('podium')]
build: css
    cargo build --release

# A second line of comment would be all `just --list` shows, and this one is a
# button's tooltip, so it is stated outright.
[doc('Serve until interrupted, on a free port')]
[group('podium')]
serve port=`free-port`: css
    PORT={{ port }} cargo run --bin redstone-database

# Test, then build the committed HEAD on the server and restart the site
deploy: test
    ./deploy/deploy.sh

# Replace the local database with a copy of the live one
pull-db:
    ./deploy/pull-db.sh

# Many fixtures are deliberately broken test cases, so only distinct builds
# that keep the conventions once standardized are taken, across most tested
# categories. `rsdb add` simulates each, which takes about a minute.
[doc('Make a database from the test fixtures, for working without the live one')]
seed db="redstone.db":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -e "{{ db }}" ]; then
        echo "{{ db }} exists; remove it first, or seed another: just seed other.db" >&2
        exit 1
    fi
    export DATABASE_URL="sqlite:{{ db }}"
    cargo build -q --bin rsdb
    rsdb() { target/debug/rsdb "$@"; }
    add() {
        local name=$1 orientation=$2
        shift 2
        rsdb add "tests/fixtures/$name.schem" --orientation "$orientation" "$@"
    }
    rsdb define-tag pistonless --kind restriction
    add alpha-rca horz --category adder --design ripple_carry --tag pistonless
    add alpha-cla horz --category adder --design carry_lookahead
    add alpha-rca-slice horz --category adder --design ripple_carry
    add diag-3t-cca diag --category adder --design carry_cancel
    add vert-adder-8-stacks vert --category adder --standardize
    add hex-cca-8 horz --category adder --design carry_cancel --standardize
    add inc-8 horz --category incrementer
    add cout-8 horz --category carry_out --standardize
    add abs-8 horz --category absolute_value --standardize
    add bin2bcd-8 horz --category binary_to_bcd --width 8
    add bcd2bin-10 horz --category bcd_to_binary --width 10
    add mult-8-diag-signed diag --category multiplier --width 8 --standardize
    add alu-8-lines horz --category alu
    add register-8-2-cells horz --category register --standardize
    add alpha-register horz --category register
    add threshold-3-raw horz --category threshold --width 3 --at-least 2 --standardize

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

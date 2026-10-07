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

# Restart the server when source files change
watch:
    watchexec --restart --watch src --watch static --watch styles --ignore static/app.css --exts rs,css,js -- just serve

# Needs a live database; everything else does not.
[doc('Refresh the offline query metadata in .sqlx/ after changing a query')]
prepare:
    SQLX_OFFLINE=false cargo sqlx prepare -- --all-targets

# Run the rsdb CLI against this repo's database, from any directory
[no-cd]
rsdb *args:
    @DATABASE_URL=sqlite:{{ justfile_directory() }}/redstone.db cargo run -q --manifest-path {{ justfile_directory() }}/Cargo.toml --bin rsdb -- {{ args }}

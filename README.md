# RedstoneDB

RedstoneDB is a site for sharing, verifying, and archiving high-quality Minecraft redstone builds, live at <https://redstonedb.com>. It was launched by embizone in October 2026, taking inspiration from and building on many earlier plot-of-logic projects. This repo holds the site and its command-line tool, `rsdb`.

Depending on the circuit, RedstoneDB tests every build automatically for:

- build type: adder, register, ALU, and so on
- correctness, across every input case, or every case of each byte for wide builds
- latency, in ticks
- throughput: how often it can take a new input
- whether its outputs change in sync
- orientation: horizontal, vertical, or diagonal
- stackability, and how it ranks stacked to 8, 16, 32, and 64 bits
- the weakest signal strength its inputs need, and the weakest its outputs give
- the oldest Minecraft version it works in

All of this is worked out mechanically, in MCHPRS's Redpiler, so it can't drift out of date with a description, and an uploader can't enter incorrect information. Even an adder's design, like ripple carry or carry lookahead, is read from the circuit.

Recognized so far: adders, hex adders, subtractors, ALUs, multipliers, incrementers, decrementers, comparators, carry outs, absolute values, negators, popcounts, thresholds, parity circuits, binary-to-hex and hex-to-binary converters, gates, inverters, decoders, barrel shifters, monostables, registers, buffers, counters, shift registers, and T flip-flops. Anything else is still archived, as built, under Other.

## How builds are recognized

Each type of build has an input and output contract. RedstoneDB finds a build's inputs and outputs, then tries its lines in different roles and orders until they fit a type's truth table. No particular colours or signs are needed: once it knows what each line does, it keeps that with the build.

- **Outputs** are repeaters or comparators that power nothing: the block in front of each is air.
- **Inputs** are levers on the side of a solid block, with room for a repeater there so a bus could drive it. A hex-digit input is a comparator with air behind it.
- **Buses** are evenly spaced lines, straight or diagonal, or two lines with their bits alternating.
- **Stacking** is checked by `//rstack`ing copies west or up, 1, 2, or 4 blocks a bit apart, and testing the result.

The full rules, with pictures, are on the site: [How builds are recognized](https://redstonedb.com/about/recognized). A build that doesn't fit still goes up, untested, with a note on what's in the way.

## Docs

- [docs/conventions.md](docs/conventions.md): what a build needs, what the site does with it, and stacking.
- [docs/glossary.md](docs/glossary.md): categories, designs, fields, and their abbreviations.
- [docs/naming.md](docs/naming.md): how a build is named, and its download filename.

## Development

Running the site needs Rust, [`just`](https://just.systems), and a C compiler: `build-essential` on Debian or Ubuntu, the Xcode command-line tools on macOS. `rust-toolchain.toml` pins Rust, so [rustup](https://rustup.rs) installs the right version on the first build. SQLite is compiled in, and TLS needs no OpenSSL. `nix develop` gives all of it, along with the tools `just check` uses.

```sh
just setup
just serve 3001
```

`just setup` writes `.env` from `.env.example` and, when there is no `database.sqlite3`, fills a new one from the test fixtures with `rsdb seed`. With no port given, `just serve` picks the first port from 3000 that nothing answers on.

A fresh checkout needs no `.env`: the development settings, such as the database's path and Discord login's public ID, are in `.cargo/config.toml`, which Cargo gives to `cargo run` and `cargo test` and never to the deployed binary. `.env` holds only what is your own, such as the Discord secret, and any setting there wins over the committed one. The recipes that reach the live site, `setup live`, `deploy`, `logs`, and `status`, go to `root@redstonedb.com` unless `DEPLOY_HOST` says otherwise.

`just check` also needs `dprint`, `fd`, `ripgrep`, `nixfmt`, and Tailwind CSS v4, which `nix develop` provides.

A development build signs every request in as `DEV_USER`, the admin account `debug` that `rsdb seed` makes; set `DEV_USER` in `.env` to another account's name, or empty to see the site signed out. `/dev/login` signs in as `debug` too, with the role asked for, such as `http://localhost:3001/dev/login?role=user&next=/upload`; only debug builds have that route. Discord login needs `DISCORD_CLIENT_SECRET` in `.env`, and redirects to port 3001, so serve there to sign in with it. On the live site, the first admin is made from the command line, after they have logged in once:

```sh
just rsdb users
just rsdb users --role <discord username> admin
```

`database.sqlite3` is not in git. The server and `rsdb` create it if it is missing and apply pending migrations from `migrations/` when they open it. `rsdb seed` fills a new one with every build in `tests/fixtures`, published as uploads with the details the upload page suggests, each simulated and stored whole as it goes in, as an upload is; it refuses a database that has builds. `just setup live` replaces it with a copy of the live database, backing up the local one to `backups/` first; it needs SSH access to the server. The copy leaves out live sign-ins and the Schemati sessions people gave.

| Recipe            | What it does                                            |
| ----------------- | ------------------------------------------------------- |
| `just setup`      | Write `.env`, and seed the database, if either is gone  |
| `just serve`      | Run the site, on a free port or the one given           |
| `just watch`      | Run the site, restarting when a source file changes     |
| `just rsdb ...`   | Run `rsdb` against this repo's `database.sqlite3`       |
| `just setup live` | Replace the database with a copy of the live one        |
| `just test`       | Run the test suite                                      |
| `just check`      | Lint: formatting, clippy, nixfmt, and a stale `app.css` |
| `just format`     | Format the code and docs                                |
| `just css`        | Build `static/app.css` from `styles/app.css`            |
| `just prepare`    | Refresh the offline query metadata in `.sqlx/`          |
| `just deploy`     | Test, then build and restart the live site              |

Every pull request runs `just check` and `just test` in the flake's shell, side by side, from `.github/workflows/check.yml`, which also runs on main to keep the build cache pull requests start from. Pushing a `v*` tag on main deploys it too: `.github/workflows/deploy.yml` runs the suite, then `deploy/deploy.sh`, with the key and host as secrets of the repo's production environment.

`static/app.css` is committed, so `just serve`, `just build`, and the Nix build need no Tailwind; `just watch` rebuilds it as `styles/app.css` changes. Queries compile against `.sqlx/`, as `.cargo/config.toml` sets `SQLX_OFFLINE=true`, so after changing one, run `just prepare`.

## rsdb

`rsdb` does from the command line what the site does as an admin, and a little more. Ids can be shortened to any unique prefix; `rsdb --help` lists every command.

```console
$ just rsdb inspect build.schem
build.schem: 0 errors; inputs 8 8 1; outputs 8 1
$ just rsdb search 8 bit adder -carry_cancel
0f356d03  1_bit_Ripple_Carry_Adder.schem
bb624bfe  8_bit_Ripple_Carry_Adder.schem
9879770a  8_bit_Carry_Lookahead_Adder.schem
```

The 1-bit slice matches `8 bit` because eight stacked copies of it make an 8-bit adder. The site's search box takes the same words.

```sh
just rsdb show bb62
just rsdb set bb62 --contributor Owenage_ --tag pistonless
just rsdb export bb62 --out ~/.minecraft/config/worldedit/schematics
just rsdb new-version bb62 fixed.schem --note "carry out reads through a repeater"
just rsdb refresh $(just rsdb search --ids adder vertical)
just rsdb refresh --all --prune --dry-run
just rsdb inspect bb62
```

- `refresh` brings what is worked out from builds' files up to date, or every build's with `--all`: it reads each file again, judges the build again as its category, runs its test and timing, and keeps what its building blocks must be, for themes. Nothing reruns on its own after the simulator changes. `--prune` also takes out blocks no part of a build's circuit uses, proving with Redpiler that the circuit is unchanged; `--dry-run` changes nothing, working builds out from their files alone and listing each field that differs from what is stored.
- `set` changes a build's details and tags, and `new-version` replaces its file with a fix, as the site's edit page does, keeping the old one, or with `--in-place` fixes the file itself; each then refreshes the build, so nothing it leaves goes stale.
- `inspect` checks a file or build by the rules for inputs and outputs, lists them in buses, and says what the site recognizes it as.

On the server, `deploy/rsdb` runs `rsdb` against the live database as the site's user.

## How it works

1. **Ingest.** `ingest/` takes a Sponge `.schem` file in: it reads it (`schem/`), lays it the standard way and fits its box (`standard/`), holds its details to the rules, and stores it. Files live only in `build_version`; a later file becomes a new version only if `compare/` finds it the same build, fixed.
2. **Recognition.** `interface/` finds a build's inputs and outputs and checks each one, and `recognize/probe/` finds each line's role by trying the categories' tests. Roles are stored with the version; the file keeps its author's colours. `works_from.rs` finds the oldest release the build works in.
3. **Simulation.** `category/` holds each category's facts and its test. `simulate/` is the engine: it runs the build in Redpiler through a test's cases, times it, and finds how its copies stack, joined as `//rstack` joins them (`stack/`). `ingest/` runs these once a build is stored, and each result is kept with its file's hash.
4. **Search, leaderboards, and similarity.** `search.rs` parses queries like `register vertical -torchless`, `leaderboard.rs` ranks builds and finds records, and `compare/` scores how alike two builds are by the share of their redstone components that line up.

Every module but `web/` and the binaries is a layer, in the order below, each using only those above it in the list; `tests/layers.rs` holds them to it.

```text
src/
  schem/         reading, writing, and editing .schem files; the bottom layer
  interface/     a build's inputs and outputs, and the rules for them
  stack/         joining copies of a build as //rstack does
  simulate/      the engine: cases, timing, throughput, and stacking in Redpiler
  category/      the standard categories, each with its facts and its test
  standard/      laying a file the standard way, in the blocks the site keeps
  recognize/     what a build is: the roles of its lines, by probing
  works_from.rs  the oldest release a file's blocks work in
  compare/       how alike two files are: versions, diffs, and similar builds
  thumbnail.rs   a build drawn small, for cards and link previews
  build/         what a stored build is: its specs, credits, status, and name
  account.rs     who is signed in, and what they may do
  search.rs      finding builds by words
  leaderboard.rs ranking builds, and the records they hold
  external/      Discord, Mojang, player heads, and Schemati
  store/         every query, one module per table or group of tables
  players.rs     who the players behind names are, asking Mojang once
  ingest/        the path from a file to a stored build with its results
  web/           the site: routes, then a folder for each part, holding its handlers and markup
  main.rs        the web server
  bin/rsdb/      the command-line tool
static/          fixi, page scripts, and the built app.css
migrations/      the schema, applied in order at startup
tests/           integration tests, with .schem fixtures
deploy/          the server's units, Caddyfile, and scripts
```

The site is axum, maud, and sqlx over SQLite. Pages are server-rendered HTML with forms, using [fixi](https://github.com/bigskysoftware/fixi) for partial swaps. The 3D viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer).

## License

MIT.

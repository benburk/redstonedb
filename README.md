# redstone-database

A database of Minecraft redstone schematics, with a website and a command-line tool, `rsdb`. Each build is checked against one set of building conventions, then run in a redstone simulator, so its width, latency, and whether it computes the right answer are measured from its file rather than typed in. The site is live at <https://redstonedb.benburk.ca>.

A folder of `.schem` files on Schemati or a Discord channel holds the same builds, but nothing there says whether an adder adds, how fast it is, or which stored build it resembles. Here the simulator answers the first two, and a similarity search answers the third.

## Docs

- [conventions.md](conventions.md): how builds are laid out in-world, the block palette, signs, and stacking.
- [glossary.md](glossary.md): categories, designs, fields, and their abbreviations.
- [naming.md](naming.md): how a build is named, and its download filename.
- [features.md](features.md): planned checks, export options, and fields not yet stored.
- [RATIONALE.md](RATIONALE.md): why server-rendered HTML, SQLite, and Rust.
- [deploy/README.md](deploy/README.md): the live server, deploying, and backups.

Each part of the conventions and naming is marked either decided or open.

## Setup

The dev shell has everything but `free-port`, which `just serve` uses to pick a port when none is given:

```sh
nix develop
cp .env.example .env
just seed
just serve 3001
```

`just seed` makes `redstone.db` from the test fixtures: 17 builds in eleven categories, each imported with `rsdb add`, which simulates it, so it takes a minute or two. It refuses to overwrite a database that exists; `just seed other.db` writes elsewhere. `just pull-db` instead replaces `redstone.db` with a copy of the live database, backing up the local one to `backups/` first, and needs SSH access to the server.

Discord login needs the three `DISCORD_` settings in `.env`; see `.env.example`. The client secret comes from the Discord Developer Portal, and the redirect, port 3001 in the example, must be listed there too. Without them the site runs with login turned off. The first admin is made from the command line, after they have logged in once:

```sh
just rsdb users
just rsdb role <discord username> admin
```

`redstone.db` is not in git. The server and `rsdb` create it if it is missing and apply pending migrations from `migrations/` when they open it. `rsdb` checkpoints the write-ahead log before it exits, so copying `redstone.db` alone is a complete backup.

## Recipes

| Recipe          | What it does                                            |
| --------------- | ------------------------------------------------------- |
| `just serve`    | Run the site, on a free port or the one given           |
| `just watch`    | Run the site, restarting when a source file changes     |
| `just rsdb ...` | Run `rsdb` against this repo's `redstone.db`            |
| `just seed`     | Make a database from the test fixtures                  |
| `just pull-db`  | Replace the local database with a copy of the live one  |
| `just test`     | Run the test suite                                      |
| `just check`    | Lint: formatting, clippy, nixfmt, and a stale `app.css` |
| `just format`   | Format the code and docs                                |
| `just css`      | Build `static/app.css` from `styles/app.css`            |
| `just prepare`  | Refresh the offline query metadata in `.sqlx/`          |
| `just deploy`   | Test, then build and restart the live site              |

`static/app.css` is committed, so the server and the Nix build need no Tailwind; `just serve` rebuilds it, and `just check` fails when it is stale. Queries compile against `.sqlx/` with `SQLX_OFFLINE=true`, so after changing one, run `just prepare`, which needs a database.

## rsdb

`rsdb` does from the command line what the site does, as an admin, and a little more. Ids can be shortened to any unique prefix. `rsdb --help` and `rsdb <command> --help` list every command and flag.

```console
$ just rsdb add tests/fixtures/alpha-rca.schem --category adder --orientation horz --design ripple_carry
bb624bfe  8_bit_Ripple_Carry_Adder.schem
ok      bb624bfe  8_bit_Ripple_Carry_Adder.schem: every 131072 A + B + carry in, all right
        doesn't stack 16 west apart: The carry does not pass between copies: each keeps its own carry in, which lands past the copy before.
        timing: A at 0, B at 0, carry in at +3; output by 20t, carry out by 19t; not synced
        ...
$ just rsdb search 8 bit adder -carry_cancel
0f356d03  1_bit_Ripple_Carry_Adder.schem
bb624bfe  8_bit_Ripple_Carry_Adder.schem
9879770a  8_bit_Carry_Lookahead_Adder.schem
cce8e000  8_bit_Adder.schem
```

The 1-bit slice matches `8 bit` because eight stacked copies of it make an 8-bit adder. The site's search box takes the same words.

`add` refuses a file that breaks a building convention, listing each, unless given `--force`. `--standardize` brings a downloaded build into the palette first, and `--conventional false` marks a build, such as a display, that follows its own layout.

```sh
just rsdb add build.schem --category adder --orientation vert --standardize \
    --builder cullback --built-on 2026-10-07 --tag pistonless
just rsdb list --category adder --tag pistonless
just rsdb show bb62
just rsdb set bb62 --design carry_cancel --contributor Owenage_
just rsdb set bb62 --unset built-on
just rsdb tag bb62 pistonless
just rsdb export bb62 --out ~/.minecraft/config/worldedit/schematics
just rsdb export 0f35 --copies 8 --markers
just rsdb new-version bb62 fixed.schem --note "carry out reads through a repeater"
just rsdb lint build.schem
just rsdb simulate
just rsdb reread
just rsdb remove bb62
```

- `set` changes only the fields given. A list given, such as `--builder` or `--stack`, replaces the stored one, and `--unset` makes a field unknown.
- `export --copies` joins copies of a stackable build into one file: a count, or one per direction. `--markers` puts a glass block off each corner for WorldEdit.
- `new-version` replaces a build's file with a fix, as the site's edit page does; `replace` puts a file in place without adding a version or comparing.
- `simulate` runs a build's test and timing again, or every build's; `reread` reads every stored file again. Run them after the simulator or the import rules change.
- `categories` lists the standard categories with their measures, designs, flags, and aliases; `define-category`, `define-design`, `define-flag`, and `define-alias` add to them. `tags` and `define-tag` do the same for tags, which must exist before use.
- `respell` asks Mojang how each stored Minecraft name is spelled.

On the server, `rsdb` runs against the live database as the site's user; see [deploy/README.md](deploy/README.md).

## How it works

1. **Import.** `import::prepare` reads a Sponge `.schem` file, versions 2 and 3 (`schem.rs`), standardizes it if asked, takes out corner markers, and fits its box (`standardize.rs`), then sets where `//paste` puts it, the same for every file. `import::store` keeps it as the build's first version; files live only in `schematic_version` (`models/version.rs`). A later file replaces it as a new version only if `version.rs` finds it the same build, fixed.
2. **Analysis.** `interface.rs` reads the inputs and outputs from block colours and signs, `lint.rs` checks them against [conventions.md](conventions.md), and `works_from.rs` finds the oldest release the build works in. `details.rs` holds a build's details to one set of rules, whether they come from the upload page, the edit page, or `rsdb`.
3. **Simulation.** `category.rs` says which test each category gets and what is read from its file. `simulate/` runs the build in MCHPRS's Redpiler through its cases, times it, and for an adder or register finds how its copies stack, joined by `stack.rs` as `//rstack` joins them in game. `models/simulation.rs` stores each result with its file's hash and its test's version, and reruns it in the background when either changes.
4. **Search, leaderboards, and similarity.** `search.rs` parses a query such as `register vertical -torchless`, `leaderboard.rs` ranks builds against those they compete with, and `similarity.rs` scores how alike two builds are by the share of their redstone components that line up.

## Project structure

```text
src/
  main.rs           the web server
  bin/rsdb.rs       the command-line tool
  db.rs             opening the database and applying migrations
  schem.rs          reading and writing .schem files
  standardize.rs    the palette, box fitting, and corner markers
  import.rs         a file into a stored build
  interface.rs      inputs and outputs, from blocks and signs
  lint.rs           the building conventions check
  works_from.rs     the oldest release a build works in
  details.rs        the rules a build's details keep
  category.rs       what the code does with each category
  simulate.rs       the simulator's entry point; tests in simulate/
  stack.rs          copies of a build joined into one
  version.rs        whether a new file is the same build
  similarity.rs     how alike two builds are
  search.rs         the search query
  leaderboard.rs    rankings and records
  thumbnail.rs      a build's card picture
  head.rs           a player's head from their skin
  mojang.rs         Minecraft player lookups
  discord.rs        Discord login
  schemati.rs       the Open Redstone Engineers schematic store
  models/           queries, one module per table or group of tables
  web/              routes and handlers
  web/pages/        each page's markup, with shared pieces in web/pages.rs
static/             fixi, page scripts, and the built app.css
styles/app.css      the Tailwind source
migrations/         the schema, applied in order at startup
tests/              integration tests, with .schem fixtures
deploy/             the server's units, Caddyfile, and scripts
```

The site is axum, maud, and sqlx over SQLite. Pages are server-rendered HTML with forms; [fixi](https://github.com/bigskysoftware/fixi), vendored as `static/fixi.js`, swaps in a fragment where a form changes only part of a page, and `static/app.js` holds its policies for every page. The 3D viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer), loaded from jsDelivr with its resource pack.

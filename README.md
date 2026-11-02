# redstonedb

A database of Minecraft redstone schematics, with a website and a command-line tool, `rsdb`. Each upload is probed to find what it is, by its levers and outputs alone, then run in a redstone simulator, so its width, latency, and whether it computes the right answer are measured from its file rather than typed in. Adders, ALUs, multipliers, incrementers, decrementers, comparators, carry outs, absolute values, popcounts, thresholds, binary-to-hex converters, gates, inverters, decoders, barrel shifters, monostables, registers, buffers, counters, shift registers, and T flip-flops are recognized so far; any other build goes up as built, off the list. The site is live at <https://redstonedb.benburk.ca>.

A folder of `.schem` files on Schemati or a Discord channel holds the same builds, but nothing there says whether an adder adds, how fast it is, or which stored build it resembles. Here the simulator answers the first two, and a similarity search answers the third.

## Docs

- [conventions.md](conventions.md): what a build needs, what the site does with it, and stacking.
- [glossary.md](glossary.md): categories, designs, fields, and their abbreviations.
- [naming.md](naming.md): how a build is named, and its download filename.
- [RATIONALE.md](RATIONALE.md): why server-rendered HTML, SQLite, and Rust.

## Setup

The dev shell has everything `just serve` needs. Discord login redirects to port 3001, so serve there to sign in:

```sh
nix develop
cp .env.example .env
just serve 3001
```

`just pull-db` replaces `redstone.db` with a copy of the live database, backing up the local one to `backups/` first, and needs SSH access to the server.

Discord login needs the three `DISCORD_` settings in `.env`; see `.env.example`. The client secret comes from the Discord Developer Portal, and the redirect, port 3001 in the example, must be listed there too. Without them the site runs with login turned off. The first admin is made from the command line, after they have logged in once:

```sh
just rsdb users
just rsdb role <discord username> admin
```

To try the site signed in without Discord, open `/dev/login` on a local server, such as `http://localhost:3001/dev/login?next=/upload`. It signs in as the account `debug`, made the first time, as an admin, or with `role=user` or `role=mod` as either. Only a debug build has the route, as `just serve` runs; the release build the live site runs answers it with 404.

`redstone.db` is not in git. The server and `rsdb` create it if it is missing and apply pending migrations from `migrations/` when they open it. `rsdb` checkpoints the write-ahead log before it exits, so copying `redstone.db` alone is a complete backup.

## Recipes

| Recipe          | What it does                                            |
| --------------- | ------------------------------------------------------- |
| `just serve`    | Run the site, on a free port or the one given           |
| `just watch`    | Run the site, restarting when a source file changes     |
| `just rsdb ...` | Run `rsdb` against this repo's `redstone.db`            |
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
$ just rsdb inspect build.schem
build.schem: 0 errors; inputs 8 8 1; outputs 8 1
  note: 16 of 17 inputs drive dust, so a bus drives them through repeaters
$ just rsdb search 8 bit adder -carry_cancel
0f356d03  1_bit_Ripple_Carry_Adder.schem
bb624bfe  8_bit_Ripple_Carry_Adder.schem
9879770a  8_bit_Carry_Lookahead_Adder.schem
cce8e000  8_bit_Adder.schem
```

The 1-bit slice matches `8 bit` because eight stacked copies of it make an 8-bit adder. The site's search box takes the same words.

Builds come in through the upload page. `rebuild` takes a category's builds from another copy of the database, each probed and standardized, keeping its id and credits.

```sh
just rsdb list --category adder --tag pistonless
just rsdb show bb62
just rsdb set bb62 --contributor Owenage_
just rsdb set bb62 --unset built-on
just rsdb tag bb62 pistonless
just rsdb export bb62 --out ~/.minecraft/config/worldedit/schematics
just rsdb export 0f35 --markers
just rsdb new-version bb62 fixed.schem --note "carry out reads through a repeater"
just rsdb simulate $(just rsdb search --ids adder vertical)
just rsdb prune --dry-run
just rsdb evaluate --category adder
just rsdb reread
just rsdb remove bb62
```

- `set` changes only the fields given. A list given, such as `--builder` or `--stack`, replaces the stored one, and `--unset` makes a field unknown.
- `export --copies` joins copies of a stackable build into one file: a count, or one per direction. `--markers` puts glass at two opposite corners of the box `//rstack` stacks, to select in WorldEdit and then break; it takes one copy.
- `new-version` replaces a build's file with a fix, as the site's edit page does; `replace` puts a file in place without adding a version or comparing.
- `simulate` runs the named builds' tests and timing again, four at once, or every build's with `--all`; `search --ids` lists the builds a change touches. Nothing reruns on its own, so run them after the simulator changes. `reread` reads every stored file again, after the import rules change.
- `evaluate` works every build of a category, or one build, out from its file alone, as `src/recognize/evaluate.rs` does, trusting nothing stored: the category its test recognizes, whether every case comes out right, its width, orientation, and flags, how it stacks, its latency, whether its outputs change together, and its throughput, the fewest ticks between inputs that keep every result right. It lists each field where that differs from what is stored.
- `prune` takes out the blocks no part of a build's circuit uses, found by the rules in `src/recognize/roles.rs`: a block holding nothing up that passes no power, cuts no step between dust, and lets no dust on it power dust a step down, and dust that powers nothing a component reads, from its ends inwards, each read alone and stacked every way the build stacks. It works on one build or every build of a category the site tests, each as a new version noted `Unused blocks taken out`. Each is proven the same circuit first: Redpiler compiles it before and after, alone and stacked, and every link between components and the strength it loses must match, so the build's stored test result and timing move to the new file rather than being run again. The whole site takes seconds. `--dry-run` lists what it would take out.
- `categories` lists the standard categories with their measures, designs, flags, and aliases; `define-category`, `define-design`, `define-flag`, and `define-alias` add to them. `tags` and `define-tag` do the same for tags, which must exist before use.
- `respell` asks Mojang how each stored Minecraft name is spelled.

On the server, `deploy/rsdb` runs `rsdb` against the live database as the site's user.

## How it works

1. **Import.** `import::prepare` reads a Sponge `.schem` file, versions 2 and 3 (`schem/`), standardizes it if asked, takes out corner markers, and fits its box (`standardize.rs`), then sets where `//paste` puts it, the same for every file. `import::store` keeps it as the build's first version; files live only in `schematic_version` (`models/version.rs`). A later file replaces it as a new version only if `version.rs` finds it the same build, fixed.
2. **Analysis.** `inspect.rs` checks each input and output by what it does, `probe/` finds each line's role by trying the blessed categories' tests and colours the build by them, `interface.rs` reads the coloured lines back, and `works_from.rs` finds the oldest release the build works in. `details.rs` holds a build's details to one set of rules, whether they come from the upload page, the edit page, or `rsdb`.
3. **Simulation.** `category.rs` says which test each category gets and what is read from its file. `simulate/` runs the build in MCHPRS's Redpiler through its cases, times it, and for an adder or register finds how its copies stack, joined by `stack.rs` as `//rstack` joins them in game. `models/simulation.rs` stores each result with its file's hash, and runs a build whose file has none, as a new upload or version. A change to a test reruns nothing on its own: `rsdb search --ids` lists the builds it touches, and `rsdb simulate` runs those again, four at once, or every build with `--all`.
4. **Search, leaderboards, and similarity.** `search.rs` parses a query such as `register vertical -torchless`, `leaderboard.rs` ranks builds against those they compete with, copies of a stackable build stacked to each wider standard width among them, and `similarity.rs` scores how alike two builds are by the share of their redstone components that line up.

## Project structure

```text
src/
  main.rs           the web server
  bin/rsdb/         the command-line tool
  db.rs             opening the database and applying migrations
  schem/            reading and writing .schem files
    standardize.rs  the palette and box fitting
    stack.rs        copies of a build joined into one
    works_from.rs   the oldest release a build works in
  import.rs         a file into a stored build
  recognize/        what a build is
    inspect.rs      the check of inputs and outputs by function
    probe/          recognizing a build by trying its lines
    interface.rs    inputs and outputs, from blocks and signs
    colour.rs       the colours a recognized build's roles take
    palette.rs      a viewer's block theme
    roles.rs        what each block of a build is for
    category.rs     what the code does with each category
    evaluate.rs     a build worked out from its file alone
  details.rs        the rules a build's details keep
  simulate.rs       the simulator's entry point; tests in simulate/
  version.rs        whether a new file is the same build
  similarity.rs     how alike two builds are
  search.rs         the search query
  leaderboard.rs    rankings and records
  thumbnail.rs      a build's card picture
  external/         the services the site asks
    head.rs         a player's head from their skin
    mojang.rs       Minecraft player lookups
    discord.rs      Discord login
    schemati.rs     the Open Redstone Engineers schematic store
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

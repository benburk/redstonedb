# RedstoneDB

RedstoneDB is a site for sharing, verifying, and archiving high-quality Minecraft redstone builds, live at <https://redstonedb.com>. It was launched by embizone in October 2026, taking inspiration from and building on many earlier plot-of-logic projects. This repo holds the site and its command-line tool, `rsdb`.

Depending on the circuit, RedstoneDB tests every build automatically for:

- build type: adder, register, ALU, and so on
- correctness, across every input case, or every case of each byte for wide builds
- latency, in ticks
- throughput: how often it can take a new input
- whether its outputs change in sync
- orientation: horizontal, vertical, or diagonal
- stackability, and how it ranks stacked to 8, 16, and 32 bits
- the weakest signal strength its inputs need, and the weakest its outputs give
- the oldest Minecraft version it works in

All of this is worked out mechanically, in MCHPRS's Redpiler, so it can't drift out of date with a description, and an uploader can't enter incorrect information. Even an adder's design, like ripple carry or carry lookahead, is read from the circuit.

Recognized so far: adders, hex adders, subtractors, ALUs, multipliers, incrementers, decrementers, comparators, carry outs, absolute values, popcounts, thresholds, parity circuits, binary-to-hex converters, gates, inverters, decoders, barrel shifters, monostables, registers, buffers, counters, shift registers, and T flip-flops. Anything else is still archived, as built, under Other.

## How builds are recognized

Each type of build has an input and output contract. RedstoneDB finds a build's inputs and outputs, then tries its lines in different roles and orders until they fit a type's truth table. No particular colours or signs are needed: once it knows what each line does, it keeps that with the build.

- **Outputs** are repeaters or comparators that power nothing: the block in front of each is air.
- **Inputs** are levers on the side of a solid block, with room for a repeater there so a bus could drive it. A hex-digit input is a comparator with air behind it.
- **Buses** are evenly spaced lines, straight or diagonal, or two lines with their bits alternating.
- **Stacking** is checked by `//rstack`ing copies west or up, 1, 2, or 4 blocks a bit apart, and testing the result.

The full rules, with pictures, are on the site: [How builds are recognized](https://redstonedb.com/about/recognized). A build that doesn't fit still goes up, unverified, with a note on what's in the way.

## Docs

- [docs/conventions.md](docs/conventions.md): what a build needs, what the site does with it, and stacking.
- [docs/glossary.md](docs/glossary.md): categories, designs, fields, and their abbreviations.
- [docs/naming.md](docs/naming.md): how a build is named, and its download filename.

## Development

The dev shell has everything `just serve` needs. Discord login redirects to port 3001, so serve there to sign in:

```sh
nix develop
cp .env.example .env
just serve 3001
```

Discord login needs the three `DISCORD_` settings in `.env`; see `.env.example`. Without them the site runs with login turned off. To try it signed in without Discord, open `/dev/login` on a local server, such as `http://localhost:3001/dev/login?next=/upload`; only debug builds have that route. The first admin is made from the command line, after they have logged in once:

```sh
just rsdb users
just rsdb role <discord username> admin
```

`redstone.db` is not in git. The server and `rsdb` create it if it is missing and apply pending migrations from `migrations/` when they open it. `just pull-db` replaces it with a copy of the live database, backing up the local one to `backups/` first; it needs SSH access to the server.

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

`static/app.css` is committed, so the server and the Nix build need no Tailwind. Queries compile against `.sqlx/` with `SQLX_OFFLINE=true`, so after changing one, run `just prepare`.

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
just rsdb set bb62 --contributor Owenage_
just rsdb tag bb62 pistonless
just rsdb export bb62 --out ~/.minecraft/config/worldedit/schematics
just rsdb new-version bb62 fixed.schem --note "carry out reads through a repeater"
just rsdb simulate $(just rsdb search --ids adder vertical)
just rsdb evaluate --category adder
just rsdb prune --dry-run
```

- `new-version` replaces a build's file with a fix, as the site's edit page does, keeping the old one.
- `simulate` runs builds' tests and timing again, or every build's with `--all`. Nothing reruns on its own after the simulator changes.
- `evaluate` works builds out from their files alone, trusting nothing stored, and lists each field that differs.
- `prune` takes out blocks no part of a build's circuit uses, proving with Redpiler that the circuit is unchanged.
- `outputs`, `convert`, and `stack-check` read a build's interface, port it to repeater outputs, and check how it stacks.

On the server, `deploy/rsdb` runs `rsdb` against the live database as the site's user.

## How it works

1. **Import.** `import.rs` reads a Sponge `.schem` file (`schem/`), takes out corner markers, and fits its box. Files live only in `schematic_version`; a later file becomes a new version only if `version.rs` finds it the same build, fixed.
2. **Recognition.** `recognize/repeater_outputs.rs` finds the outputs, `recognize/inspect.rs` checks each input and output, and `recognize/probe/` finds each line's role by trying the categories' tests. Roles are stored with the version; the file keeps its builder's colours. `schem/works_from.rs` finds the oldest release the build works in.
3. **Simulation.** `category/table.rs` says which test each category gets. `simulate/` runs the build in Redpiler through its cases, times it, and finds how its copies stack, joined as `//rstack` joins them (`schem/rstack.rs`). Each result is stored with its file's hash.
4. **Search, leaderboards, and similarity.** `search.rs` parses queries like `register vertical -torchless`, `leaderboard.rs` ranks builds and finds records, and `similarity.rs` scores how alike two builds are by the share of their redstone components that line up.

```text
src/
  main.rs        the web server
  bin/rsdb/      the command-line tool
  schem/         reading, writing, standardizing, and stacking .schem files
  recognize/     what a build is: its inputs, outputs, and roles
  category/      the standard categories and what each is tested on
  simulate/      tests, timing, throughput, and stacking in Redpiler
  models/        queries, one module per table or group of tables
  web/           routes, handlers, and each page's markup
  external/      Discord, Mojang, player heads, and Schemati
static/          fixi, page scripts, and the built app.css
migrations/      the schema, applied in order at startup
tests/           integration tests, with .schem fixtures
deploy/          the server's units, Caddyfile, and scripts
```

The site is axum, maud, and sqlx over SQLite. Pages are server-rendered HTML with forms, using [fixi](https://github.com/bigskysoftware/fixi) for partial swaps. The 3D viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer).

## License

MIT.

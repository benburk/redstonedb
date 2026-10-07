# Planned features

None of these are built yet.

## Block whitelist

`rsdb add` rejects a schematic that contains a block outside the whitelist. The whitelist starts as the block palette in [conventions.md](conventions.md) plus the redstone components themselves, and grows as needed. A Sponge schematic lists every block state it uses in its palette, so the check needs only that list, not a scan of every block.

## Linter

`rsdb lint` checks a schematic against [conventions.md](conventions.md) and reports each violation with its position. It runs on `add` and on demand. Candidate checks:

- Every block is whitelisted.
- Signs appear only on the A and B LSBs, control lines, and the output LSB.
- Every lever is saved off, except on inverted inputs. A lever's block state records whether it is on, so the check reads it from the palette.
- Each role's block appears only in that role, for example no white concrete that is hard-powered.
- Every lever sits on an input or control block, and every such block has a lever.
- A and B each have exactly one `lsb` sign, and the output has one with a latency.
- Every control input has a sign naming it.
- Every output bit is a redstone lamp.
- The bounding box matches the stored size.

Most role checks need a model of which blocks are powered, so they come after the whitelist and position checks.

## Import extraction

`rsdb add` reads facts from the schematic instead of asking for them:

- Control lines: find each magenta block, its lever, and its sign, and record the line's name, position, and whether it is inverted.
- Inverted inputs: any lever saved on.
- Input and output positions and widths: lime, green, light blue, cyan, and magenta blocks, and lamps. The shade of an input also gives whether it is soft- or hard-powered.
- The LSB of each bus, from its `lsb` sign.
- Latency, from the output LSB sign.

Sponge schematics store sign text in their block entities, so no game is needed to read it. The sign format changed in Minecraft 1.20, so the reader must handle both the old and new layouts.

## Export options

`rsdb export` rewrites the schematic before writing it:

- Recolour each role to a user's palette, such as `--palette mine.toml`.
- Keep or strip the signs, or rewrite them from the database.

Because roles are encoded by block, this is a palette substitution, not a change to the build.

## Open

- Where user palettes live: a file passed on the command line, or stored in the database.
- Whether the linter blocks `add` or only warns.

# Planned features

None of these are built yet.

## Block whitelist

`rsdb add` rejects a schematic that contains a block outside the whitelist. The whitelist starts as the block palette in [conventions.md](conventions.md) plus the redstone components themselves, and grows as needed. A Sponge schematic lists every block state it uses in its palette, so the check needs only that list, not a scan of every block.

## Linter

`rsdb lint` checks a schematic file, or a stored schematic by id, against [conventions.md](conventions.md) and reports each violation with its position. `rsdb add` runs it too, and refuses a schematic that breaks a convention unless given `--force`.

Built so far:

- Glass fills all 32 corner marker positions.
- Every block is in the palette or is a redstone component. The whitelist is `PALETTE` and `COMPONENTS` in `src/lint.rs`.
- Every lever sits on a lime, light blue, or magenta block, and every such block has exactly one lever.
- Every A and B lever is on the south side of its block.
- No lever touches a redstone component other than through its own block.
- Every control line has a sign naming it.
- A and B each have exactly one `lsb` sign.
- Every output is a redstone lamp, and a multi-bit output has an `lsb` sign.

Still to build:

- Signs appear only on the A and B LSBs, control lines, and the output LSB.
- Every lever is saved off, except on inverted inputs.
- Each role's block appears only in that role, for example no white concrete that is hard-powered.

Most role checks need a model of which blocks are powered, so they come after the whitelist and position checks.

## Import extraction

`rsdb add` reads facts from the schematic instead of asking for them:

- Control lines: find each magenta block, its lever, and its sign, and record the line's name, position, and whether it is inverted.
- Inverted inputs: any lever saved on.
- Input and output positions and widths: lime, light blue, and magenta blocks, and lamps.
- How each input must be driven: hard-powered when dust touches its block, soft-powered otherwise, at the strength its sign gives or 1.
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

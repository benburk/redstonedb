# RedstoneDB

RedstoneDB is a site for sharing, verifying, and archiving high-quality Minecraft redstone builds, live at <https://redstonedb.com>. It was launched by embizone in October 2026, taking inspiration from and building on many earlier plot-of-logic projects.

Upload a `.schem`, from your computer or straight from your Schemati folder, and RedstoneDB works out what it is and how well it does it. Nothing is typed in by hand, so a build's specs can't drift out of date with its description, and an uploader can't enter incorrect information.

## What it tests

Depending on the circuit, every build is tested automatically for:

- build type: adder, register, ALU, and so on
- correctness, across every input case, or every pairing of the test patterns for wide builds
- latency, in ticks
- throughput: how often it can take a new input
- whether its outputs change in sync: every data output bit on the same tick, without glitches
- orientation: horizontal, vertical, or diagonal
- stackability, and how it ranks stacked to 8, 16, 32, and 64 bits
- the weakest signal strength its inputs need, and the weakest its outputs give
- the oldest Minecraft version it works in

Even an adder's design, like ripple carry or carry lookahead, is read from the circuit. Every test runs the build in [MCHPRS](https://github.com/MCHPR/MCHPRS)'s redstone compiler, Redpiler.

Recognized so far: adders, hex adders, subtractors, ALUs, multipliers, carry-save adders, incrementers, decrementers, comparators, hex maxes, carry outs, absolute values, negators, popcounts, thresholds, parity circuits, binary-to-hex and hex-to-binary converters, hex input spreads, gates, inverters, decoders, barrel shifters, monostables, registers, RAM, buffers, counters, shift registers, and T flip-flops. Anything else is still archived, as built, under Other.

## How builds are recognized

Each type of build has an input and output contract. RedstoneDB finds a build's inputs and outputs, then tries its lines in different roles and orders until they fit a type's truth table. No particular colours or signs are needed.

- **Outputs** are repeaters or comparators that power nothing: the block in front of each is air.
- **Inputs** are levers on the side of a solid block, with room for a repeater there so a bus could drive it. A hex-digit input is a comparator with air behind it.
- **Stacking** is checked by `//rstack`ing copies west or up and testing the result.

A build that doesn't fit still goes up, untested, with a note on what's in the way; upload a fixed version later and it's verified. The full rules, with pictures, are on the site: [How builds are recognized](https://redstonedb.com/about/recognized).

## Records

Builds that work stacked compete on leaderboards for each category at 8, 16, 32, and 64 bits. First place on a board, fastest or smallest, is a record, and a board can be narrowed by latency or by tags like torchless, synced, or a design. Gates, registers, RAM, and other cells rank by how small each copy is.

## Also

- Every build shows in 3D, in your own block theme, or in the colours its author gave it, and you can send any of them straight to your Schemati.
- Uploading a new version keeps the old ones. Compare any two in 3D, with what changed in colour.
- Tags RedstoneDB finds itself, like a design, orientation, or stackable, are teal; tags people add are other colours.
- Search understands shorthand like `8b`, `3t`, `torchless`, and `version:1.12`.
- Untick Public on upload and a build is yours alone. Star any build to keep it under Favourites on your profile.
- Write `@name` in a description to link a player.

## Contributing

Running the site locally, its command-line tool `rsdb`, and how the code is laid out are in [docs/development.md](docs/development.md). Every push to main deploys to the live site. More docs:

- [docs/conventions.md](docs/conventions.md): what a build needs, what the site does with it, and stacking.
- [docs/glossary.md](docs/glossary.md): categories, designs, fields, and their abbreviations.
- [docs/naming.md](docs/naming.md): how a build is named, and its download filename.
- [docs/errors.md](docs/errors.md): how the site words its errors.

## Credits

Built by Ben Burk, embizone in game. The 3D viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer), by Schem.at. RedstoneDB is not an official Minecraft product, and is not approved by or associated with Mojang or Microsoft.

## License

MIT.

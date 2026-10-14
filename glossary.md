# Glossary

Restrictions are tags in the database. A tag is named in full words, such as `torchless`, and the website and `rsdb show` print that name. A tag may also store a short token of 3 or 4 letters, which commands and search accept in its place. Each tag has a kind, `restriction` or `other`, which colours its badge; tags a person adds on the site are of the `other` kind.

A fact every build has, with one of a few values, is a field rather than a tag, as orientation, encoding, and logic are. A yes-or-no fact that means something only within one category, such as an adder's carry in, is a flag of that category. A tag is left for what is optional and crosses categories, where a build without it says nothing.

## Restrictions

Optional. Omitted when the build is unrestricted.

| Token       | Meaning                                                   |
| ----------- | --------------------------------------------------------- |
| `alpha`     | Works from Alpha: dust, torches, levers, buttons, plates. |
| `torchless` | No redstone torch, standing or on a wall.                 |

Restriction terms mean different things to different people. Before naming many builds with them, write down exactly which components each restriction forbids.

A restriction that can be read from the file is worked out from it rather than tagged by hand, so it is never wrong: each build's torches are counted whenever its file is stored, and one with none is torchless, and one that works from Alpha is alpha. Like `fails simulation`, such a tag cannot be added or removed by hand.

## Works from

A field read from the file: the oldest Minecraft: Java Edition release with every redstone component and mechanic the build uses, and the one that sets it, as in 1.5 for a comparator or 1.16 for a target block. Every build on the site works in the latest release, so there is no newest. Building blocks do not count, as any solid block stands in for white concrete and any transparent one for white stained glass. The releases, from the Minecraft Wiki's history sections, are in `src/works_from.rs`: Alpha, then dispensers and note blocks in Beta 1.2, repeaters in Beta 1.3, pistons in Beta 1.7, lamps in 1.2.1, tripwire hooks in 1.3.1, wooden buttons and repeater locking in 1.4.2, comparators, blocks of redstone, hoppers, droppers, and daylight detectors in 1.5, cauldrons read by comparators in 1.6.1, slime blocks and cakes read by comparators in 1.8, a block of redstone powering a comparator's side in 1.9, observers in 1.11, lecterns and composters in 1.14, honey blocks in 1.15, target blocks and the dot of dust in 1.16, lightning rods in 1.17, sculk sensors in 1.19, calibrated sculk sensors in 1.20, and the crafter and copper bulb in 1.21. Barrels and furnaces do not count: one matters only when a comparator reads it, which sets 1.5 already, and in an older release a chest or furnace stands in for a barrel, as a barrel stands in for the furnaces, cauldrons, and cakes uploads swap. Search finds the builds that work in a release with `version:1.12` or `version:beta_1.3`.

## Orientation

Required. Orientation also implies the stacking direction.

| Token  | Meaning    |
| ------ | ---------- |
| `horz` | horizontal |
| `vert` | vertical   |
| `diag` | diagonal   |

## Encoding

Required.

| Token | Meaning                 |
| ----- | ----------------------- |
| `bin` | binary                  |
| `hex` | signal-strength encoded |

## Logic

A field. Each standard category gives its builds' usual logic: a register is sequential and an adder combinational. A build gives its own only when it departs from that, such as a sequential multiplier (`--logic` on `rsdb add` and `set`, and `--unset logic` to follow the category again). Bussing, Display, Line Drawer, and categories off the list have no usual logic.

| Token  | Meaning       |
| ------ | ------------- |
| `comb` | combinational |
| `seq`  | sequential    |

Whether every output bit settles on the same tick is another field, `synchronized`, which is yes, no, or unknown. Search finds it with `sync` or `unsync`.

## Throughput

A field: the fewest ticks from one input to the next that still give right outputs, as in a 3-tick throughput, whatever the latency. Any build may have one; unknown is left empty rather than guessed. Search finds it with `throughput:3`.

A combinational, synchronized build whose throughput is below its latency is pipelined: a new input goes in before the last one's result comes out. Pipelined is worked out from those fields, not stored, and search finds it with `pipelined` or `pipe`.

## Categories

A category is one function: two builds share one only when they have the same truth table, the same outputs for the same inputs, so either could stand in for the other. So a subtractor is no adder, a decrementer no incrementer, and an XNOR gate no XOR gate, though each pair is built alike.

The standard categories are a list in the database, shown alphabetically: Adder, ALU, Barrel Shifter, BCD to Binary, Binary to BCD, Binary to Hex, Bussing, Carry Save Adder, Counter, Dataloop, Decoder, Decrementer, Display, Divider, Hex to Binary, Inc/Dec, Incrementer, Line Drawer, Multiplier, NAND+NOR Gate, NAND+XNOR Gate, Popcount, RAM, Register, RNG, Serial Adder, Serial Write Memory, Shift Register, Subtractor, Threshold, XNOR Gate, and XOR Gate. Each is stored by a name in lowercase words joined by underscores, such as `bcd_to_binary`, with the label people read beside it. `rsdb categories` shows the list, and `rsdb define-category` adds to its end, with `--label` when the label is not just the name's words capitalized. A category can also let each build give its own name, as Bussing and Display do: a build named Bus Crossing shows as, say, 8-bit Bus Crossing (`rsdb define-category --named`, and `--name` on `rsdb add` and `set`). A build of any other kind names its category freely, in words, numbers, and hyphens such as `3x3_piston_door` or `8-bit_horizontal_diode`, and shows it capitalized: 3x3 Piston Door. Such a build has no width or encoding.

Every build has a design: how it works, `other` unless its category lists a better one (`rsdb define-design`). A category's designs all do its one job, so they compete on one leaderboard. An adder's designs:

| Design                  | Short |
| ----------------------- | ----- |
| `ripple_carry`          | RCA   |
| `carry_lookahead`       | CLA   |
| `carry_cancel`          | CCA   |
| `carry_look_everywhere` | CLE   |

A design whose label is not just its name's words capitalized, such as LFSR for `lfsr`, stores that label (`rsdb define-design --label`).

The gates, XOR, XNOR, NAND+XNOR, and NAND+NOR, are each a category, under a Gates section of their own; the simpler gates are left out.

Each category has measures, the numbers its builds are named and ranked by, in order: width for most, as in "8-bit Adder"; width then capacity for a memory; throughput for a serial build; width then *k* for a threshold. Each leads the name, and each splits the leaderboards, so builds compete only at the same value of every one (`rsdb define-category --measure`, repeated; width alone when left out).

A category may have aliases, other words search finds it by, such as `connect4` for a four-in-a-row detector, `inc` for an incrementer, or `bcd` for both BCD converters (`rsdb define-alias`, or `--alias` on `define-category`). A category off the list may have them too.

The design is part of a build's name, as in "8-bit Carry Lookahead Adder", and so of its filename; see [naming.md](naming.md). A category may also have flags, yes-or-no facts its builds record. Most opt a build into columns of its category's truth table beyond the ones every build has, as an ALU's operations do, so builds with different flags still share their category and a leaderboard. The flags: adders and subtractors have Carry in and Carry out, set from signs labelled `carry in` or `cin` and `carry out` or `cout`, shift registers have Bidirectional and Writeable, ALUs have AND, OR, XOR, NAND, NOR, XNOR, SHL, SHR, and ASR for the operations they offer beyond adding and subtracting, ticked from their control lines' signs, Invert A and Invert B for inverting an operand first, Carry in and Carry out as adders have, and Overflow and Zero for those status lines, ticked from their signs, and multipliers have Upper product, for one that outputs the full product, as 16 bits from 8 by 8, and FMA, for a fused multiply-add, A×B+C.

An incrementer adds 1, a decrementer subtracts 1, and an inc/dec does either as a control line says; each is its own category. An incrementer's or a decrementer's control line, if it has one, is the 1 it adds or subtracts.

A threshold turns on when at least *k* of its inputs are on. Its width is its number of inputs, and its *k* is a measure after it, as in "8-bit At Least 3 Threshold", so builds of different *k* never share a leaderboard (`--at-least` on `rsdb add` and `set`). Search finds it as `"at least 3"`. Popcount, by contrast, outputs how many inputs are on; its width too is its number of inputs. Majority is a threshold at more than half the inputs, ≥5 of 8; it is no category of its own, since for an even count "half" is ambiguous, but `majority` finds thresholds.

An RNG gives random numbers, its width being the bits of each. Its designs are LFSR, a linear-feedback shift register, and Xorshift, which are pseudo-random and repeat once their state cycles, and Dropper, which reads the game's own randomness from droppers or dispensers. Its flag Seedable marks one that can be given a starting value.

A memory, RAM or Serial Write Memory, records its capacity in bytes, as in "8-bit 32-byte RAM", beside its width in bits; a stack of copies holds every copy's bytes. Its capacity is a measure after its width, so its leaderboards split by capacity after width (`rsdb define-category --measure width --measure bytes`, and `--bytes` on `rsdb add` and `set`).

A serial build takes its operands a bit at a time, so its input is a bit and its throughput is the ticks from one bit to the next, as in "2-tick Serial Adder", which takes two 1-bit streams and outputs one. A serial category's measure is throughput in place of width, so its throughput leads its builds' names and splits its leaderboards (`rsdb define-category --measure throughput`, and `--throughput` on `rsdb add` and `set`), and search finds a serial build's throughput as `2t` too. Ticks, not frequency: a 2-tick adder runs at 5 Hz, but nobody calls it that.

Add new standard categories here when adding them to the database.

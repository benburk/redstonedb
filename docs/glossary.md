# Glossary

Restrictions are tags in the database. A tag is named in full words, such as `torchless`, which dashes may join, as in `double-density`, though none may start or end it, since a dash before a search term leaves it out. The website and `rsdb show` print that name. A tag may also store a short token of 3 or 4 letters, which commands and search accept in its place. Each tag has a kind, `restriction` or `other`, which colours its badge; tags a person adds on the site are of the `other` kind.

A fact every build has, with one of a few values, is a field rather than a tag, as orientation, encoding, and statelessness are. A yes-or-no fact that means something only within one category, such as an adder's carry in, is a flag of that category. A tag is left for what is optional and crosses categories, where a build without it says nothing.

## Restrictions

Optional. Omitted when the build is unrestricted.

| Token       | Meaning                                                   |
| ----------- | --------------------------------------------------------- |
| `alpha`     | Works from Alpha: dust, torches, levers, buttons, plates. |
| `torchless` | No redstone torch, standing or on a wall.                 |

Restriction terms mean different things to different people. Before naming many builds with them, write down exactly which components each restriction forbids.

A restriction that can be read from the file is worked out from it rather than tagged by hand, so it is never wrong: each build's torches are counted whenever its file is stored, and one with none is torchless, and one that works from Alpha is alpha. Like `fails simulation`, such a tag cannot be added or removed by hand: the upload and edit forms show it in the tag field, read from the file chosen, without a remove button, and never suggest it.

## Works from

A field read from the file: the oldest Minecraft: Java Edition release with every redstone component and mechanic the build uses, and the one that sets it, as in 1.5 for a comparator or 1.16 for a target block. Every build on the site works in the latest release, so there is no newest. Building blocks do not count, as any solid block stands in for white concrete and any transparent one for white stained glass. The releases, from the Minecraft Wiki's history sections, are in `src/works_from.rs`: Alpha, then dispensers and note blocks in Beta 1.2, repeaters in Beta 1.3, pistons in Beta 1.7, lamps in 1.2.1, tripwire hooks in 1.3.1, wooden buttons and repeater locking in 1.4.2, comparators, blocks of redstone, hoppers, droppers, and daylight detectors in 1.5, cauldrons read by comparators in 1.6.1, slime blocks and cakes read by comparators in 1.8, a block of redstone powering a comparator's side in 1.9, observers in 1.11, lecterns and composters in 1.14, honey blocks in 1.15, target blocks and the dot of dust in 1.16, lightning rods in 1.17, sculk sensors in 1.19, calibrated sculk sensors in 1.20, and the crafter and copper bulb in 1.21. Where a component rests counts, though, as one could not always rest on a transparent block: dust or a standing torch on one sets 1.2.1, when dust could go on glowstone, passing signals only up through it as through glass, and a torch on glass; a repeater, comparator, lever, button, or pressure plate on one sets 1.3.1, when they could go on an upside-down slab; and a torch, lever, or button on its side or underside sets 1.14, when glass came to hold them. Barrels and furnaces do not count: one matters only when a comparator reads it, which sets 1.5 already, and in an older release a chest or furnace stands in for a barrel, as a barrel stands in for the furnaces, cauldrons, and cakes uploads swap. Search finds the builds that work in a release with `version:1.12` or `version:beta_1.3`.

## Orientation

The way a build's bits run, which is also the way copies of it add bits. A category with a test that takes input A as its operand, such as an adder, reads it from the file; any other build gives it.

| Token  | Meaning    |
| ------ | ---------- |
| `horz` | horizontal |
| `vert` | vertical   |
| `diag` | diagonal   |

## Encoding

How a build's numbers are written on its lines. A category that reads its orientation from the file reads its encoding too; a build of a category off the list has none.

| Token | Meaning                 |
| ----- | ----------------------- |
| `bin` | binary                  |
| `hex` | signal-strength encoded |

## Unaligned

A tag worked out from a build's file, never given by hand: a bus of its, an operand or an output, whose bits aren't one evenly spaced line, or two alternating, as the conventions lay a bus, such as one whose top bits stand out of line, whose outputs are unevenly spaced, or whose levers climb a staircase. Such a build is recognized as its category when every case of its test comes out right, and lists in its category as any verified build does, but ranks on no board; ticking Hide unaligned leaves such builds out of a listing.

## Stateless

A field: whether, once a build's inputs stop changing, its outputs depend only on them. An adder is stateless; a register, which remembers what was written, is stateful. A pipelined build is stateless too, holding values only on their way through. Each standard category gives its builds' usual value, and a build gives its own only when it departs from that, such as a stateful multiplier (`--logic stateless` or `--logic stateful` on `rsdb set`, and `--unset logic` to follow the category again). Bussing, Display, Line Drawer, and categories off the list have no usual value. The field was called logic, combinational or sequential; search finds it by either name.

| Token       | Meaning   | Also searched as        |
| ----------- | --------- | ----------------------- |
| `stateless` | stateless | `comb`, `combinational` |
| `stateful`  | stateful  | `seq`, `sequential`     |

Whether every output bit settles on the same tick is another field, `synchronized`, which is yes, no, or unknown. Search finds it with `sync` or `unsync`.

## Throughput

A field: the fewest ticks from one input to the next that still give right outputs, as in a 3-tick throughput, whatever the latency. Any build may have one; unknown is left empty rather than guessed. Search finds it with `throughput:3`.

A stateless, synchronized build whose throughput is below its latency is pipelined: a new input goes in before the last one's result comes out. Pipelined is worked out from those fields, not stored, and search finds it with `pipelined` or `pipe`.

## Categories

A category is one function: two builds share one only when they have the same truth table, the same outputs for the same inputs, so either could stand in for the other. One difference does not split a category: inverting a few lines, a fixed number of torches whatever the build's width, which makes either build of the other. A flag tells them apart. So a signed comparator is a Greater Than with the flag Signed, as inverting each operand's top bit turns one into the other, and a monostable's flags say whether its output is inverted and which edge of A it pulses on. The gates are the exception: XOR, XNOR, AND, NAND, OR, and NOR are each a category of its own, so the builds ranked together do one thing, though a torch on its output turns each into the other of its pair. Inverting a whole bus, a torch on every bit, does split one, as those torches cost ticks and blocks that grow with its width, which its leaderboard compares. So a subtractor is no adder, though an adder with A and its sum inverted subtracts, and a decrementer no incrementer.

The standard categories are a table in the code, `src/category/table.rs`, shown alphabetically: Absolute Value, Adder, ALU, AND Gate, Barrel Shifter, BCD to Binary, Binary to BCD, Binary to Hex, Buffer, Bussing, Carry Out, Carry Save Adder, Counter, Dataloop, Decoder, Decrementer, Display, Divider, Dual-Edge Monostable, Equality, Greater Than, Hex Adder, Hex to Binary, Inc/Dec, Incrementer, Inverter, Line Drawer, Monostable, Multiplier, NAND Gate, NAND+NOR Gate, NAND+XNOR Gate, Negator, NOR Gate, OR Gate, Parity, Popcount, RAM, Register, RNG, Serial Adder, Serial Write Memory, Shift Register, Subtractor, T Flip-Flop, Threshold, XNOR Gate, and XOR Gate. Each is stored by a name in lowercase words joined by underscores, such as `bcd_to_binary`, with the label people read beside it. `rsdb categories` shows the list; a category is added as a row of that table, its label beside its name. A category can also let each build give its own name, as Bussing and Display do: a build named Bus Crossing shows as, say, 8-bit Bus Crossing. A build of any other kind names its category freely, in words, numbers, and hyphens such as `3x3_piston_door` or `8-bit_horizontal_diode`, and shows it capitalized: 3x3 Piston Door. Such a build has no width or encoding.

A build's design is how it works, one of those its category lists in its row; a build may have none. A category's designs all do its one job, so they compete on one leaderboard. An adder's designs:

| Design                  | Short |
| ----------------------- | ----- |
| `ripple_carry`          | RCA   |
| `carry_lookahead`       | CLA   |
| `carry_cancel`          | CCA   |
| `carry_look_everywhere` | CLE   |

A subtractor has the same designs, as its borrows run through the same chains an adder's carries do.

A design whose label is not just its name's words capitalized, such as LFSR for `lfsr`, keeps that label in its category's row.

The gates, AND, NAND, OR, NOR, XOR, XNOR, NAND+XNOR, and NAND+NOR, are each a category, under a Gates section of their own.

Each category has measures, the numbers its builds are named and ranked by, in order: width for most, as in "8-bit Adder"; width then capacity for a memory; throughput for a serial build; width then *k* for a threshold. Each leads the name, and each splits the leaderboards, so builds compete only at the same value of every one, width alone unless its row says otherwise.

A category may have aliases, other words search finds it by, such as `connect4` for a four-in-a-row detector, `inc` for an incrementer, or `bcd` for both BCD converters, kept in its row. A category off the list may have them too, in `OFF_LIST_ALIASES` beside the table.

The design is part of a build's name, as in "8-bit Carry Lookahead Adder", and so of its filename; see [naming.md](naming.md). A category may also have flags, yes-or-no facts its builds record. Most opt a build into columns of its category's truth table beyond the ones every build has, as an ALU's operations do, so builds with different flags still share their category and a leaderboard. The flags: adders and hex adders have Carry in and Carry out and subtractors Borrow in and Borrow out, each read from its file, ticked for a lone magenta control line and a lone pink status line, which need no signs, and Inverted carry in, set by the simulation for a carry in that reads inverted; shift registers have Bidirectional and Writeable, ALUs have AND, OR, XOR, NAND, NOR, XNOR, SHL, SHR, and ASR for the operations they offer beyond adding and subtracting, ticked from their control lines' signs, Carry in and Carry out as adders have, and Overflow and Zero for those status lines, ticked from their signs, multipliers have Upper product, for one that outputs the full product, as 16 bits from 8 by 8, and FMA, for a fused multiply-add, A×B+C, monostables have Inverted, for one whose output rests on, and Falling edge, for one that pulses as A turns off, both set by the simulation, dual-edge monostables Inverted, and registers Inverted reads, for one whose reads show its cells inverted.

A register's cells share an output bus, each shown on it only while its read lever is on. A buffer has no reads: each cell shows what it holds on its own bus all the time, so it is a category of its own, as its outputs differ with every read off.

A comparison takes two operands, A and B, of one width, and gives one line. Greater Than says whether A is greater than B, both unsigned, or with the flag Signed, both in two's complement; the simulation ticks it for a build that compares signed. Equality says whether A equals B, which is the same either way. A build giving whether A is less than B is a Greater Than with its operands named the other way. `comparator` finds both.

An incrementer adds 1, a decrementer subtracts 1, and an inc/dec does either as a control line says; each is its own category. An incrementer's or a decrementer's control line, if it has one, is the 1 it adds or subtracts.

A negator gives −A, A negated in two's complement, as wide as A, so 0 and the most negative A give themselves. Its control line, if it has one, is its negate: A shows unchanged while it is off. `neg`, `negate`, and `twos complement` find it. Copies of one make no wider one, so it never stacks.

A monostable turns input A turning on, or with Falling edge turning off, into a pulse of a set number of ticks on its output, and gives none on the other edge. A dual-edge monostable pulses on both, which no torch makes of a single-edge one, so it is a category of its own, measured the same way, its pulse alike on both edges. Its pulse is its measure, as in "3-tick Monostable", so builds of different pulses never share a leaderboard; the simulation measures it. Search finds it as `3t` or `pulse:3`. Its output rests the same with A on as off, so it is stateless: off, with a high pulse, or on, with a low one, which is Inverted. Its latency is from A changing on its edge to its pulse starting. Copies stacked side by side are more bits, each with its own input and output. `mono` and `pulse generator` find both kinds.

An inverter takes a bus, A, and one control line, and gives A with every bit inverted while the line is on: A xor the line, a row of XOR gates sharing one input. Its width is A's. Copies stacked side by side are more bits sharing the line, as far as the line carries. `conditional inverter`, `controlled inverter`, and `xor bank` find it.

A threshold turns on when at least *k* of its inputs are on. Its width is its number of inputs, and its *k* is a measure after it, as in "8-bit At Least 3 Threshold", so builds of different *k* never share a leaderboard. Search finds it as `"at least 3"`. Popcount, by contrast, outputs how many inputs are on; its width too is its number of inputs. Majority is a threshold at more than half the inputs, ≥5 of 8; it is no category of its own, since for an even count "half" is ambiguous, but `majority` finds thresholds.

An RNG gives random numbers, its width being the bits of each. Its designs are LFSR, a linear-feedback shift register, and Xorshift, which are pseudo-random and repeat once their state cycles, and Dropper, which reads the game's own randomness from droppers or dispensers. Its flag Seedable marks one that can be given a starting value.

A memory, RAM or Serial Write Memory, records its capacity in bytes, as in "8-bit 32-byte RAM", beside its width in bits; a stack of copies holds every copy's bytes. Its capacity is a measure after its width, so its leaderboards split by capacity after width.

A serial build takes its operands a bit at a time, so its input is a bit and its throughput is the ticks from one bit to the next, as in "2-tick Serial Adder", which takes two 1-bit streams and outputs one. A serial category's measure is throughput in place of width, so its throughput leads its builds' names and splits its leaderboards, and search finds a serial build's throughput as `2t` too. Ticks, not frequency: a 2-tick adder runs at 5 Hz, but nobody calls it that.

Add new standard categories here when adding them to the database.

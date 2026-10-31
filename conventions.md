# Building conventions

What the site needs from a build, and what it does with one. The Before you upload page states the same rules; `rsdb inspect` checks a file or a stored build by them.

A build is on the list only when probing recognizes it as a blessed category: adders, registers, ALUs, decoders, multipliers, counters, shift registers, gates, comparators, carry outs, inverters, and absolute values so far. Any other build goes up as built, off the list, under a name of its own, such as `8-bit counter` or `3x3 piston door`, with the facts its builder gives. A new version that is recognized moves it up.

## Inputs

- An input is a lever on the side of a solid block, or a stone button for a pulse, such as a register's write. Never on the top or bottom.
- The block drives at most two dusts, on sides other than its lever's, besides torches, repeaters, and comparators reading it. No other redstone touches it, though a target or redstone block may sit on top.
- A data input, such as an operand's bit or a register's data bus, is one a bus may drive: a repeater must fit where its lever is, standing on a block or above air, with nothing behind it but air or a sign, where the bus's driver feeds it. No dust on top of its block.
- A control line, such as a carry in, a register's write, or an ALU's operation, is always driven by a lever or button, so its block may have dust on top and a block behind its lever.
- Probing tells data inputs from control lines: an adder's are A and B, a register's its data bus in, an ALU's and a multiplier's their two widest input lines, a decoder's its one input line.
- An input may read inverted, powered meaning 0. The simulator finds which lines do by testing, however their levers were saved.
- A multiplier has inputs A and B and, if sequential, one control line, its calculate, pressed once A and B are set; it has no reset. An ALU's lone outputs are named by their signs, such as `zero`.

## Outputs

- Each output bit is the dust, repeater, or comparator on a coloured block: any colour but white, gray, or an input colour, with no block of its own colour touching it, even diagonally. The bit is on while that component is.
- An adder's or ALU's widest output is its output and each lone one a status line, such as a carry out. A multiplier's product may be up to three lines, which probing puts in order, lowest first.
- A counter's or shift register's widest output is the value it holds, and each lone output a status line, such as a shift out. An input bus as wide is A, which it loads.
- A gate has two input lines of one width, A and B, both data inputs, and one output line as wide: an XOR, AND, or OR gate, or its inverse, an XNOR, NAND, or NOR, which goes in the same category under a flag.
- An equality or greater-than comparator, or a carry out, has two input lines of one width, A and B, and one output bit; a carry out may also take a carry in, a lone lever.
- An inverter has input A, one control line, its invert, and an output as wide as A; an absolute value has input A and one output line, its magnitude.
- A counter has input A, one button, its clock, and one lever, its write enable: pressing the clock counts up or down, and pressing it with write enable on loads A.
- A shift register's every other lever or button is a control line, and what each does, shifting, loading, or clearing, alone or with another lever held on, is found by trying it.

## Buses

- Each bus is a line of bits, evenly spaced: straight, or rising one block per block across. A bus may also be two such lines, its bits alternating.
- A bus's least significant bit is nearest the box's south-east-bottom corner, counting along the axis its bits spread furthest. Standardizing mirrors an upload east for west when that reads right. An `lsb` or `msb` sign on a bus end turns its count round.

## What the site does

1. Probes the levers, buttons, and outputs, trying each blessed category's test.
2. Once recognized, colours each line by its role and standardizes the rest:

   | Role         | Block                                   |
   | ------------ | --------------------------------------- |
   | Solid        | white concrete                          |
   | Transparent  | white stained glass                     |
   | Inputs A–D   | lime, light blue, cyan, blue concrete   |
   | Control line | magenta concrete                        |
   | Outputs 1–3  | yellow, orange, brown concrete or glass |
   | Status line  | pink concrete or glass                  |

   Each viewer sees it in their own block theme.
3. Turns it to lie the standard way, A's levers on the south, and stores every input at logical 0.
4. Takes out blocks no part of its circuit uses, as a new version, once compiling shows the circuit unchanged. Dust and the block under it always stay.
5. Simulates every case, or every case of each byte when there are too many, and measures latency, whether it is synchronized, throughput, whether it holds state, and how copies stack. An adder's design is read from how its carries are made.

## Stacking

- A stackable build is saved as one slice: the fewest bits that repeat, such as one bit of a ripple carry adder or register.
- The site stacks as a player can: paste the first copy whole, then `//rstack` a box of it, as many copies as wanted, a step apart. Each copy pastes over earlier ones: the later block stands, and air never replaces anything.
- The box is the slice less the layers that only the first copy needs, on the face its stack starts from: the east for a stack west, the bottom for one up, the south for one north. What only the first copy keeps, such as a carry in and its lever, goes there.
- Where the box's copies don't work, the site searches smaller boxes, the fewest blocks left out first, shrinking only off a face whose outer layer holds a control line's switch, and keeps the first whose copies work.
- A download can mark the box with glass at two opposite corners, to select it by in game and then break.
- A stackable build competes on the leaderboards at 8, 16, and 32 bits too, its copies stacked to each width, tested, and timed.

## Blocks

- Furnaces, cauldrons, and cakes become barrels of the same strength. Hoppers are kept as a fixed strength unless the uploader swaps them too.
- The simulator doesn't model pistons, observers, or wooden buttons; a build with one isn't recognized.

## Bounding box

- A stored box fits its blocks tightly. Import removes glass corner markers and trims empty layers.
- Size is that box less any outer layer holding only interface: input blocks with their levers, dust, signs, and torches.
- Sizes read height × width × depth, seen from the south. Positions count from the box's north-west-bottom corner.
- Every stored or served file pastes the same way: `//paste` puts the player one block south of the build's south-east-bottom corner.

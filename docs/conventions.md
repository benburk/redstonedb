# Building conventions

What the site needs from a build, and what it does with one. The How builds are recognized page states the same rules; `rsdb inspect` checks a file or a stored build by them.

A build is on the list only when probing recognizes it as a blessed category: adders, ALUs, multipliers, incrementers, decrementers, comparators, carry outs, absolute values, negators, popcounts, thresholds, binary-to-hex and hex-to-binary converters, gates, inverters, decoders, barrel shifters, monostables, registers, buffers, counters, shift registers, and T flip-flops so far. Any other build goes up as built, off the list, under a name of its own, such as `8-bit counter` or `3x3 piston door`, with the facts its author gives. A new version that is recognized moves it up.

## Inputs

- An input is a lever on the side of a solid block, or a button for a pulse, such as a register's write: stone for 10 ticks, wooden for 15. Never on the top or bottom.
- The block drives dust on any side but its lever's, on top and below too, and torches, repeaters, and comparators reading it. No other redstone touches it, though a target may, and a redstone block may sit on top.
- A data input, such as an operand's bit or a register's data bus, is one a bus may drive: a repeater must fit where its lever is, standing on a block or above air, with nothing behind it but air or a sign, where the bus's driver feeds it.
- A control line, such as a carry in, a register's write, or an ALU's operation, is always driven by a lever or button, so it may have a block behind its lever.
- Probing tells data inputs from control lines: an adder's are A and B, a register's its data bus in, an ALU's and a multiplier's their two widest input lines, a decoder's its one input line.
- A control line may read inverted, powered meaning 0, as an adder's carry in or a counter's clock may. The simulator finds which do by testing, however their levers were saved. A data input never reads inverted: its lever off is its 0.
- A multiplier has inputs A and B and, if sequential, one control line, its calculate, pressed once A and B are set; it has no reset. An ALU's lone outputs are named by their signs, such as `zero`.

## Outputs

- Each output bit is the dust, repeater, or comparator on a block with no block of its own kind touching it, even diagonally: any solid block but a redstone component, such as a target or lamp, or stained glass of any colour but white. Plain and white glass are what a build holds dust up with. A kind of block marks outputs only where every block of it does, so one a build is also built of never does. The bit is on while that component is. A repeater or comparator powering a block in front of it carries the circuit on, so the block under it is no output.
- No output reads inverted: each bit is the answer as it shows. A register's reads may show its cells inverted and a monostable's pulse may be low, each under a flag, and a decoder's outputs are inverted, its selected bit off and the rest on.
- An adder's or ALU's widest output is its output and each lone one a status line, such as a carry out. A multiplier's product may be up to three lines, which probing puts in order, lowest first.
- A counter's or shift register's widest output is the value it holds, and each lone output a status line, such as a shift out. An input bus as wide is A, which it loads.
- A gate has two input lines of one width, A and B, both data inputs, and one output line as wide: an XOR, XNOR, AND, NAND, OR, or NOR gate, each a category of its own, so a gate whose output is inverted is the other gate of its pair.
- An equality or greater-than comparator, or a carry out, has two input lines of one width, A and B, and one output bit; a carry out may also take a carry in, a lone lever.
- An inverter has input A, one control line, its invert, and an output as wide as A; an absolute value has input A and one output line, its magnitude.
- A negator has input A, an output as wide, and, if it has one, a control line, its negate: its output is −A, or with the line, A while it is off and −A while it is on.
- An incrementer or decrementer has input A and, if it has one, a control line that adds or subtracts it; an inc/dec's line picks between adding and subtracting 1. Its output may be a bit wider, or have a lone carry out.
- A barrel shifter has input A, an output as wide, and lines for its shift: the amount's signed with what each is worth, as `1`, `2`, and `4`.
- A binary-to-hex converter has input A, of at most 4 bits, and one output block whose strength is A.
- A hex-to-binary converter has input A, a hex digit a block, each a comparator with air behind, and outputs giving A's bits, four a digit; a lone output beside a bus is its lowest bit or its highest, which the test finds.
- A popcount has input A and outputs wide enough for its count, or one output block whose strength is the count; a threshold has input A and one output, on when at least some number of A's bits are, which the test finds.
- A monostable has input A and a lone output, its pulse. A buffer is read as a register is.
- A counter has input A, one button, its clock, and one lever, its write enable: pressing the clock counts up or down, and pressing it with write enable on loads A.
- A shift register's every other lever or button is a control line, and what each does, shifting, loading, or clearing, alone or with another lever held on, is found by trying it.
- A T flip-flop has one input, a button or a lever, and one output bit, which each press, or each turn of the lever on and off, toggles.

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

   Each viewer sees and downloads it in their own block theme, or in colours they pick for one download. A theme may mark a role with any colour, so long as its block is none of the theme's own and no other role's.
3. Turns it to lie the standard way, A's levers on the south, and stores each data input's lever off, at logical 0, settling the circuit with them. A control line's levers stay as saved, for its test to find which read inverted, and a build kept as built keeps its own levers.
4. Takes out blocks no part of its circuit uses, as a new version, once compiling shows the circuit unchanged. Dust and the block under it always stay.
5. Simulates every case, or every case of each byte when there are too many, and measures latency, whether it is synchronized, throughput, whether it holds state, and how copies stack. An adder's design is read from how its carries are made.

## Stacking

- A stackable build is saved as one slice: the fewest bits that repeat, such as one bit of a ripple carry adder or register.
- The site stacks as a player can: paste the first copy whole, then `//rstack` a box of it, as many copies as wanted, a step apart. Each copy pastes over earlier ones: the later block stands, and air never replaces anything.
- The box is the slice less the layers that only the first copy needs, on the face its stack starts from: the east for a stack west, the bottom for one up, the south for one north. What only the first copy keeps, such as a carry in and its lever, goes there.
- Where the box's copies don't work, the site searches smaller boxes, the fewest blocks left out first, shrinking only off a face whose outer layer holds a control line's switch, and keeps the first whose copies work.
- A download of one copy can mark the box of one direction with glass at two opposite corners, to select it by in game and then break.
- A stackable build competes on the leaderboards at 8, 16, and 32 bits too, its copies stacked to each width, tested, and timed.

## Blocks

- Furnaces, cauldrons, and cakes become barrels of the same strength. Hoppers are kept as a fixed strength unless the uploader swaps them too.
- The simulator doesn't model pistons or observers; a build with one isn't recognized. It holds a wooden button as a lever it turns off 15 ticks after each press.

## Bounding box

- A stored box fits its blocks tightly. Import removes glass corner markers and trims empty layers.
- Size is that box less any outer layer holding only interface: input blocks with their levers, dust, signs, and torches.
- Sizes read height × width × depth, seen from the south. Positions count from the box's north-west-bottom corner.
- Every stored or served file pastes the same way: `//paste` puts the player one block south of the build's south-east-bottom corner.

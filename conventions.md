# Building conventions

Every schematic follows one default convention, so builds combine cleanly and their names describe them consistently. `rsdb lint` and the upload page check it. A build that breaks one is not recognized: it goes up with its own blocks under a category off the list, named for what it seems meant to be, until a conforming new version moves it up. [features.md](features.md) lists checks still to come.

## Block palette

Only whitelisted blocks. Each interface role has its own block, so colour gives role; inputs and outputs stand out bright against white. Red is left to dust and torches.

| Role         | Block                                   |
| ------------ | --------------------------------------- |
| Solid        | white concrete                          |
| Transparent  | white stained glass                     |
| Inputs A–D   | lime, light blue, cyan, blue concrete   |
| Control line | magenta concrete                        |
| Outputs 1–3  | yellow, orange, brown concrete or glass |
| Status line  | pink concrete or glass                  |
| First copy   | gray concrete or stained glass          |

- Cool colours are inputs, warm outputs, each lightest first: inputs green to blue (A to D), outputs yellow to brown (first to third). Each colour is one bus. Each magenta block is its own control line and each pink block its own status line, named by its own sign, unless one sign names a row of them.
- An output colour may be stained glass where the output needs a transparent block.
- Every other solid block is white; import derives hard-powered, soft-powered, and blocker from the surrounding components. Powered and unpowered dust both read clearly on white.
- White stained glass replaces slabs and glowstone. Open: check it behaves the same wherever they are used, such as dust placement and one-way upward transmission, and decide exceptions where it doesn't.
- No plain glass, so a download's WorldEdit corner markers are never mistaken for the build.
- No observers: the simulator doesn't model them.
- A green and B blue stay apart under red-green colour blindness.
- One colour per operand, not per power requirement; drive is derived (see Inputs).
- Magenta beat orange in game: dust reads better on it powered, slightly worse unpowered, and both states count.
- Gray marks logic only a stack's first copy keeps, such as what the least significant bit alone needs (see Stacking). Dust reads on gray powered and unpowered; on red it would not.
- Colours are semantic, not aesthetic. No two roles share a block, so export can recolour each role to a user's palette.

## Inputs

- Each input bit is a concrete block with a lever: lime A, light blue B, cyan C, blue D, magenta for each control line (clock, enable, carry in, ALU op). A further operand, such as a fused multiply-add's addend, is C.
- One operand colour is one input. Unnamed operands take letters in colour order, so the first present is A. A sign on any block of the colour, such as `addend`, names the whole input; a colour takes one name.
- One sign on any line of a row of control lines names the row, such as a memory's read lines, one per word for the builder's own decoder. The lines run straight, evenly spaced, numbered from the one nearest the south-east-bottom corner: `read 0` to `read 15`. A second sign starts another row, such as the write lines. Pink status lines may form rows the same way. A memory may instead take a binary address as an operand bus and decode it itself.
- Lone lines with a plain role need no sign. Every other category signs every line.

| Category    | Control line      | Status line or output  |
| ----------- | ----------------- | ---------------------- |
| Adder       | carry in (`cin`)  | carry out (`cout`)     |
| Carry out   | carry in (`cin`)  | carry out (`cout`)     |
| Subtractor  | borrow in (`bin`) | borrow out (`bout`)    |
| Incrementer | increment         | carry out              |
| Decrementer | decrement         | borrow out             |
| Comparison  |                   | result, yellow or pink |
| Monostable  |                   | pulse                  |
| Inverter    | invert            |                        |

- A sign on an adder's or subtractor's lone line must name it as above. An adder control line named anything else, such as `subtract`, makes it another category, such as an ALU; more than one of either line breaks it. Borrow in takes 1 more away; borrow out is on when it takes away more than A. A comparison's result is `A > B` or `A = B`.
- An ALU control line's sign names the operations it takes part in, separated by spaces or lines, never commas: `and nand sub`. Choosing an operation flips every line naming it from its saved position, so a line saved on goes off. An operation may be on several lines. Operations: `add`, `sub`, `and`, `or`, `xor`, `nand`, `nor`, `xnor`, `shl`, `shr`, `asr`; `rsh` and `lsh` read as `shr` and `shl`. A line named `carry in` or `cin` is a carry in, which add takes. Standardizing takes commas out of every sign. ALU status lines:

| Name            | Shows                                                 |
| --------------- | ----------------------------------------------------- |
| `carry out`     | adder carry out; for sub, A + ~B + 1, on if no borrow |
| `overflow`      | signed overflow of add or sub                         |
| `lsb shift out` | the bit a shift right drops                           |
| `zero`          | result 0                                              |

- An input colour without a lever is not allowed, except a signal-strength input. Standardizing keeps input-colour blocks with a lever, as concrete, and input-colour concrete with dust on top, and whitens the rest.
- A control line may use a button for a pulse, such as reset or calculate. Operand bits always use levers.
- A register is one or more cells of a CPU register file. Data in is lime; data out yellow, one bus per read port. Each cell has a write and a read on magenta lines: a button writes, a lever reads. Unsigned button and lever lines read as writes and reads; a line signed `write` or `read` is one whatever drives it. A cell drives the shared bus only while its read is on. A read may be inverted (reads with its lever as saved); an output may be inverted (bus shows the complement, resting at all ones). The simulator finds both and pairs reads with writes itself, so cells may be staggered, as two to a slice.
- A buffer is a register without reads: each cell shows its value on its own yellow bus all the time. Data in is lime; each cell's write is a magenta line with a button, or a lever that passes data through while on. A lone write needs no sign. With several, buttons are writes and a lever beside a button is a read, which makes it a register.
- An inverted input is saved lever on, with no torch; the lever marks the inversion.
- The lever touches only its input block. Wiring replaces it with a driver into that block, so nothing may rely on the lever powering it directly.
- An input block feeds at most one dust, on a side other than its lever's, so wiring puts one driver into one wire. No dust on top or underneath. Nothing else powers it: no repeater or comparator into it, no torch under it, no other component touching it, such as a redstone block or target. Any number of torches hung or standing on it, repeaters and comparators reading it, and neighbouring inputs' levers are fine.
- The lever's face is where power enters, so it marks where a driver goes.
- Every A lever is on its block's south side, as in the stored adders: the side a player facing north sees. Standardizing turns an upload about the vertical to make it so. Other inputs' levers or buttons go on any side, north, south, east, or west, never top or bottom.

### Signal-strength inputs

- Takes a value 0–15 as its strength, which no lever can give. Each bit is an operand-colour block with no lever and dust on top; each block is one hex digit, four bits of the build's width.
- The dust's one open end, leading nowhere, is where power enters and a driver goes. It may face any side, unlike A's levers. Give each input its own open end, so its driver feeds it alone.
- One colour is one bus, so an operand is all levered or all signal-strength.
- The derived drive and `4ss` sign don't apply: the strength is the value.
- No recognized category takes one yet; such a build goes under its own category until the simulator can drive a strength well.

### How an input must be driven

Import derives this; nothing in-world marks it.

- Hard-powered: dust touches the block, on top, beside, or below. Only a strongly powered block powers dust, so the driver must be a repeater, comparator, or torch.
- Soft-powered: only torches, repeaters, comparators, and mechanisms touch the block. They respond to weak power, so any signal works, dust included.
- A lever strongly powers its block at 15, so a saved build always works hard-powered; the derivation only decides whether weaker drivers work too.
- Required strength is 1 or more unless a sign on one of the operand's blocks, or the control line's sign, says otherwise, such as `4ss` for 4 or more. It applies to every bit of the operand, and as a minimum doesn't describe signal-strength inputs.
- The simulator drives every input by lever at full hard power, so it checks neither yet. It could: drive each input weakly at 1, 2, up to 15, and take the weakest that gives right outputs; if none, it needs hard power. The sign would then be a check, not the source.

### Signal strengths

- A fixed strength, such as 1ss, is a barrel of redstone dust read by a comparator. A barrel is solid and repeats in every stack copy.
- No furnaces, cauldrons, or cakes. Every file the site takes in has each swapped for a barrel of the same strength, and the upload page says so. A furnace conducts as a barrel does, so nothing else changes.
- Hoppers are allowed only as a fixed strength: the simulator reads their contents and moves no items. The upload page offers a same-strength barrel instead.
- A strength only the first copy needs, such as a 1ss feeding its carry, is a barrel on gray.

## Outputs

- Every output bit is yellow, orange, or brown concrete, or that colour of stained glass where it needs a transparent block. One colour is one bus. Unnamed, a lone bus is `output`; several are `output 1` to `output 3` in colour order. A sign on any block, such as `low` or `remainder`, names the whole bus. When buses make one value, such as a 16-bit product from two 8-bit halves, yellow holds the lowest bits.
- A status line, such as `carry out`, `overflow`, or `zero`, is pink concrete or glass, each block its own line with its own sign. A status name on another output colour is an error. Standardizing turns a block signed with a status output pink, since older builds used yellow; `rsdb reread` did so for stored builds.
- Each output block carries the component leading the output out: dust, a repeater, or a comparator on it. The bit is on while that component is, however the block below is powered. A block with nothing on it, such as one powered by dust into its side or a torch beneath, is not an output, and the simulator fails the build.
- Redstone lamps are ordinary components and don't mark outputs.

## Signs

Import reads signs to extract a build's interface without a human. Everything else, latency included, is database metadata.

| Where              | First line           | Further lines      |
| ------------------ | -------------------- | ------------------ |
| Control line       | its name             | strength, as `4ss` |
| Status line        | its name             |                    |
| Input colour, any  | bus name or strength | strength, as `4ss` |
| Output colour, any | bus name, if any     |                    |
| End of a bus       | `lsb` or `msb`       |                    |

- A status name ends in `out` (`carry out`, `cout`, `shift out`) or is `overflow`, `zero`, `negative`, `ready`, or `done`.
- Each further line is one token, such as `4ss`.
- Signs stay in the file and are reread with it. Rewriting or stripping them on export is planned; see [features.md](features.md).

## Bit order

- A bus's least significant bit is nearest the box's south-east-bottom corner. Bits count up along the bus's axis, the one its bits spread furthest along; of two bits level along it, the nearer comes first.
- So horizontal buses count from the east end and vertical buses from the bottom, even when bits zig-zag between columns, as a double density adder's alternate layers.
- Standardizing mirrors an upload east for west, keeping A's levers south, when the mirror gets more quick cases right for what it seems to be, such as an adder, subtractor, or ALU with its least significant bits at the west end. It never flips a build upside down, which redstone doesn't survive.
- A bus counting the other way has a sign reading `lsb` on its least significant end or `msb` on its most significant end. Either word alone on a line is a mark, not a name, and belongs on a bus end. One such sign orders every unsigned bus alongside it, so a build with all least significant bits on top needs one bus signed.

## Stacking

- A stackable build is saved as one slice: the fewest bits that repeat, such as one bit of a ripple carry adder or register, or two or four bits of a staggered design.
- A slice may stack along several directions, each with its own step and most copies. Copies count up from the south-east-bottom corner: west, up, north, or a mix, such as west and up for a diagonal build. A register might stack 7 times west for bits and 3 times north for registers.
- Copies along the bits, west for horizontal, up for vertical, west and up for diagonal, are new bits with their own data inputs and outputs. Copies any other way are parallel copies and keep the width.
- The site stacks as a player can, with `//rstack` and fixups:
  1. Break the parts only the first copy keeps: gray blocks and what rests on them; inputs the copy before feeds, such as a carry in; and, for copies across a register's or RAM's bits, its data levers and the data blocks that would land on a block of the copy before. Each goes with its lever, signs, and dust.
  2. `//rstack` the rest, as many copies as wanted, a step apart.
  3. Paste the broken parts back on the first copy into air only (`//gmask air`, then `//paste -a`), so a later copy's block stands over them.
  4. Whiten the labelled outputs and status lines, signed or not, such as a carry out, of every copy but the last, and remove their signs; the next copy takes them over. A monostable's copies keep theirs, as each pink line is its own bit's pulse.
- Each copy pastes over earlier ones, as with `//rstack`: the later block stands, air never replaces anything, and meeting dust joins. A build may rely on this, as a carry out capped by a block the next copy covers.
- Later copies leave out inputs the copy before feeds, with their levers, buttons, and signs: a control line landing within the build on a block or beside redstone that could feed it, such as a carry in's magenta block beside the dust carrying the carry out; and an input block outside the build's core landing in the copy before's core, such as a memory's input layer. A torch on one is logic, such as one inverting a carry in, so it stays where the copy before has a solid block there to hang on, such as the carry out it continues. A control line landing past the copy before, or on air with no redstone near, as a register's own does, stays.
- Copies across a register's or RAM's bits are more registers or words sharing its data buses, driven and read only at the first copy's end, so data inputs and outputs never collide. Only the first copy keeps data levers. A later copy's lime or yellow block landing on a block of the copy before, such as the block or dust its data runs through, is left out with its dust. Its other data blocks are plain, shown white. Other categories' copies keep their own data inputs and outputs. Registers stack west.
- Only the first copy keeps gray blocks and the dust, torches, repeaters, comparators, levers, signs, and barrels on them. Put logic only the least significant bit needs on gray, and a strength it alone takes in a barrel on gray. A recognized upload keeps gray only when its copies stack with gray parts left out of all but the first; otherwise it is whitened, as when its category never stacks. `rsdb add --standardize --keep-gray` keeps it regardless.
- The site finds stacking itself for these categories. A stacking found replaces the builder's west step; none found keeps the builder's.
  - Adder that adds right: the step is its bit spacing carried past the last bit, or for a 1-bit adder the shortest step up, west, or north whose copies add right, carry in to carry out winning a tie. Enough copies must add right that one sits between two others and the stack reaches a copy past dust's 15-block range: a 1-bit slice 2 wide takes 9, an 8-bit adder 16 high takes 3. An adder that stacks gets that step with unlimited copies; one that doesn't gets none. Either replaces any typed stacking.
  - Register: the shortest step west at which two copies store and read right, as more cells or bits, then the most copies that do, up to 16.
  - Buffer: likewise, as more bits only, since its cells show all the time and can't share a bus. A later copy's own write button on the shared write line is left; only the first copy's is pressed.
  - Monostable: as more bits, each pulsing alone, at the shortest step west up to two past its width; a 1-wide one may need a gap.
- Where simulation doesn't find a stacking, the build's page warns at each spot two copies disagree, such as a barrel replaced by concrete, dust cut by a block, or containers giving different strengths. It also says whether two copies make a bigger build of its shape: copies along the bits double each data bus; copies any other way keep each bus's width, or each tap it, and add their own control lines. Neither blocks saving. When copies disagree, the page gives the shortest step that way, short of the build's length, where overlapping copies hold the same blocks.
- A stackable build's page gives in-game steps for each direction: the `//rstack` command and, when it has first-copy-only parts, downloads of the build without them and of those parts alone, pasted back from the same spot with `//paste -a`.

## Saved state

- Every input is saved at logical 0: a normal input lever off, an inverted one lever on.

Builds with their own layout, such as displays, are stored with `--conventional false`, exempt from everything here. An upload that breaks a convention is stored the same way, keeping its own blocks.

## Bounding box

- A stored box fits its blocks tightly. Import removes glass corner markers, such as older builds' 32 glass blocks or WorldEdit's single corner blocks, and trims empty layers.
- Size is that box less any outer layer holding only interface: input blocks with their levers, dust, signs, and torches. So input blocks can sit in their own layer, levers south, without enlarging the build.
- Sizes read height × width × depth, seen from the south: up, west to east, north to south.
- Positions count from the box's north-west-bottom corner.
- Every stored or served file pastes the same way: `//paste` puts the player one block south of the build's south-east-bottom corner, the build reaching west and north in front of them from their feet up. The uploader's copy position is dropped; `rsdb add --keep-paste` keeps it. Either way, cropping, stacking, and corner markers keep the build where it pastes relative to the player.
- A download can add a glass block diagonally off each box corner, so WorldEdit can select the build without the glass touching it.

## Layout proposals

None of these are decided yet.

- Data flows toward +x.
- Inputs enter one face and outputs leave the opposite face.
- Control lines arrive from above.

## Open

- The default orientation and axes.
- Which face a sign goes on.

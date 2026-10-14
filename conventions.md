# Building conventions

Each schematic should follow one default building convention, so builds line up when combined and their names describe them consistently. `rsdb lint` and the upload page check these conventions, and refuse a build that breaks one; [features.md](features.md) lists the checks still to come.

## Block palette

Builds use only whitelisted blocks. Each interface role has its own block, so a block's role can be read from its colour: inputs and outputs are bright colours that stand out against white. Red is left to redstone dust and torches.

| Role         | Block                                   |
| ------------ | --------------------------------------- |
| Solid        | white concrete                          |
| Transparent  | white stained glass                     |
| Inputs A–D   | lime, light blue, cyan, blue concrete   |
| Control line | magenta concrete                        |
| Outputs 1–3  | yellow, orange, brown concrete or glass |
| Status line  | pink concrete or glass                  |
| First copy   | gray concrete or stained glass          |

- Cool colours are inputs and warm colours outputs. Each family runs lightest first: inputs from green to blue, A to D, and outputs from yellow to brown, first to third. Each colour is one bus. Magenta and pink are the exceptions: each magenta block is its own control line, and each pink block its own status line, named by its own sign, unless one sign names a row of them, below.
- An output colour may be stained glass where the output needs a transparent block.
- Every solid block that is not an input or output is white. Whether a block is hard-powered, soft-powered, or a blocker follows from the components around it, so import can derive it rather than read it from a colour. White is the lightest choice, against which both powered and unpowered dust read clearly.
- White stained glass replaces slabs and glowstone wherever a transparent block is needed. Check whether it behaves the same in every place they are used, such as dust placement and one-way upward transmission. Where it doesn't, decide whether that use gets an exception.
- Plain glass is not in the palette, so the WorldEdit corner markers a download can add are never mistaken for part of the build.
- A is green and B is blue, which stay apart under red-green colour blindness.
- Inputs get one colour per operand, not one per power requirement. How each input must be driven is derived from the build; see Inputs.
- Magenta was chosen over orange after an in-game comparison: dust reads better on it when powered and slightly worse when unpowered, and both states count.
- Gray marks logic only the first copy of a stack keeps, such as what a stackable build's least significant bit alone needs; see Stacking. Gray stands out against white, and dust reads on it both powered and unpowered, which red would not.
- Colours are semantic, not aesthetic. Because no two roles share a block, export can recolour each role to a user's palette.

## Inputs

- Each input bit is a block of concrete with a lever on it: lime for every bit of A, light blue for B, cyan for C, blue for D, and magenta for every control line, such as a clock, enable, carry-in, or ALU op line. A further operand, such as the addend of a fused multiply-add, is C.
- All of one operand colour is one input. Unnamed, the operands take letters in colour order, so the first present is A. A sign naming any block of an operand's colour, such as `addend`, names that whole input, so a colour takes one name. Each magenta block is its own control line, named by its own sign.
- A row of control lines, such as a memory's read lines, one per word for a decoder of the builder's own, is named by one sign on any line of the row: the lines run in a straight line, evenly spaced, and are numbered from the one nearest the south-east-bottom corner, as `read 0` to `read 15`. A second sign starts another row, such as the write lines. Pink status lines may form a row the same way. A memory may instead take a binary address, as an operand bus, and decode it itself.
- An adder's lines need no signs, their roles being plain: its one control line, if it has one, is its carry in, and its one status line its carry out. A sign on either must name it so, as `carry in` or `cin`, or `carry out` or `cout`; a control line named anything else, such as `subtract`, makes it a build of another category, such as an ALU. An adder with more than one of either is broken. Every other category signs every line.
- An ALU's control line's sign names the operations it takes part in, separated by spaces and lines, never commas, as `and nand sub`: choosing an operation flips every line that names it from where its lever was saved, so a line saved on goes off. One operation may be named on several lines. The operations are `add`, `sub`, `and`, `or`, `xor`, `nand`, `nor`, `xnor`, `shl`, `shr`, and `asr`; `rsh` and `lsh` read as `shr` and `shl`. A line named `carry in` or `cin` is a carry in, which an add takes. Its status lines are named for what they show: `carry out`, the adder's carry out, for a subtract, A + ~B + 1, on when it borrows nothing; `overflow`, a signed overflow of an add or subtract; `lsb shift out`, the bit a shift right drops; and `zero`, a result of 0. Standardizing takes commas out of every sign.
- An input colour with no lever on it is not allowed, unless it is a signal-strength input; other solid blocks are white. Standardizing keeps an input colour with a lever, as concrete, and input-colour concrete with dust on top, and whitens the rest.
- A control line may use a button instead of a lever, for a pulse such as a reset or a calculate line. Operand bits always use levers.
- An inverted input is saved with its lever on. No torch is needed, and the lever's position marks the inversion.
- The lever touches nothing but its input block. Wiring a build up replaces each lever with a driver pointed into the input block, so the circuit must not rely on the lever powering anything else directly.
- The face the lever is on is the face power enters from, so the lever also marks where a driver goes.
- Every A lever is on the south side of its block, as in the stored adders: the side a player faces north to look at. The other operands' levers may be on any side, north, south, east, or west, but not on top or underneath.

### Signal-strength inputs

- A signal-strength input takes a value from 0 to 15 as its strength, which no lever can give. Each bit is a block of its operand's colour with no lever and dust on top, and each block is one hex digit, four bits of the build's width.
- The dust's one open end, the side that leads nowhere, is where power enters, so it marks where a driver goes, as a lever's face does. It may face any side, unlike A's levers; give each input's dust an open end of its own, so a driver there feeds that input alone.
- One colour is one bus, so an operand's blocks are either all levered or all signal-strength.
- The derived drive and the `4ss` strength sign do not apply: the strength is the value.

### How an input must be driven

Import derives this from the build, so nothing in-world marks it.

- Hard-powered: dust touches the input block, on top, beside, or below. Only a strongly powered block powers dust, so a driver must be a repeater, comparator, or torch.
- Soft-powered: nothing but torches, repeaters, comparators, and mechanisms touches the input block. These respond to a weakly powered block, so any signal into the block works, including dust.
- A lever strongly powers its block at 15, so a saved build always works under hard power. The derivation only decides whether weaker drivers work too.
- Required strength: the input block must be powered at strength 1 or more, unless a sign on one of the operand's input blocks, or the control line's sign, says otherwise, such as `4ss` for 4 or more. The value applies to every bit of the operand.
- Required strength is a minimum, so it does not describe signal-strength inputs, whose strength is their value; see Signal-strength inputs.
- Later, a redstone simulator can derive both facts: drive each input with weak power at strength 1, 2, and so on up to 15, and take the weakest that gives the right outputs. If none does, the input needs hard power. The sign then becomes a check rather than the source.

### Signal strengths

- A fixed signal strength, such as a 1ss, comes from a barrel of redstone dust read by a comparator. A barrel is a solid block and repeats in every copy of a stack.
- Furnaces, cauldrons, and cakes are not allowed. The site swaps each for a barrel giving the same strength whenever it takes in a file, and the upload page says so. A furnace conducts as a barrel does, so the swap changes nothing else.
- A hopper is allowed, but only as a fixed strength: the simulator reads what it holds and moves no items. The upload page offers to swap it for a barrel giving the same strength.
- A strength only a stack's first copy needs, such as a 1ss feeding its carry, comes from a barrel standing on gray; see Stacking.

## Outputs

- Every output bit is yellow, orange, or brown concrete, or the same colour of stained glass where it needs a transparent block. All of one colour is one bus. Unnamed, a lone bus is `output` and several are `output 1`, `output 2`, `output 3` in colour order; a sign naming any block of a colour, such as `low` or `remainder`, names that whole bus. When the buses make one value, as a 16-bit product from two 8-bit halves, yellow holds the lowest bits.
- A status line, such as `carry out`, `overflow`, or `zero`, is pink concrete or glass, each block its own line named by its own sign. A status name on any other output colour is an error. Standardizing turns a block whose sign names a status output pink, as older builds marked them on yellow; `rsdb reread` did so for the stored builds.
- An output bit is on while its block is powered, or while a repeater or comparator standing on it is on, as when the block carries the component that leads the output out of the build.
- Redstone lamps are allowed as ordinary components but do not mark outputs.

## Signs

Signs carry machine-readable labels. Import reads them, so a build's interface can be extracted without a human. Everything else, latency included, is metadata in the database.

| Where              | First line           | Further lines      |
| ------------------ | -------------------- | ------------------ |
| Control line       | its name             | strength, as `4ss` |
| Status line        | its name             |                    |
| Input colour, any  | bus name or strength | strength, as `4ss` |
| Output colour, any | bus name, if any     |                    |
| End of a bus       | `lsb` or `msb`       |                    |

- A status name ends in `out`, as `carry out`, `cout`, or `shift out`, or is `overflow`, `zero`, `negative`, `ready`, or `done`.
- Each further line is one token, such as `4ss`.
- Import copies sign values into the database. After that the database is the source of truth, so export either rewrites signs from it or strips them.

## Bit order

- Each bus's least significant bit is the bit nearest the south-east-bottom corner of the bounding box. The other bits count up from it by distance.
- So a horizontal bus counts up from its east end, and a vertical bus from its bottom.
- A bus that counts the other way says so with a sign reading `lsb` on its least significant end, or `msb` on its most significant one. Either word alone on a line is a mark, not a name, and belongs on an end of the bus.

## Stacking

- A stackable build is saved as one slice: the fewest bits that repeat, such as one bit of a ripple carry adder or a register, or two or four bits of a staggered design.
- A slice may stack along several directions, each with its own step and most copies. Copies count up from the south-east-bottom corner, so they go west, up, north, or a mix, such as west and up for a diagonal build. A register might stack 7 times west for more bits and 3 times north for more registers.
- Copies along the way the build's bits run add bits: west for a horizontal build, up for a vertical one, west and up for a diagonal one. Copies along any other direction are parallel copies and leave the width alone.
- Stacking pastes each copy over the copies before it, as `//rstack` does: where copies overlap, the later copy's block stands, air never replaces anything, and dust that meets joins. A build may rely on that, as a carry out capped by a block the next copy covers.
- Later copies leave out the inputs the copy before feeds: a control line that lands within the build, such as the magenta block feeding a carry in, and an input block outside the build's core that lands in the copy before's core, such as a memory's input layer, with any lever, button, or sign on them. A torch on them is logic, such as one inverting a carry in, so it stays where the copy before has a solid block in their place, such as the carry out it continues, to hang on. A control line that lands past the copy before, as a register's own does, stays.
- Every copy but the last turns its labelled outputs and status lines, such as a carry out, signed or not, white and drops their signs, since the next copy takes them over.
- Only the first copy keeps gray blocks and the dust, torches, repeaters, comparators, levers, signs, and barrels resting on them. Logic the least significant bit alone needs goes on gray, and a signal strength it alone takes, such as a 1ss feeding its carry, comes from a barrel standing on gray; see Signal strengths. To stack in-game with `//rstack`, delete these and the carry in first, then put them back on the first copy. Standardizing an upload whitens gray unless its uploader unticks Whiten gray blocks too (`rsdb add --standardize --keep-gray`).
- The site tests whether an adder that adds right stacks: the step comes from how its bits are spaced, carried on past the last one, or for a 1-bit adder the shortest step up, west, or north whose copies add right, its carry in to its carry out winning a tie, and enough copies must add right together that one sits between two others and the stack reaches a copy past the 15 blocks dust carries a signal, as a 1-bit slice 2 wide takes 9 and an 8-bit adder 16 high takes 3. One that does is taken to stack any number of times, and the site sets an adder's stacking from what it found: that step with no limit on copies, or none, in place of any typed.
- A later copy's solid block landing on a different solid block of the copy before, one of them an input or output block, means the step is wrong; the site's stack check and export refuse it. Any other block a later copy replaces is replaced, as in-game, and the simulation judges whether the copies still add right.

## Saved state

- Every input is saved at logical 0: a normal input with its lever off, an inverted input with its lever on.

Some builds, such as displays, follow their own layout. They are stored with `--conventional false`, which exempts them from everything here.

## Bounding box

- A stored schematic's box fits its blocks tightly. Import takes out any glass corner markers, such as the 32 glass blocks older builds carried or WorldEdit's single corner blocks, and trims every empty layer.
- The build's size is that box, less any outer layer that holds nothing but its interface: input blocks with their levers, dust, signs, and torches. Such a layer only carries the inputs, so it does not make the build bigger. So input blocks can sit in their own layer, with their levers on the south side, without making the build bigger.
- Sizes read height × width × depth, as the build looks from the south: up, then west to east, then north to south.
- Positions count from the box's first block, its north-west-bottom corner.
- Every file the site stores or hands out pastes the same way: `//paste` puts the build so the player stands one block south of its south-east-bottom corner, just outside it, with the build reaching west and north from in front of them, from their feet up, and the position the uploader copied it from is dropped. An uploader may instead keep where their file pastes from (Keep where it pastes from on the upload page, `rsdb add --keep-paste`). Either way, cropping, stacking, and corner markers keep the build where it pastes, relative to the player.
- A download can add a glass block off each corner of the box, diagonally, so WorldEdit can select the build by them without the glass touching it.

## Layout proposals

None of these are decided yet.

- Data flows toward +x.

- Inputs enter one face and outputs leave the opposite face.
- Control lines arrive from above.
- Each build gets a test bench: input levers with the expected outputs, to catch breakage across game versions. The bench should drive each input with the weakest signal its derived requirement allows, since a lever always gives full hard power and would hide a wrong requirement.

## Open

- The default orientation and axes. These also fix the order of `WxHxL` in the filename.
- Which face a sign goes on.

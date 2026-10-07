# Building conventions

Each schematic should follow one default building convention, so builds line up when combined and their names describe them consistently. The planned linter checks these conventions; see [features.md](features.md).

## Block palette

Builds use only whitelisted blocks. Each interface role has its own block, so a block's role can be read from its colour: inputs and outputs are bright colours that stand out against white. Red is left to redstone dust and torches.

| Role        | Block               |
| ----------- | ------------------- |
| Solid       | white concrete      |
| Transparent | white stained glass |
| Input A     | lime concrete       |
| Input B     | light blue concrete |
| Control     | magenta concrete    |
| Output      | yellow concrete     |

- Every solid block that is not an input or output is white. Whether a block is hard-powered, soft-powered, or a blocker follows from the components around it, so import can derive it rather than read it from a colour. White is the lightest choice, against which both powered and unpowered dust read clearly.
- White stained glass replaces slabs and glowstone wherever a transparent block is needed. Check whether it behaves the same in every place they are used, such as dust placement and one-way upward transmission. Where it doesn't, decide whether that use gets an exception.
- Plain glass is kept for the corner markers, so a marker is never mistaken for part of the build.
- A is green and B is blue, which stay apart under red-green colour blindness.
- Inputs get one colour per operand, not one per power requirement. How each input must be driven is derived from the build; see Inputs.
- Magenta was chosen over orange after an in-game comparison: dust reads better on it when powered and slightly worse when unpowered, and both states count.
- Colours are semantic, not aesthetic. Because no two roles share a block, export can recolour each role to a user's palette.

## Inputs

- Each input bit is a block with a lever on it: lime for every bit of A, light blue for every bit of B, magenta for every control line, such as a clock, enable, carry-in, or ALU op line.
- An inverted input is saved with its lever on. No torch is needed, and the lever's position marks the inversion.
- The lever touches nothing but its input block. Wiring a build up replaces each lever with a driver pointed into the input block, so the circuit must not rely on the lever powering anything else directly.
- The face the lever is on is the face power enters from, so the lever also marks where a driver goes.
- Every A and B lever is on the south side of its block, as in the stored adders: the side a player faces north to look at.

### How an input must be driven

Import derives this from the build, so nothing in-world marks it.

- Hard-powered: dust touches the input block, on top, beside, or below. Only a strongly powered block powers dust, so a driver must be a repeater, comparator, or torch.
- Soft-powered: nothing but torches, repeaters, comparators, and mechanisms touches the input block. These respond to a weakly powered block, so any signal into the block works, including dust.
- A lever strongly powers its block at 15, so a saved build always works under hard power. The derivation only decides whether weaker drivers work too.
- Required strength: the input block must be powered at strength 1 or more, unless a sign on one of the operand's input blocks, or the control line's sign, says otherwise, such as `4ss` for 4 or more. The value applies to every bit of the operand.
- Required strength is a minimum, so it does not describe signal-strength inputs, whose strength is their value.
- Later, a redstone simulator can derive both facts: drive each input with weak power at strength 1, 2, and so on up to 15, and take the weakest that gives the right outputs. If none does, the input needs hard power. The sign then becomes a check rather than the source.

## Outputs

- Every output bit is a yellow concrete block. Redstone lamps are allowed as ordinary components but do not mark outputs.

## Signs

Signs carry machine-readable labels. Import reads them, so a build's interface can be extracted without a human. Everything else, latency included, is metadata in the database.

| Where                     | First line         | Further lines           |
| ------------------------- | ------------------ | ----------------------- |
| Each control line         | its name           | strength, such as `4ss` |
| Each single-bit output    | its name           |                         |
| Any input block of A or B | strength, as `4ss` |                         |

- Each further line is one token, such as `4ss`.
- Import copies sign values into the database. After that the database is the source of truth, so export either rewrites signs from it or strips them.

## Bit order

- Each bus's least significant bit is the bit nearest the south-east-bottom corner of the bounding box. The other bits count up from it by distance.
- So a horizontal bus counts up from its east end, and a vertical bus from its bottom. No sign marks the LSB.

## Stacking

- A stackable build is saved as one slice: the fewest bits that repeat, such as one bit of a ripple carry adder or a register, or two or four bits of a staggered design.
- A slice may stack along several directions, each with its own step and most copies. Copies count up from the south-east-bottom corner, so they go west, up, north, or a mix, such as west and up for a diagonal build. A register might stack 7 times west for more bits and 3 times north for more registers.
- Copies along the way the build's bits run add bits: west for a horizontal build, up for a vertical one, west and up for a diagonal one. Copies along any other direction are parallel copies and leave the width alone.
- Stacking keeps every copy's slice, the blocks inside its corner markers. Blocks a copy has in the marker shell that reach into the previous copy's slice belong to the edge of the stack, such as the magenta block, lever, sign, and torch feeding a carry in or a shared control line, so only the first copy keeps them, with any lever or sign on them.
- Every copy but the last turns its labelled outputs, such as a carry out, white and drops their signs, since the next copy takes them over.
- A copy must not overwrite a different block of the copy before. Export refuses a stack whose copies collide, since that means a step is wrong.

## Saved state

- Every input is saved at logical 0: a normal input with its lever off, an inverted input with its lever on.

Some builds, such as displays, follow their own layout. They are stored with `--conventional false`, which exempts them from everything here.

## Corner markers

- The schematic's bounding box has a glass marker at each of its eight corners: the corner block, plus the block next to it along each axis, so 32 glass blocks in all.
- The markers sit in a shell one block thick around the build. The build's size is the bounding box shrunk by one block on every face, and positions count from the first block inside the shell.
- Only a build's interface reaches into the shell: levers, signs, input blocks, and redstone torches standing on or hanging from an input block, such as the torch that drives a control line from its magenta block. Glass in the shell belongs only to the corner markers.
- `rsdb add` refuses a schematic with a marker block that is not glass, or with anything else in the shell.

## Layout proposals

None of these are decided yet.

- Data flows toward +x.

- Inputs enter one face and outputs leave the opposite face.
- Control lines arrive from above.
- Each build gets a test bench: input levers with the expected outputs, to catch breakage across game versions. The bench should drive each input with the weakest signal its derived requirement allows, since a lever always gives full hard power and would hide a wrong requirement.

## Open

- The default orientation and axes. These also fix the order of `WxHxL` in the filename.
- Which face a sign goes on.
- How signal-strength inputs, including `hex` builds, are supplied.

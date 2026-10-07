# Building conventions

Each schematic should follow one default building convention, so builds line up when combined and their names describe them consistently. The planned linter checks these conventions; see [features.md](features.md).

## Block palette

Builds use only whitelisted blocks. Each role has its own block, so a block's role can be read from its colour. Power roles are a grey scale that darkens with the role's strength, and inputs are bright colours that stand out against it. Red is left to redstone dust and torches.

| Role         | Block               |
| ------------ | ------------------- |
| Soft-powered | white concrete      |
| Transparent  | glass               |
| Hard-powered | light gray concrete |
| Blocker      | gray concrete       |
| Input A      | lime concrete       |
| Input B      | light blue concrete |
| Control      | magenta concrete    |
| Output       | redstone lamp       |

- Soft-powered blocks most often carry dust, so they get the lightest colour, against which both powered and unpowered dust read clearly.
- Glass replaces slabs and glowstone wherever a transparent block is needed. Check whether it behaves the same in every place they are used, such as dust placement and one-way upward transmission. Where it doesn't, decide whether that use gets an exception.
- A is green and B is blue, which stay apart under red-green colour blindness.
- Inputs get one colour per operand, not one per power requirement. How each input must be driven is derived from the build; see Inputs.
- Magenta was chosen over orange after an in-game comparison: dust reads better on it when powered and slightly worse when unpowered, and both states count.
- Colours are semantic, not aesthetic. Because no two roles share a block, export can recolour each role to a user's palette.

## Inputs

- Each input bit is a block with a lever on it: lime for every bit of A, light blue for every bit of B, magenta for every control line, such as a clock, enable, carry-in, or ALU op line.
- An inverted input is saved with its lever on. No torch is needed, and the lever's position marks the inversion.
- The lever touches nothing but its input block. Wiring a build up replaces each lever with a driver pointed into the input block, so the circuit must not rely on the lever powering anything else directly.
- Proposal: the face the lever is on is the face power enters from, so the lever also marks where a driver goes.

### How an input must be driven

Import derives this from the build, so nothing in-world marks it.

- Hard-powered: dust touches the input block, on top, beside, or below. Only a strongly powered block powers dust, so a driver must be a repeater, comparator, or torch.
- Soft-powered: nothing but torches, repeaters, comparators, and mechanisms touches the input block. These respond to a weakly powered block, so any signal into the block works, including dust.
- A lever strongly powers its block at 15, so a saved build always works under hard power. The derivation only decides whether weaker drivers work too.
- Required strength: the input block must be powered at strength 1 or more, unless the operand's LSB sign or the control line's sign says otherwise, such as `4ss` for 4 or more. The value applies to every bit of the operand.
- Required strength is a minimum, so it does not describe signal-strength inputs, whose strength is their value.
- Later, a redstone simulator can derive both facts: drive each input with weak power at strength 1, 2, and so on up to 15, and take the weakest that gives the right outputs. If none does, the input needs hard power. The sign then becomes a check rather than the source.

## Outputs

- Every output bit is a redstone lamp, so the result reads at a glance.
- A lamp turns off a short delay after losing power. Account for this when measuring latency.

## Signs

Signs carry machine-readable labels. Import reads them, so a build's interface can be extracted without a human. Everything else is metadata in the database.

| Where             | First line | Further lines           |
| ----------------- | ---------- | ----------------------- |
| LSB of A and of B | `lsb`      | strength, such as `4ss` |
| Each control line | its name   | strength, such as `4ss` |
| Output LSB lamp   | `lsb`      | latency, such as `8t`   |

- Proposal: each further line is one token in the filename's shapes, such as `8t` for latency.
- Import copies sign values into the database. After that the database is the source of truth, so export either rewrites signs from it or strips them.

## Saved state

- Every input is saved at logical 0: a normal input with its lever off, an inverted input with its lever on.

## In-world markers

- Original idea: four markers inside the build, for WorldEdit selection.
- Caveat: a WorldEdit selection is a cuboid defined by two corners, and marker blocks inside the selection paste along with the build.
- Proposal: two corner markers plus an origin marker on the bit 0 input. WorldEdit records the paste offset relative to where the player stands when copying, so always copy from the origin and pastes will align the same way.
- Open: the database already stores the exact bounding box, and the A LSB sign may mark the origin, so markers may not be needed at all.

## Layout proposals

None of these are decided yet.

- Data flows toward +x.
- The LSB sits at a fixed end.
- Inputs enter one face and outputs leave the opposite face.
- Control lines arrive from above.
- Each build gets a test bench: input levers with the expected outputs, to catch breakage across game versions. The bench should drive each input with the weakest signal its derived requirement allows, since a lever always gives full hard power and would hide a wrong requirement.

## Open

- The default orientation and axes. These also fix the order of `WxHxL` in the filename.
- Whether markers are needed, and if so which block and how many.
- Blocker shade: gray, or black for more contrast with light gray. Black hides unpowered dust, but blockers rarely carry dust.
- Which face a sign goes on.
- Lamps are conductive blocks, so a powered lamp can power its neighbours. Decide whether output lamps must be isolated.
- How signal-strength inputs, including `hex` builds, are supplied.

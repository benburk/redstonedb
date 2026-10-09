# Naming

A build's name is its width, design, and category, such as `8-bit Carry Lookahead Adder`. Unknown parts are left out, and so is the design `other`. Its download filename is that name with an underscore for each run of anything but letters and digits, since Schemati, the Open Redstone Engineers server's schematic store, takes no other characters:

```text
8_bit_Carry_Lookahead_Adder.schem
16_bit_RAM.schem
1_bit_NAND_XNOR_Gate.schem
8_bit_Inc_Dec.schem
```

A stack is named for what it became: its width is the slice's times the copies that add bits.

The name does not identify a build; the database's id does. Two builds can share a name, and everything else about them, such as tags, latency, size, builders, and build date, is in the database and on the site.

## Decided

- Filenames are for people downloading a build, not a catalogue. They stay short and readable.
- Categories and designs are stored as lowercase words joined by underscores, such as `carry_lookahead`, and named with their labels, such as `Carry Lookahead` and `RAM`; see [glossary.md](glossary.md).

## Open

- Latency unit: redstone ticks or game ticks.
- Width for multi-operand categories. One proposal: `8-bit` means 8-bit inputs, so an 8-bit Multiplier is 8 × 8.
- Builds without a datapath, such as doors. One proposal: put the opening size in the category name, as in `door_3x3`.

# Glossary

Restrictions and timing are tags in the database. A tag is named in full words, such as `combinational`, and the website and `rsdb show` print that name. A tag may also store a short token of 3 or 4 letters, such as `comb`, which commands and search accept in its place. Each tag has a kind, `restriction`, `timing`, or `other`, which colours its badge; tags a person adds on the site are of the `other` kind.

## Restrictions

Optional. Omitted when the build is unrestricted.

| Token       | Meaning                  |
| ----------- | ------------------------ |
| `alpha`     | To be defined precisely. |
| `torchless` | To be defined precisely. |

Restriction terms mean different things to different people. Before naming many builds with them, write down exactly which components each restriction forbids.

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

## Timing

Optional. The default is whatever the category implies: a register is assumed sequential and an adder combinational. Add a token only when the build departs from that.

| Token  | Tag name        |
| ------ | --------------- |
| `seq`  | `sequential`    |
| `comb` | `combinational` |
| `pipe` | `pipelined`     |

Whether every output bit settles on the same tick is not a tag but a field, `synchronized`, which is yes, no, or unknown. Search finds it with `sync` or `unsync`.

## Categories

The standard categories are a list in the database, shown alphabetically: Adder, ALU, Barrel Shifter, BCD to Binary, Binary to BCD, Binary to Hex, Bussing, Carry Save Adder, Counter, Dataloop, Decoder, Display, Divider, Gate, Hex to Binary, Line Drawer, Multiplier, RAM, Register, Serial Write Memory, Shift Register, and Subtractor. Each is stored by a name in lowercase words joined by underscores, such as `bcd_to_binary`, with the label people read beside it. `rsdb categories` shows the list, and `rsdb define-category` adds to its end, with `--label` when the label is not just the name's words capitalized. A category can also let each build give its own name, as Bussing and Display do: a build named Bus Crossing shows as, say, 8-bit Bus Crossing (`rsdb define-category --named`, and `--name` on `rsdb add` and `set`). A build of any other kind names its category freely, in words, numbers, and hyphens such as `threshold_3` or `8-bit_horizontal_diode`, and shows it capitalized: Threshold 3. Such a build has no width or encoding.

Every build has a design: `other`, unless its category lists a better one (`rsdb define-design`). An adder's designs:

| Design                  | Short |
| ----------------------- | ----- |
| `ripple_carry`          | RCA   |
| `carry_lookahead`       | CLA   |
| `carry_cancel`          | CCA   |
| `carry_look_everywhere` | CLE   |

Gates have XOR, XNOR, NAND+XNOR, and NAND+NOR; the simpler gates are left out. A design whose label is not just its name's words capitalized, such as XOR for `xor`, stores that label (`rsdb define-design --label`).

The design is part of a build's name, as in "8-bit Carry Lookahead Adder", and so of its filename; see [naming.md](naming.md). A category may also have flags, yes-or-no facts its builds record: adders and subtractors have Carry in and Carry out, set from signs labelled `carry in` or `cin` and `carry out` or `cout`, shift registers have Bidirectional and Writeable, and multipliers have Upper product, for one that outputs the full product, as 16 bits from 8 by 8, and FMA, for a fused multiply-add, A×B+C.

A threshold gate turns on when at least *k* of its inputs are on; `threshold_3` turns on for at least 3. Popcount, by contrast, outputs how many inputs are on. Majority is the threshold at half the inputs.

Add new standard categories here when adding them to the database.

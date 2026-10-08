# Glossary

Restrictions and timing are tags in the database. A tag is named in full words, such as `combinational`, and the website and `rsdb show` print that name. A tag may also store a short token of 3 or 4 letters, such as `comb`, which commands and search accept in its place. Tags a person adds while uploading a build are created under `other`.

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

Optional. The default is whatever the function implies: a register is assumed sequential and an adder combinational. Add a token only when the build departs from that.

| Token  | Tag name        |
| ------ | --------------- |
| `seq`  | `sequential`    |
| `comb` | `combinational` |
| `pipe` | `pipelined`     |

Whether every output bit settles on the same tick is not a tag but a field, `synchronized`, which is yes, no, or unknown. Search finds it with `sync` or `unsync`.

## Functions

The standard functions are a list in the database, in this order: Adder, Subtractor, Multiplier, Divider, Register, Serial Write Memory, RAM, Dataloop, ALU, BCD to Binary, Binary to BCD, Counter, Shift Register, Barrel Shifter, Carry Save Adder, Display, Line Drawer, and Decoder. Each is stored by a name in lowercase words joined by underscores, such as `bcd_to_binary`, with the label people read beside it. `rsdb functions` shows the list, and `rsdb define-function` adds to its end, with `--label` when the label is not just the name's words capitalized. A build of any other kind names its function freely, such as `threshold_3`, and shows it capitalized: Threshold 3.

Every build has a design: `other`, unless its function lists a better one (`rsdb define-design`). An adder's designs:

| Design                  | Short |
| ----------------------- | ----- |
| `ripple_carry`          | RCA   |
| `carry_lookahead`       | CLA   |
| `carry_cancel`          | CCA   |
| `carry_look_everywhere` | CLE   |

The design is part of a build's name, as in "8-bit Carry Lookahead Adder", and so of its filename; see [naming.md](naming.md). Adders and subtractors also record whether they have a carry in and a carry out, read from signs labelled `carry in` or `cin` and `carry out` or `cout`.

A threshold gate turns on when at least *k* of its inputs are on; `threshold_3` turns on for at least 3. Popcount, by contrast, outputs how many inputs are on. Majority is the threshold at half the inputs.

Add new standard functions here when adding them to the database.

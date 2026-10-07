# Glossary

Modifiers appear in the filename in the order of this file's sections, alphabetical within a section. Orientation and encoding are always written. Restrictions and timing are written only when they apply.

Abbreviated tokens use 3 or 4 letters. Restrictions are written out in full.

Restrictions and timing are tags in the database. A tag is named in full words, such as `combinational`, and the website and `rsdb show` print that name. A tag whose filename token is an abbreviation stores the token beside its name.

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

Whether every output bit settles on the same tick is not a tag but a field, `synchronized`, which is yes, no, or unknown. The filename writes it last among the timing tokens: `sync` for yes, `unsync` for no, and nothing when unknown.

## Functions

`adder`, `multiplier`, `dataloop`, `register`, `alu`, `line_drawer`, `door`, `decoder`, `bin2bcd`, `maze_screen`.

Add new functions here before using them in a filename.

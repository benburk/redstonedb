# Naming

## Grammar

```text
<function>-<width>-<modifiers…>-<latency>-<size>-<author>-<date>.schem
```

Dashes separate parts, so no part may contain a dash. Multi-word function names use underscores, such as `line_drawer`. Each part has a distinct shape, so a name can be parsed without placeholders for omitted parts.

| Part      | Shape           | Example          |
| --------- | --------------- | ---------------- |
| function  | snake_case word | `bin2bcd`        |
| width     | `NNb`           | `08b`            |
| modifiers | glossary tokens | `alpha-horz-bin` |
| latency   | `Nt`            | `3t`             |
| size      | `WxHxL`         | `16x4x9`         |
| author    | in-game name    | `cullback`       |
| date      | `YYYYMMDD`      | `20261007`       |

- **function:** always present. It comes first so builds group by function.
- **width:** zero-padded to two digits so `08b` sorts before `16b`.
- **modifiers:** in glossary order. Orientation and encoding are required.
- **latency:** ticks; the unit is still open.
- **size:** the full bounding box. The axes follow the building convention.
- **author:** lowercase. Minecraft names use only `[A-Za-z0-9_]`.
- **date:** serves as the version.

## Examples

These are illustrative only; they are not real builds.

```text
adder-08b-alpha-vert-bin-3t-16x4x9-cullback-20261007.schem
bin2bcd-08b-horz-hex-5t-16x5x12-someone-20261007.schem
register-16b-diag-bin-2t-32x6x4-someone-20261007.schem
```

## Decided

- Width, latency, and size go in the filename.
- Stackability follows from the orientation, and the bit pitch is almost always 2 wide or follows from the function, so neither gets its own part.
- Orientation and encoding are always written; restrictions and timing only when they apply.
- Abbreviated tokens use 3 or 4 letters. Restrictions are written out in full.
- The author and a date together identify the version.

## Open

- Latency unit: redstone ticks or game ticks.
- Throughput or period for pipelined and sequential builds: in the filename, or only in the database.
- Which date to use: the build date, or the capture date when the build date is unknown.
- Width for multi-operand functions. One proposal: `08b` means 8-bit inputs, so an `08b` multiplier is 8 × 8.
- Builds without a datapath, such as doors. One proposal: put the opening size in the function name, as in `door_3x3`.

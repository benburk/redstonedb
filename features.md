# Planned features

None of these are built yet. The linter and import extraction they extend are described in the [README](README.md).

## Lint checks

- Signs appear only on control lines, single-bit outputs, and as strength tokens on A and B.
- Every lever is saved off, except on inverted inputs, and every button is saved unpressed.

Most role checks need a model of which blocks are powered, so they come after the position checks.

## Export options

`rsdb export` and the download dialog rewrite the schematic before handing it over:

- Recolour each role to a user's palette, such as `--palette mine.toml`.
- Keep or strip the signs, or rewrite them from the database.

Because roles are encoded by block, this is a palette substitution, not a change to the build.

## Open

- Where user palettes live: a file passed on the command line, or stored in the database.

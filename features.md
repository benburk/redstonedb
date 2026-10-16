# Planned features

None of these are built yet. The checks they extend are in `src/lint.rs`, and the export they extend is `rsdb export` and the build page's download dialog.

## Lint checks

- Signs appear only on control lines, single-bit outputs, and as strength tokens on A and B.
- Every lever is saved off, except on inverted inputs, and every button is saved unpressed.

Most role checks need a model of which blocks are powered, so they come after the position checks.

## Export options

`rsdb export` and the download dialog rewrite the schematic before handing it over:

- Recolour each role to a user's palette, such as `--palette mine.toml`.
- Keep or strip the signs, or rewrite them from the database.

Because roles are encoded by block, this is a palette substitution, not a change to the build.

## Fields

The database is the source of truth for metadata; signs carry only machine-readable labels. `migrations/` defines the fields stored today. These candidate fields are not stored yet:

- Edition, and any mechanics the build relies on, such as quasi-connectivity or update order. The oldest Java Edition release it works in is already read from the file.
- Status, working, buggy, or work in progress, for a build of a category the simulator does not test.
- Source URL, and permission to use builds by other people.

## Open

- Where user palettes live: a file passed on the command line, or stored in the database.
- Which candidate fields are required.

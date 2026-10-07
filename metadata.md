# Metadata

The database is the source of truth for metadata; signs carry only machine-readable labels. `src/schema.sql` defines the fields stored today. These candidate fields are not stored yet.

## Candidate fields

- Game version and edition, plus any mechanics the build relies on, such as quasi-connectivity or update order.
- I/O spec: pin locations, LSB end, output polarity, and the meaning of each control line. The LSB markers and output lamps give part of this.
- Throughput or period, for pipelined and sequential builds.
- Status: working, buggy, or work in progress.
- Source URL, and permission to use builds by other people.
- Free-text description.

## Open

- Which fields become columns and which become tags.
- Which fields are required.

-- What simulating a build found, for the file it was run on; see
-- src/simulate.rs. A build whose category and width have no test has no
-- row.
CREATE TABLE simulation (
    schematic_id TEXT NOT NULL PRIMARY KEY
        REFERENCES schematic (id) ON DELETE CASCADE,
    -- The test run, such as `adder`.
    test TEXT NOT NULL CHECK (test <> ''),
    -- The file's hash when it ran, so a changed file is run again.
    sha256 TEXT NOT NULL,
    -- `simulate::Report` as JSON.
    report TEXT NOT NULL CHECK (json_valid(report)),
    ran_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

-- A build of a function off the list, such as a display, has no encoding; a
-- function's name may start with a digit, as in `4bit_decoder`; and an
-- importer's name keeps its capitals. SQLite cannot loosen a column's check
-- in place, so the table is rebuilt. The migration runs in one transaction with
-- foreign keys on, so the rows that hang on each build are kept aside first.
CREATE TEMP TABLE kept_tag AS SELECT * FROM schematic_tag;
CREATE TEMP TABLE kept_credit AS SELECT * FROM credit;
CREATE TEMP TABLE kept_stack_direction AS SELECT * FROM stack_direction;

CREATE TABLE schematic_new (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    function TEXT NOT NULL CHECK (
        function <> '' AND function NOT GLOB '*[^a-z0-9_]*'
    ),
    width INTEGER CHECK (width > 0),
    orientation TEXT NOT NULL CHECK (orientation IN ('horz', 'vert', 'diag')),
    encoding TEXT CHECK (encoding IN ('bin', 'hex')),
    latency INTEGER CHECK (latency >= 0),
    size_x INTEGER NOT NULL CHECK (size_x > 0),
    size_y INTEGER NOT NULL CHECK (size_y > 0),
    size_z INTEGER NOT NULL CHECK (size_z > 0),
    built_on TEXT CHECK (
        built_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    sha256 TEXT NOT NULL UNIQUE,
    data BLOB NOT NULL,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    synchronized INTEGER CHECK (synchronized IN (0, 1)),
    imported_by TEXT CHECK (
        imported_by <> '' AND imported_by NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    conventional INTEGER NOT NULL DEFAULT 1 CHECK (conventional IN (0, 1)),
    description TEXT CHECK (description <> ''),
    uploaded_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    design TEXT,
    carry_in INTEGER CHECK (carry_in IN (0, 1)),
    carry_out INTEGER CHECK (carry_out IN (0, 1))
) STRICT;

INSERT INTO schematic_new (
    id, function, width, orientation, encoding, latency, size_x, size_y,
    size_z, built_on, sha256, data, added_at, synchronized, imported_by,
    conventional, description, uploaded_by, design, carry_in, carry_out
)
SELECT
    id, function, width, orientation, encoding, latency, size_x, size_y,
    size_z, built_on, sha256, data, added_at, synchronized, imported_by,
    conventional, description, uploaded_by, design, carry_in, carry_out
FROM schematic;

-- Dropping the old table deletes its rows, and with them every tag, credit,
-- and stacking row by cascade, so those are put back once the new table has
-- the name.
DROP TABLE schematic;
ALTER TABLE schematic_new RENAME TO schematic;

INSERT INTO schematic_tag SELECT * FROM kept_tag;
INSERT INTO credit SELECT * FROM kept_credit;
INSERT INTO stack_direction SELECT * FROM kept_stack_direction;
DROP TABLE kept_tag;
DROP TABLE kept_credit;
DROP TABLE kept_stack_direction;

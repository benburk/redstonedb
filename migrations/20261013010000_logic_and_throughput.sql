-- Whether a build's logic is combinational or sequential is a column, not
-- a tag: every build is one or the other. A category gives its builds'
-- usual logic, and a build that differs, such as a sequential multiplier,
-- says so itself; NULL follows its category, and a category's NULL means
-- it has no usual logic.
ALTER TABLE category ADD COLUMN logic TEXT CHECK (logic IN ('comb', 'seq'));
ALTER TABLE schematic ADD COLUMN logic TEXT CHECK (logic IN ('comb', 'seq'));

UPDATE category SET logic = 'comb' WHERE name IN (
    'adder', 'subtractor', 'carry_save_adder', 'multiplier', 'divider', 'alu',
    'popcount', 'incrementer', 'gate', 'threshold', 'decoder',
    'barrel_shifter', 'bcd_to_binary', 'binary_to_bcd', 'binary_to_hex',
    'hex_to_binary'
);
UPDATE category SET logic = 'seq' WHERE name IN (
    'ram', 'register', 'shift_register', 'serial_write_memory', 'dataloop',
    'counter', 'rng', 'serial_adder'
);

-- The timing tags become the column. Pipelined is no logic of its own: a
-- pipelined build is combinational, and is known by its throughput.
UPDATE schematic SET logic = 'seq' WHERE id IN (
    SELECT schematic_id FROM schematic_tag
    JOIN tag ON tag.id = schematic_tag.tag_id
    WHERE tag.name = 'sequential'
);
UPDATE schematic SET logic = 'comb' WHERE id IN (
    SELECT schematic_id FROM schematic_tag
    JOIN tag ON tag.id = schematic_tag.tag_id
    WHERE tag.name IN ('combinational', 'pipelined')
);
DELETE FROM tag WHERE kind = 'timing';

-- A tag's kind loses timing. The tables are rebuilt to change the check,
-- the links first, so dropping the old tags takes none of them along.
CREATE TABLE tag_new (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL UNIQUE CHECK (
        name <> '' AND name NOT GLOB '*[^a-z0-9_]*'
    ),
    -- Which colours its badge.
    kind TEXT NOT NULL CHECK (kind IN ('restriction', 'other')),
    -- A short form that commands and search accept in its place.
    token TEXT UNIQUE CHECK (token <> '' AND token NOT GLOB '*[^a-z0-9_]*')
) STRICT;
INSERT INTO tag_new (id, name, kind, token)
SELECT id, name, kind, token FROM tag;

CREATE TABLE schematic_tag_new (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tag_new (id) ON DELETE CASCADE,
    PRIMARY KEY (schematic_id, tag_id)
) STRICT;
INSERT INTO schematic_tag_new (schematic_id, tag_id)
SELECT schematic_id, tag_id FROM schematic_tag;

DROP TABLE schematic_tag;
DROP TABLE tag;
ALTER TABLE tag_new RENAME TO tag;
ALTER TABLE schematic_tag_new RENAME TO schematic_tag;

-- A build's throughput is the fewest ticks from one input to the next that
-- still gives right outputs; any build may have one. A serial build's input
-- is one bit, so its throughput is the ticks from one bit to the next, as
-- its period was. A combinational, synchronized build whose throughput is
-- below its latency is pipelined.
ALTER TABLE schematic RENAME COLUMN period TO throughput;

-- A serial category's builds take their operands a bit at a time, so their
-- throughput, not their width, leads their name and splits their boards.
ALTER TABLE category RENAME COLUMN periodic TO serial;

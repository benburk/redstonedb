-- The stored thing is a build, as the code and the site call it, and a
-- schematic is its file: the tables and columns named for a schematic are
-- named for a build. Nothing else of them changes, their rows staying where
-- they are.
--
-- A build not recognized as the category it was meant as stays under that
-- category, unverified, as an upload has gone up for some time. Before
-- that, such a build went up under a name of its own made from it, such as
-- `8-bit_adder`, or `unrecognized_adder` when its width could not be read,
-- which the code read back; those move to their categories here.
--
-- The welcome steps are gone, so no account keeps whether it saw them.
--
-- No build has a capacity or a name of its own: only categories the site
-- never recognized a build of took either, and those are gone from the
-- list, so the columns go.

DROP VIEW current_file;

ALTER TABLE schematic RENAME TO build;
ALTER TABLE schematic_version RENAME TO build_version;
ALTER TABLE schematic_flag RENAME TO build_flag;
ALTER TABLE schematic_tag RENAME TO build_tag;

ALTER TABLE build_version RENAME COLUMN schematic_id TO build_id;
ALTER TABLE build_flag RENAME COLUMN schematic_id TO build_id;
ALTER TABLE build_tag RENAME COLUMN schematic_id TO build_id;
ALTER TABLE credit RENAME COLUMN schematic_id TO build_id;
ALTER TABLE stack_direction RENAME COLUMN schematic_id TO build_id;
ALTER TABLE stacked_width RENAME COLUMN schematic_id TO build_id;
ALTER TABLE favorite RENAME COLUMN schematic_id TO build_id;
ALTER TABLE simulation RENAME COLUMN schematic_id TO build_id;
ALTER TABLE similarity RENAME COLUMN schematic_id TO build_id;
ALTER TABLE download RENAME COLUMN schematic_id TO build_id;
ALTER TABLE edit RENAME COLUMN schematic_id TO build_id;
ALTER TABLE deletion RENAME COLUMN schematic_id TO build_id;

-- An index keeps the name it was made with, so each named for a schematic
-- is made again under its new one.
DROP INDEX schematic_uploaded_by;
DROP INDEX schematic_version_added_by;
DROP INDEX schematic_version_sha256;
DROP INDEX schematic_version_added_at;
DROP INDEX download_by_schematic;
DROP INDEX favorite_schematic_id;
DROP INDEX deletion_schematic_id;

CREATE INDEX build_uploaded_by ON build (uploaded_by);
CREATE INDEX build_version_added_by ON build_version (added_by);
CREATE INDEX build_version_added_at ON build_version (added_at);
-- No two builds on the site hold the same file, which the site checks as
-- it stores one. The index is not unique, as a deleted build keeps its
-- file, for an admin to restore it, and the same file may be uploaded
-- again.
CREATE INDEX build_version_sha256 ON build_version (sha256);
CREATE INDEX download_by_build ON download (build_id, at);
CREATE INDEX favorite_build_id ON favorite (build_id);
CREATE INDEX deletion_build_id ON deletion (build_id);

-- Each build's newest version, its current file.
CREATE VIEW current_file AS
SELECT build_id, version, data, sha256, roles FROM build_version
WHERE version = (
    SELECT max(version) FROM build_version AS newest
    WHERE newest.build_id = build_version.build_id
);

-- The categories the site recognizes builds of, as the code lists them
-- now.
CREATE TEMP TABLE recognized (name TEXT NOT NULL PRIMARY KEY);
INSERT INTO recognized (name) VALUES
    ('adder'), ('hex_adder'), ('subtractor'), ('inverted_adder'), ('alu'),
    ('multiplier'), ('incrementer'), ('inc_dec'), ('decrementer'),
    ('equality'), ('greater_than'), ('carry_out'), ('absolute_value'),
    ('negator'), ('popcount'), ('threshold'), ('parity'), ('binary_to_hex'),
    ('hex_to_binary'), ('xor_gate'), ('xnor_gate'), ('and_gate'),
    ('nand_gate'), ('or_gate'), ('nor_gate'), ('nand_xnor_gate'),
    ('inverter'), ('decoder'), ('barrel_shifter'), ('monostable'),
    ('dual_edge_monostable'), ('register'), ('buffer'), ('counter'),
    ('shift_register'), ('t_flip_flop');

-- `8-bit_adder` is an adder of 8 bits, its width kept when it has none. A
-- reason stored already stays; a build with none takes the one a build
-- kept as built has, until `rsdb refresh` judges it again and says what
-- keeps it from being recognized.
UPDATE build SET
    category = substr(category, instr(category, '-bit_') + 5),
    width = coalesce(width, nullif(CAST(category AS INTEGER), 0)),
    unverified = coalesce(unverified, 'Couldn''t read its inputs and outputs')
WHERE category GLOB '[0-9]*-bit_?*'
    AND substr(category, 1, instr(category, '-bit_') - 1) NOT GLOB '*[^0-9]*'
    AND substr(category, instr(category, '-bit_') + 5)
        IN (SELECT name FROM recognized);

UPDATE build SET
    category = substr(category, length('unrecognized_') + 1),
    unverified = coalesce(unverified, 'Couldn''t read its inputs and outputs')
WHERE category GLOB 'unrecognized_?*'
    AND substr(category, length('unrecognized_') + 1)
        IN (SELECT name FROM recognized);

DROP TABLE recognized;

ALTER TABLE user DROP COLUMN welcomed;
ALTER TABLE build DROP COLUMN bytes;
ALTER TABLE build DROP COLUMN given_name;

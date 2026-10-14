-- A category is one function: two builds share one only when either could
-- stand in for the other, the same inputs giving the same outputs. A design
-- is only ever how a build works, so every category's designs compete on
-- one leaderboard. A gate's kind and what an incrementer does were designs,
-- and become categories of their own; a threshold's k was a design, and
-- becomes a number, as width is.
--
-- The numbers that lead a build's name and split its leaderboards, such as
-- width, are its category's measures, in order. They replace a category's
-- serial and holds_bytes switches.
--
-- The category table is rebuilt to drop the switches and to add the gates
-- section, which its check names. The design and flag tables, which point
-- at it, are rebuilt first to point at the new one, so dropping the old
-- one takes none of their rows along.
CREATE TABLE category_new (
    name TEXT NOT NULL PRIMARY KEY CHECK (
        name <> '' AND name NOT GLOB '*[^-a-z0-9_]*'
    ),
    -- As people read it, such as RAM.
    label TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (label <> ''),
    -- For many builds, such as ALUs; NULL for the label with an s.
    plural TEXT CHECK (plural <> ''),
    -- Whether each build gives its own name, as bussing's do.
    named INTEGER NOT NULL DEFAULT 0 CHECK (named IN (0, 1)),
    -- The sidebar's heading it sits under; NULL puts it with the categories
    -- off the list, under Other.
    section TEXT CHECK (section IN (
        'arithmetic', 'logic', 'gates', 'conversion', 'memory', 'display',
        'wiring'
    )),
    -- Its builds' usual logic; NULL for none.
    logic TEXT CHECK (logic IN ('comb', 'seq'))
) STRICT;

INSERT INTO category_new (name, label, plural, named, section, logic)
SELECT name, label, plural, named, section, logic FROM category;

-- A design within a category, such as the carry lookahead adder: how a
-- build works. Every build's design is one of these or `other`.
CREATE TABLE design_new (
    category TEXT NOT NULL REFERENCES category_new (name) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    -- The usual short name, such as CLA; NULL when there is none.
    abbreviation TEXT CHECK (abbreviation <> ''),
    -- How people read it, such as LFSR; NULL for the name's words.
    label TEXT CHECK (label <> ''),
    PRIMARY KEY (category, name)
) STRICT;
INSERT INTO design_new (category, name, abbreviation, label)
SELECT category, name, abbreviation, label FROM design;

-- A yes-or-no fact a category's builds record, such as an adder's carry in
-- or a shift register being bidirectional.
CREATE TABLE flag_new (
    category TEXT NOT NULL REFERENCES category_new (name) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    label TEXT NOT NULL CHECK (label <> ''),
    -- Its place among its category's flags.
    position INTEGER NOT NULL,
    PRIMARY KEY (category, name)
) STRICT;
INSERT INTO flag_new (category, name, label, position)
SELECT category, name, label, position FROM flag;

-- What the switches said, kept until the measures below are made from it.
CREATE TEMP TABLE switches AS
SELECT name, serial, holds_bytes FROM category;

DROP TABLE flag;
DROP TABLE design;
DROP TABLE category;
ALTER TABLE category_new RENAME TO category;
ALTER TABLE design_new RENAME TO design;
ALTER TABLE flag_new RENAME TO flag;

-- The numbers a category's builds are named and ranked by, in order: width
-- in bits, a memory's capacity in bytes, a serial build's throughput in
-- ticks, and a threshold's k, the fewest inputs on that turn it on.
CREATE TABLE category_measure (
    category TEXT NOT NULL REFERENCES category (name) ON DELETE CASCADE,
    measure TEXT NOT NULL CHECK (
        measure IN ('width', 'bytes', 'throughput', 'at_least')
    ),
    -- Its place among its category's measures.
    position INTEGER NOT NULL,
    PRIMARY KEY (category, measure)
) STRICT;

INSERT INTO category_measure (category, measure, position)
SELECT name, iif(serial, 'throughput', 'width'), 1 FROM temp.switches;
INSERT INTO category_measure (category, measure, position)
SELECT name, 'bytes', 2 FROM temp.switches WHERE holds_bytes;
DROP TABLE temp.switches;

-- A threshold's k, as in an at-least-3 threshold; NULL when unknown.
ALTER TABLE schematic ADD COLUMN at_least INTEGER CHECK (at_least > 0);

INSERT INTO category_measure (category, measure, position)
VALUES ('threshold', 'at_least', 2);

UPDATE schematic SET
    at_least = CAST(substr(design, length('at_least_') + 1) AS INTEGER),
    design = 'other'
WHERE category = 'threshold' AND design GLOB 'at_least_[0-9]*';

-- An incrementer, a decrementer, and one that does either as a control
-- line says compute different things.
INSERT INTO category (name, label, plural, section, logic) VALUES
    ('decrementer', 'Decrementer', NULL, 'arithmetic', 'comb'),
    ('inc_dec', 'Inc/Dec', 'Inc/Dec', 'arithmetic', 'comb');

-- The gates, each its own function, under a section of their own.
INSERT INTO category (name, label, plural, section, logic) VALUES
    ('xor_gate', 'XOR Gate', NULL, 'gates', 'comb'),
    ('xnor_gate', 'XNOR Gate', NULL, 'gates', 'comb'),
    ('nand_xnor_gate', 'NAND+XNOR Gate', NULL, 'gates', 'comb'),
    ('nand_nor_gate', 'NAND+NOR Gate', NULL, 'gates', 'comb');

INSERT INTO category_measure (category, measure, position) VALUES
    ('decrementer', 'width', 1),
    ('inc_dec', 'width', 1),
    ('xor_gate', 'width', 1),
    ('xnor_gate', 'width', 1),
    ('nand_xnor_gate', 'width', 1),
    ('nand_nor_gate', 'width', 1);

-- A moved build's simulation was of its old category's test, so it runs
-- again.
DELETE FROM simulation WHERE schematic_id IN (
    SELECT id FROM schematic
    WHERE (category = 'incrementer' AND design IN ('decrement', 'inc_dec'))
        OR (category = 'gate'
            AND design IN ('xor', 'xnor', 'nand_xnor', 'nand_nor'))
);

UPDATE schematic SET category = 'decrementer', design = 'other'
WHERE category = 'incrementer' AND design = 'decrement';
UPDATE schematic SET category = 'inc_dec', design = 'other'
WHERE category = 'incrementer' AND design = 'inc_dec';
UPDATE schematic SET design = 'other'
WHERE category = 'incrementer' AND design = 'increment';
UPDATE schematic SET category = design || '_gate', design = 'other'
WHERE category = 'gate' AND design IN ('xor', 'xnor', 'nand_xnor', 'nand_nor');

-- The gate category goes, with its designs; the incrementer's and the
-- threshold's designs said what the build computes, and go too.
DELETE FROM category WHERE name = 'gate';
DELETE FROM design WHERE category IN ('incrementer', 'threshold');

-- Other words people search a category by, such as connect4 for a
-- four-in-a-row detector, as search terms are typed: lowercase, with
-- spaces. A category off the list may have them too.
CREATE TABLE category_alias (
    alias TEXT NOT NULL CHECK (alias <> '' AND alias NOT GLOB '*[^a-z0-9 ]*'),
    category TEXT NOT NULL CHECK (
        category <> '' AND category NOT GLOB '*[^-a-z0-9_]*'
    ),
    PRIMARY KEY (alias, category)
) STRICT;

INSERT INTO category_alias (alias, category) VALUES
    ('connect4', 'four_in_a_row_detector'),
    ('connect 4', 'four_in_a_row_detector'),
    ('xor', 'xor_gate'),
    ('xnor', 'xnor_gate'),
    ('gate', 'xor_gate'),
    ('gate', 'xnor_gate'),
    ('gate', 'nand_xnor_gate'),
    ('gate', 'nand_nor_gate'),
    ('inc', 'incrementer'),
    ('dec', 'decrementer'),
    ('bcd', 'bcd_to_binary'),
    ('bcd', 'binary_to_bcd'),
    ('csa', 'carry_save_adder'),
    ('majority', 'threshold');

-- Anything left pointing nowhere fails the check and stops the migration.
CREATE TEMP TABLE dangling (count INTEGER CHECK (count = 0));
INSERT INTO temp.dangling SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE temp.dangling;

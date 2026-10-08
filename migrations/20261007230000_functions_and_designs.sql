-- A build's function comes from a list rather than free text, and some
-- functions name their designs: an adder is a ripple carry, carry
-- lookahead, carry cancel, or carry look everywhere design, or another.
CREATE TABLE function (
    name TEXT PRIMARY KEY CHECK (
        name GLOB '[a-z]*' AND name NOT GLOB '*[^a-z0-9_]*'
    ),
    -- Whether a build of this function must name its design.
    design_required INTEGER NOT NULL DEFAULT 0
        CHECK (design_required IN (0, 1)),
    -- Whether its builds have carry lines worth recording.
    carries INTEGER NOT NULL DEFAULT 0 CHECK (carries IN (0, 1))
) STRICT;

INSERT INTO function (name, design_required, carries) VALUES
    ('adder', 1, 1),
    ('subtractor', 0, 1),
    ('multiplier', 0, 0),
    ('divider', 0, 0),
    ('register', 0, 0);
INSERT OR IGNORE INTO function (name) SELECT DISTINCT function FROM schematic;

CREATE TABLE design (
    function TEXT NOT NULL REFERENCES function (name),
    name TEXT NOT NULL CHECK (
        name GLOB '[a-z]*' AND name NOT GLOB '*[^a-z0-9_]*'
    ),
    -- The usual short name, such as CLA; NULL when there is none.
    abbreviation TEXT,
    PRIMARY KEY (function, name)
) STRICT;

INSERT INTO design (function, name, abbreviation) VALUES
    ('adder', 'ripple_carry', 'RCA'),
    ('adder', 'carry_lookahead', 'CLA'),
    ('adder', 'carry_cancel', 'CCA'),
    ('adder', 'carry_look_everywhere', 'CLE'),
    ('adder', 'other', NULL);

-- NULL design: none named. NULL carry: unknown, or the function has none.
ALTER TABLE schematic ADD COLUMN design TEXT;
ALTER TABLE schematic ADD COLUMN carry_in INTEGER CHECK (carry_in IN (0, 1));
ALTER TABLE schematic ADD COLUMN carry_out INTEGER
    CHECK (carry_out IN (0, 1));

-- The design tags become the design field.
UPDATE schematic SET design = (
    SELECT tag.name FROM schematic_tag
    JOIN tag ON tag.id = schematic_tag.tag_id
    WHERE schematic_tag.schematic_id = schematic.id
        AND tag.name IN ('ripple_carry', 'carry_lookahead', 'carry_cancel')
);
DELETE FROM schematic_tag WHERE tag_id IN (
    SELECT id FROM tag
    WHERE name IN ('ripple_carry', 'carry_lookahead', 'carry_cancel')
);
DELETE FROM tag
WHERE name IN ('ripple_carry', 'carry_lookahead', 'carry_cancel');

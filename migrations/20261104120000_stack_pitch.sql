-- Copies stack west or up, never north, and copies that add bits are
-- classed by their pitch: the step from one copy to the next over the bits
-- each adds, 1, 2, or 4, the only spacings tested. The one stored stack
-- north, of a RAM off the list, goes, and so do decoders' stacks, as
-- copies of a decoder side by side are separate decoders.

CREATE TABLE stack_direction_new (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    west INTEGER NOT NULL DEFAULT 0 CHECK (west >= 0),
    up INTEGER NOT NULL DEFAULT 0 CHECK (up >= 0),
    -- The most copies that work; NULL for no limit.
    max_copies INTEGER CHECK (max_copies >= 2),
    -- The box later copies repeat, when testing copies chose it over the
    -- one the box rule finds: the layers it leaves out on each face, as
    -- `{"low": [west, bottom, north], "high": [east, top, south]}`. NULL for
    -- the box rule's.
    faces TEXT CHECK (faces IS NULL OR json_valid(faces)),
    -- For copies the simulation found add bits, the class of their pitch,
    -- the step over the bits each copy adds: 1, 2, or 4, a build of a
    -- tighter pitch counting in each looser class too. NULL for copies that
    -- add words on a shared bus, as a register's across its bits, and for
    -- a stack its builder gave.
    pitch INTEGER CHECK (pitch IN (1, 2, 4)),
    CHECK (west + up > 0),
    PRIMARY KEY (schematic_id, west, up)
) STRICT;

INSERT INTO stack_direction_new (schematic_id, west, up, max_copies, faces)
SELECT schematic_id, west, up, max_copies, faces FROM stack_direction
WHERE north = 0 AND schematic_id NOT IN (
    SELECT id FROM schematic WHERE category = 'decoder'
);

DROP TABLE stack_direction;

ALTER TABLE stack_direction_new RENAME TO stack_direction;

DELETE FROM stacked_width WHERE schematic_id IN (
    SELECT id FROM schematic WHERE category = 'decoder'
);

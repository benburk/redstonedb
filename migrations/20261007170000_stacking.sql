-- A stackable build is one slice that repeats, along one or more
-- directions. Each copy along a direction sits `west`, `up`, and `north`
-- blocks from the one before, since copies count up from the south-east-
-- bottom corner. max_copies caps how many work; NULL means no limit.
CREATE TABLE stack_direction (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    west INTEGER NOT NULL DEFAULT 0 CHECK (west >= 0),
    up INTEGER NOT NULL DEFAULT 0 CHECK (up >= 0),
    north INTEGER NOT NULL DEFAULT 0 CHECK (north >= 0),
    max_copies INTEGER CHECK (max_copies >= 2),
    CHECK (west + up + north > 0),
    PRIMARY KEY (schematic_id, west, up, north)
) STRICT;

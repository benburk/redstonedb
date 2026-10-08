-- A build can have several builders, so they move from one author column
-- to a list, in the order they were given.
CREATE TABLE builder (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    -- An in-game name, lowercase.
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    position INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, name)
) STRICT;

INSERT INTO builder (schematic_id, name, position)
SELECT id, author, 0 FROM schematic WHERE author IS NOT NULL;

ALTER TABLE schematic DROP COLUMN author;

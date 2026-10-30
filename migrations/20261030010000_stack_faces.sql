-- The box a stack's later copies repeat, when testing copies chose it over
-- the one the box rule finds: the layers it leaves out on each face, as
-- `{"low": [west, bottom, north], "high": [east, top, south]}`. NULL for
-- the box rule's.
ALTER TABLE stack_direction ADD COLUMN faces TEXT
    CHECK (faces IS NULL OR json_valid(faces));

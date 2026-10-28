-- What each building block of a recognized build's version must be, as
-- `roles::roles` reads it, for palettes: JSON of `solid`, `transparent`,
-- and `either`, each a list of [x, y, z]; NULL for any other build.
ALTER TABLE schematic_version ADD COLUMN needs TEXT
    CHECK (needs IS NULL OR json_valid(needs));

-- A tag's name and token may hold dashes, as `double-density` does, but
-- not at either end, as a dash before a search term leaves it out. The
-- tables are rebuilt to change the check, the links first, so dropping
-- the old tags takes none of them along.
CREATE TABLE tag_new (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL UNIQUE CHECK (
        name <> '' AND name NOT GLOB '*[^-a-z0-9_]*'
        AND name NOT GLOB '-*' AND name NOT GLOB '*-'
    ),
    -- Which colours its badge.
    kind TEXT NOT NULL CHECK (kind IN ('restriction', 'other')),
    -- A short form that commands and search accept in its place.
    token TEXT UNIQUE CHECK (
        token <> '' AND token NOT GLOB '*[^-a-z0-9_]*'
        AND token NOT GLOB '-*' AND token NOT GLOB '*-'
    )
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

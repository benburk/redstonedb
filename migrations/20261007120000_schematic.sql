-- Orientation and encoding are required single-valued facets, so they are
-- columns. Restrictions, timing, and future tags are many-valued, so they live
-- in `tag`. NULL in width, latency, author, or built_on means unknown or not
-- applicable.
CREATE TABLE schematic (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    function TEXT NOT NULL CHECK (
        function GLOB '[a-z]*' AND function NOT GLOB '*[^a-z0-9_]*'
    ),
    width INTEGER CHECK (width > 0),
    orientation TEXT NOT NULL CHECK (orientation IN ('horz', 'vert', 'diag')),
    encoding TEXT NOT NULL CHECK (encoding IN ('bin', 'hex')),
    latency INTEGER CHECK (latency >= 0),
    size_x INTEGER NOT NULL CHECK (size_x > 0),
    size_y INTEGER NOT NULL CHECK (size_y > 0),
    size_z INTEGER NOT NULL CHECK (size_z > 0),
    author TEXT CHECK (author <> '' AND author NOT GLOB '*[^a-z0-9_]*'),
    built_on TEXT CHECK (
        built_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    source_name TEXT NOT NULL,
    sha256 TEXT NOT NULL UNIQUE,
    data BLOB NOT NULL,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

CREATE TABLE tag (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL UNIQUE CHECK (
        name <> '' AND name NOT GLOB '*[^a-z0-9_]*'
    ),
    category TEXT NOT NULL CHECK (
        category IN ('restriction', 'timing', 'other')
    )
) STRICT;

CREATE TABLE schematic_tag (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tag (id),
    PRIMARY KEY (schematic_id, tag_id)
) STRICT;

INSERT INTO tag (name, category) VALUES
    ('alpha', 'restriction'),
    ('torchless', 'restriction'),
    ('seq', 'timing'),
    ('comb', 'timing'),
    ('pipe', 'timing');

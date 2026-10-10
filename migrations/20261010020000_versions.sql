-- Every file a build has had, numbered from 1; the highest is the one in
-- `schematic.data`. A new version replaces a build's file to fix it,
-- keeping its details; see src/version.rs.
CREATE TABLE schematic_version (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    data BLOB NOT NULL,
    sha256 TEXT NOT NULL,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- The account that uploaded it; NULL for one imported with rsdb.
    added_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    -- What it changed, in its uploader's words; NULL for the first.
    note TEXT CHECK (note <> '' AND length(note) <= 500),
    PRIMARY KEY (schematic_id, version)
) STRICT;

INSERT INTO schematic_version
    (schematic_id, version, data, sha256, added_at, added_by)
SELECT id, 1, data, sha256, added_at, uploaded_by FROM schematic;

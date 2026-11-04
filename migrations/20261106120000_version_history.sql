-- A build's history: who made each version, the site itself among them;
-- a restored version, copying an earlier one's file; what each version
-- measured, and what it changed from the one before, for its list; and
-- downloads of a diff between two versions.

-- RedstoneDB, the author of the versions the site makes itself, as when
-- it takes out unused blocks or ports a build to a new rule. No Discord
-- account has this id, so no one signs in as it.
INSERT INTO user (id, discord_id, username, display_name)
VALUES (X'000000000000000000000000000000DB', 'redstonedb', 'RedstoneDB', 'RedstoneDB');

UPDATE schematic_version SET added_by = X'000000000000000000000000000000DB'
WHERE added_by IS NULL
    AND note IN ('Unused blocks taken out', 'Its outputs read by the rule for them');

-- The version a restored version copies, whose file it shares; NULL for
-- any other.
ALTER TABLE schematic_version ADD COLUMN restores INTEGER CHECK (restores >= 1);

-- What the version measured, once tested: JSON of its verdict, latency,
-- throughput, size, and tightest stacking pitch; NULL until then, and for
-- versions from before it was kept.
ALTER TABLE schematic_version ADD COLUMN measures TEXT
    CHECK (measures IS NULL OR json_valid(measures));

-- What the version changed from the one before, as `diff` counts it:
-- JSON of `added`, `removed`, and `components`; NULL until first shown,
-- and for the first version.
ALTER TABLE schematic_version ADD COLUMN changes TEXT
    CHECK (changes IS NULL OR json_valid(changes));

-- No two versions are the same file, but a restored one and the version it
-- copies.
DROP INDEX schematic_version_sha256;
CREATE UNIQUE INDEX schematic_version_sha256 ON schematic_version (sha256)
WHERE restores IS NULL;

-- A download may be either side of a diff.
CREATE TABLE download_new (
    id INTEGER PRIMARY KEY,
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    -- NULL once the account is deleted.
    user_id BLOB REFERENCES user (id) ON DELETE SET NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    -- The whole file, a stacking part of it, an earlier version, the file
    -- sent to the account's Schemati, or a side of a diff, `version` being
    -- that side's.
    kind TEXT NOT NULL
        CHECK (kind IN ('file', 'part', 'version', 'schemati', 'diff')),
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;
INSERT INTO download_new SELECT * FROM download;
DROP TABLE download;
ALTER TABLE download_new RENAME TO download;
CREATE INDEX download_by_schematic ON download (schematic_id, at);
CREATE INDEX download_by_user ON download (user_id, at);

-- A build's file lives only in its versions. Its current file is its
-- highest version's, so `schematic` keeps neither the file nor its hash,
-- which it copied from that version. Every file is stored once: no two
-- versions, of one build or of two, share a hash.
--
-- Any build whose highest version is not its current file first gets it
-- as that version, and any without a version gets its file as version 1,
-- so no file is lost when `schematic.data` goes.
UPDATE schematic_version
SET data = (SELECT data FROM schematic WHERE id = schematic_id),
    sha256 = (SELECT sha256 FROM schematic WHERE id = schematic_id)
WHERE version = (
        SELECT max(version) FROM schematic_version AS newest
        WHERE newest.schematic_id = schematic_version.schematic_id
    )
    AND sha256 <> (SELECT sha256 FROM schematic WHERE id = schematic_id);

INSERT INTO schematic_version
    (schematic_id, version, data, sha256, added_at, added_by)
SELECT id, 1, data, sha256, added_at, uploaded_by FROM schematic
WHERE NOT EXISTS (
    SELECT 1 FROM schematic_version WHERE schematic_id = schematic.id
);

-- SQLite cannot drop a unique column, so the table is rebuilt without the
-- two. Every table that points at it is rebuilt first to point at the new
-- one, so dropping the old one takes none of their rows along.
CREATE TABLE schematic_new (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    -- A standard category's name, or any other in the same form.
    category TEXT NOT NULL CHECK (
        category <> '' AND category NOT GLOB '*[^-a-z0-9_]*'
    ),
    -- The build's own name, for a category that takes one.
    given_name TEXT CHECK (given_name <> '' AND length(given_name) <= 60),
    design TEXT NOT NULL DEFAULT 'other' CHECK (
        design <> '' AND design NOT GLOB '*[^a-z0-9_]*'
    ),
    width INTEGER CHECK (width > 0),
    orientation TEXT NOT NULL CHECK (orientation IN ('horz', 'vert', 'diag')),
    -- NULL for a category off the list, which has none.
    encoding TEXT CHECK (encoding IN ('bin', 'hex')),
    latency INTEGER CHECK (latency >= 0),
    synchronized INTEGER CHECK (synchronized IN (0, 1)),
    -- Its builds' logic where it differs from its category's; NULL
    -- follows the category.
    logic TEXT CHECK (logic IN ('comb', 'seq')),
    -- The fewest ticks from one input to the next that still gives right
    -- outputs; for a serial build, from one bit to the next.
    throughput INTEGER CHECK (throughput > 0),
    -- For a memory, how many bytes it holds.
    bytes INTEGER CHECK (bytes > 0),
    -- For a threshold, the fewest inputs on that turn it on.
    at_least INTEGER CHECK (at_least > 0),
    -- The build less any outer layer of nothing but its interface.
    size_x INTEGER NOT NULL CHECK (size_x > 0),
    size_y INTEGER NOT NULL CHECK (size_y > 0),
    size_z INTEGER NOT NULL CHECK (size_z > 0),
    -- Whether it follows the building conventions, so it is linted.
    conventional INTEGER NOT NULL DEFAULT 1 CHECK (conventional IN (0, 1)),
    -- Whether its file keeps where its uploader's //paste put it, rather
    -- than pasting from one block south of its south-east-bottom corner.
    keeps_paste INTEGER NOT NULL DEFAULT 0 CHECK (keeps_paste IN (0, 1)),
    -- Read from its current file: its redstone torches, and the oldest
    -- release it works in with the component or mechanic that sets it.
    torches INTEGER CHECK (torches >= 0),
    works_from TEXT CHECK (works_from <> ''),
    works_from_by TEXT CHECK (works_from_by <> ''),
    description TEXT CHECK (description <> ''),
    built_on TEXT CHECK (
        built_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    -- The in-game name of whoever imported it with rsdb.
    imported_by TEXT CHECK (
        imported_by <> '' AND imported_by NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    uploaded_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- When a person last changed its details; NULL if never.
    edited_at TEXT,
    downloads INTEGER NOT NULL DEFAULT 0 CHECK (downloads >= 0),
    -- The category and file hash, as `category/sha256`, it was last
    -- compared under; NULL until it is.
    compared_as TEXT
) STRICT;

INSERT INTO schematic_new (
    id, category, given_name, design, width, orientation, encoding, latency,
    synchronized, logic, throughput, bytes, at_least, size_x, size_y, size_z,
    conventional, keeps_paste, torches, works_from, works_from_by,
    description, built_on, imported_by, uploaded_by, added_at, edited_at,
    downloads, compared_as
)
SELECT
    id, category, given_name, design, width, orientation, encoding, latency,
    synchronized, logic, throughput, bytes, at_least, size_x, size_y, size_z,
    conventional, keeps_paste, torches, works_from, works_from_by,
    description, built_on, imported_by, uploaded_by, added_at, edited_at,
    downloads, compared_as
FROM schematic;

-- Every file a build has had, numbered from 1; the highest is the one it
-- has now. A new version replaces a build's file to fix it, keeping its
-- details; see src/version.rs.
CREATE TABLE schematic_version_new (
    schematic_id TEXT NOT NULL
        REFERENCES schematic_new (id) ON DELETE CASCADE,
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
INSERT INTO schematic_version_new
    (schematic_id, version, data, sha256, added_at, added_by, note)
SELECT schematic_id, version, data, sha256, added_at, added_by, note
FROM schematic_version;

CREATE TABLE schematic_flag_new (
    schematic_id TEXT NOT NULL
        REFERENCES schematic_new (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    PRIMARY KEY (schematic_id, name)
) STRICT;
INSERT INTO schematic_flag_new (schematic_id, name)
SELECT schematic_id, name FROM schematic_flag;

CREATE TABLE credit_new (
    schematic_id TEXT NOT NULL
        REFERENCES schematic_new (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('builder', 'contributor')),
    name TEXT NOT NULL COLLATE NOCASE CHECK (
        name <> '' AND name NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    position INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, role, name)
) STRICT;
INSERT INTO credit_new (schematic_id, role, name, position)
SELECT schematic_id, role, name, position FROM credit;

CREATE TABLE stack_direction_new (
    schematic_id TEXT NOT NULL
        REFERENCES schematic_new (id) ON DELETE CASCADE,
    west INTEGER NOT NULL DEFAULT 0 CHECK (west >= 0),
    up INTEGER NOT NULL DEFAULT 0 CHECK (up >= 0),
    north INTEGER NOT NULL DEFAULT 0 CHECK (north >= 0),
    max_copies INTEGER CHECK (max_copies >= 2),
    CHECK (west + up + north > 0),
    PRIMARY KEY (schematic_id, west, up, north)
) STRICT;
INSERT INTO stack_direction_new (schematic_id, west, up, north, max_copies)
SELECT schematic_id, west, up, north, max_copies FROM stack_direction;

CREATE TABLE simulation_new (
    schematic_id TEXT NOT NULL PRIMARY KEY
        REFERENCES schematic_new (id) ON DELETE CASCADE,
    -- The test run, such as `adder`.
    test TEXT NOT NULL CHECK (test <> ''),
    -- The version of the test it ran under; see `simulate::Test::version`.
    version INTEGER NOT NULL DEFAULT 0,
    -- The file's hash when it ran, so a changed file is run again.
    sha256 TEXT NOT NULL,
    -- Whether the build got cases wrong or is broken.
    failed INTEGER NOT NULL DEFAULT 0 CHECK (failed IN (0, 1)),
    -- `simulate::Report` as JSON.
    report TEXT NOT NULL CHECK (json_valid(report)),
    ran_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;
INSERT INTO simulation_new
    (schematic_id, test, version, sha256, failed, report, ran_at)
SELECT schematic_id, test, version, sha256, failed, report, ran_at
FROM simulation;

CREATE TABLE schematic_tag_new (
    schematic_id TEXT NOT NULL
        REFERENCES schematic_new (id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tag (id) ON DELETE CASCADE,
    PRIMARY KEY (schematic_id, tag_id)
) STRICT;
INSERT INTO schematic_tag_new (schematic_id, tag_id)
SELECT schematic_id, tag_id FROM schematic_tag;

-- How alike two builds of one category are, as src/similarity.rs measures
-- it, in tenths of a percent. Each pair is kept once, the lesser id first.
CREATE TABLE similarity_new (
    schematic_id TEXT NOT NULL
        REFERENCES schematic_new (id) ON DELETE CASCADE,
    other_id TEXT NOT NULL REFERENCES schematic_new (id) ON DELETE CASCADE,
    permille INTEGER NOT NULL CHECK (permille BETWEEN 0 AND 1000),
    PRIMARY KEY (schematic_id, other_id),
    CHECK (schematic_id < other_id)
) STRICT;
INSERT INTO similarity_new (schematic_id, other_id, permille)
SELECT schematic_id, other_id, permille FROM similarity;

DROP TABLE schematic_version;
DROP TABLE schematic_flag;
DROP TABLE credit;
DROP TABLE stack_direction;
DROP TABLE simulation;
DROP TABLE schematic_tag;
DROP TABLE similarity;
DROP TABLE schematic;
ALTER TABLE schematic_new RENAME TO schematic;
ALTER TABLE schematic_version_new RENAME TO schematic_version;
ALTER TABLE schematic_flag_new RENAME TO schematic_flag;
ALTER TABLE credit_new RENAME TO credit;
ALTER TABLE stack_direction_new RENAME TO stack_direction;
ALTER TABLE simulation_new RENAME TO simulation;
ALTER TABLE schematic_tag_new RENAME TO schematic_tag;
ALTER TABLE similarity_new RENAME TO similarity;

CREATE UNIQUE INDEX schematic_version_sha256 ON schematic_version (sha256);

-- Each build's current file: its highest version.
CREATE VIEW current_file AS
SELECT schematic_id, version, data, sha256 FROM schematic_version
WHERE version = (
    SELECT max(version) FROM schematic_version AS newest
    WHERE newest.schematic_id = schematic_version.schematic_id
);

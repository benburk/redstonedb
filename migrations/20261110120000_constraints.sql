-- More of the rules the code keeps, kept by the database too:
-- * an account's creation time is always known, taken at insert;
-- * a restored version names an earlier version of its own build, and a
--   download a version that exists;
-- * an author's or contributor's position is unique in its build;
-- * a confirmed Minecraft name has a name, Schemati's player is known only
--   once checked, and its expiry only with a session;
-- * a build's release reason only with a release, and an edit no earlier
--   than its upload;
-- * a file hash is 64 lowercase hex digits, and the times these tables
--   keep are ISO 8601 in UTC to the second.
--
-- Each table is rebuilt as SQLite's own procedure for changing one has it:
-- a new table made, filled, the old dropped, the new renamed to its name.
-- Dropping a table others point at would cascade into them, so foreign
-- keys are off for it; SQLite cannot turn them off inside a transaction,
-- and sqlx runs each SQLite migration in one, so this one commits sqlx's
-- empty transaction, does its work in its own, and opens another for sqlx
-- to record it in and commit. The check at the end refuses the whole
-- change, rolled back, if anything points nowhere.

COMMIT;
PRAGMA foreign_keys = OFF;
BEGIN;

-- The view reads `schematic_version`, so it goes while that is rebuilt.
DROP VIEW current_file;

-- Accounts ---------------------------------------------------------------

-- RedstoneDB itself is given the site's start, the first build's upload,
-- or now on a database with none.
UPDATE user SET created_at = coalesce(
    (SELECT min(added_at) FROM schematic),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
WHERE created_at IS NULL;

CREATE TABLE user_new (
    id BLOB NOT NULL PRIMARY KEY,
    discord_id TEXT NOT NULL UNIQUE,
    -- The Discord username, refreshed at each login.
    username TEXT NOT NULL UNIQUE,
    -- The Discord display name and avatar hash; NULL where the Discord
    -- account has none.
    display_name TEXT,
    avatar TEXT,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'mod', 'admin')),
    -- The Minecraft name the person gave; one account per name, whatever
    -- its case.
    minecraft_name TEXT UNIQUE COLLATE NOCASE CHECK (
        minecraft_name <> ''
        AND length(minecraft_name) <= 16
        AND minecraft_name NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    minecraft_uuid TEXT CHECK (
        length(minecraft_uuid) = 32 AND minecraft_uuid NOT GLOB '*[^0-9a-f]*'
    ),
    minecraft_verified INTEGER NOT NULL DEFAULT 0
        CHECK (minecraft_verified IN (0, 1)),
    -- The `sessionId` cookie Schemati's own Discord login sets, which opens
    -- the folder of the player linked to it; Schemati forgets it within a
    -- day.
    schemati_session TEXT,
    -- What that session is known to be, so a page can say whose folder it
    -- opens without asking Schemati: the player Schemati greeted, NULL
    -- until it names one, and when it last answered.
    schemati_player TEXT,
    schemati_checked_at TEXT CHECK (
        schemati_checked_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'
    ),
    -- When Schemati first said it no longer knows the session; NULL while it
    -- still does. Answering with it again, or a new session, clears it.
    schemati_expired_at TEXT CHECK (
        schemati_expired_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'
    ),
    -- Their block theme, as JSON; NULL for the default.
    palette TEXT CHECK (palette IS NULL OR json_valid(palette)),
    -- Whether they have been shown the welcome steps, which a new account
    -- sees once, at its first login.
    welcomed INTEGER NOT NULL DEFAULT 0 CHECK (welcomed IN (0, 1)),
    -- Its first login; for an account from before that was kept, a best
    -- guess.
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    CHECK (minecraft_verified = 0 OR minecraft_name IS NOT NULL),
    CHECK (schemati_player IS NULL OR schemati_checked_at IS NOT NULL),
    CHECK (schemati_expired_at IS NULL OR schemati_session IS NOT NULL)
) STRICT;
INSERT INTO user_new (
    id, discord_id, username, display_name, avatar, role, minecraft_name,
    minecraft_uuid, minecraft_verified, schemati_session, schemati_player,
    schemati_checked_at, schemati_expired_at, palette, welcomed, created_at
)
SELECT
    id, discord_id, username, display_name, avatar, role, minecraft_name,
    minecraft_uuid, minecraft_verified, schemati_session, schemati_player,
    schemati_checked_at, schemati_expired_at, palette, welcomed, created_at
FROM user;
DROP TABLE user;
ALTER TABLE user_new RENAME TO user;
CREATE UNIQUE INDEX user_minecraft_uuid ON user (minecraft_uuid);

-- Builds -----------------------------------------------------------------

CREATE TABLE schematic_new (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    -- A standard category's name, as the code lists them, or any other in
    -- the same form.
    category TEXT NOT NULL CHECK (
        category <> '' AND category NOT GLOB '*[^-a-z0-9_]*'
    ),
    -- The build's own name, for a category that takes one.
    given_name TEXT CHECK (given_name <> '' AND length(given_name) <= 60),
    -- One of its category's designs, as the code lists them; NULL for
    -- none.
    design TEXT CHECK (design <> '' AND design NOT GLOB '*[^a-z0-9_]*'),
    description TEXT CHECK (description <> ''),
    built_on TEXT CHECK (
        built_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    -- A private build is seen only by its uploader and admins, in no
    -- listing, search, or leaderboard.
    public INTEGER NOT NULL DEFAULT 1 CHECK (public IN (0, 1)),
    -- Why a build of a category the site recognizes builds of isn't
    -- recognized as one, such as `It does not pass the adder test`; NULL for
    -- one that is, or any other build. Such a build is kept as built, under
    -- the category its uploader gave, and listed only when asked for.
    unverified TEXT CHECK (unverified IS NULL OR unverified <> ''),

    -- Its measures. NULL where its category has none or none is known.
    width INTEGER CHECK (width > 0),
    orientation TEXT NOT NULL CHECK (orientation IN ('horz', 'vert', 'diag')),
    -- NULL for a category off the list, which has none.
    encoding TEXT CHECK (encoding IN ('bin', 'hex')),
    -- Its logic where it differs from its category's; NULL follows the
    -- category.
    logic TEXT CHECK (logic IN ('stateless', 'stateful')),
    latency INTEGER CHECK (latency >= 0),
    synchronized INTEGER CHECK (synchronized IN (0, 1)),
    throughput INTEGER CHECK (throughput > 0),
    -- For a memory, how many bytes it holds.
    bytes INTEGER CHECK (bytes > 0),
    -- For a threshold, the fewest inputs on that turn it on.
    at_least INTEGER CHECK (at_least > 0),
    -- For a monostable, its pulse's length in ticks.
    pulse INTEGER CHECK (pulse > 0),
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
    -- The category and file hash, as `category/sha256`, it was last
    -- compared to other builds under; NULL until it is.
    compared_as TEXT,

    -- The in-game name of whoever imported it with rsdb.
    imported_by TEXT CHECK (
        imported_by <> '' AND imported_by NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    uploaded_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        CHECK (added_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    -- When a person last changed its details; NULL if never.
    edited_at TEXT CHECK (
        edited_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'
        AND edited_at >= added_at
    ),
    downloads INTEGER NOT NULL DEFAULT 0 CHECK (downloads >= 0),
    CHECK (works_from_by IS NULL OR works_from IS NOT NULL)
) STRICT;
INSERT INTO schematic_new (
    id, category, given_name, design, description, built_on, public,
    unverified, width, orientation, encoding, logic, latency, synchronized,
    throughput, bytes, at_least, pulse, size_x, size_y, size_z, conventional,
    keeps_paste, torches, works_from, works_from_by, compared_as, imported_by,
    uploaded_by, added_at, edited_at, downloads
)
SELECT
    id, category, given_name, design, description, built_on, public,
    unverified, width, orientation, encoding, logic, latency, synchronized,
    throughput, bytes, at_least, pulse, size_x, size_y, size_z, conventional,
    keeps_paste, torches, works_from, works_from_by, compared_as, imported_by,
    uploaded_by, added_at, edited_at, downloads
FROM schematic;
DROP TABLE schematic;
ALTER TABLE schematic_new RENAME TO schematic;
CREATE INDEX schematic_uploaded_by ON schematic (uploaded_by);

-- Versions ---------------------------------------------------------------

CREATE TABLE schematic_version_new (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    data BLOB NOT NULL,
    sha256 TEXT NOT NULL
        CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    -- What it changed, in its uploader's words; NULL for none given.
    note TEXT CHECK (note <> '' AND length(note) <= 500),
    -- The IP address it was uploaded from; NULL for one imported with rsdb.
    added_from TEXT CHECK (added_from <> '' AND length(added_from) <= 45),
    -- For a recognized build, what each building block must be, as
    -- `roles::roles` reads it, for palettes: JSON of `solid`, `transparent`,
    -- and `either`, each a list of [x, y, z]; NULL for any other build.
    needs TEXT CHECK (needs IS NULL OR json_valid(needs)),
    -- The colours its author gave its building blocks, where they differ
    -- from the stored file's: JSON of [[x, y, z], block] pairs; NULL when
    -- none are kept.
    colours TEXT CHECK (colours IS NULL OR json_valid(colours)),
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        CHECK (added_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    -- The account that uploaded it; NULL for one imported with rsdb.
    added_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    -- The roles of its lines, as `interface::Kept` writes them; NULL until
    -- found.
    roles TEXT CHECK (roles IS NULL OR json_valid(roles)),
    -- The earlier version a restored version copies, whose file it shares;
    -- NULL for any other.
    restores INTEGER CHECK (restores >= 1 AND restores < version),
    -- What it measured, once tested, and what it changed from the version
    -- before, once counted; NULL until then.
    measures TEXT CHECK (measures IS NULL OR json_valid(measures)),
    changes TEXT CHECK (changes IS NULL OR json_valid(changes)),
    PRIMARY KEY (schematic_id, version),
    FOREIGN KEY (schematic_id, restores)
        REFERENCES schematic_version (schematic_id, version)
) STRICT;
INSERT INTO schematic_version_new (
    schematic_id, version, data, sha256, note, added_from, needs, colours,
    added_at, added_by, roles, restores, measures, changes
)
SELECT
    schematic_id, version, data, sha256, note, added_from, needs, colours,
    added_at, added_by, roles, restores, measures, changes
FROM schematic_version;
DROP TABLE schematic_version;
ALTER TABLE schematic_version_new RENAME TO schematic_version;
CREATE INDEX schematic_version_added_by ON schematic_version (added_by);
CREATE UNIQUE INDEX schematic_version_sha256 ON schematic_version (sha256)
WHERE restores IS NULL;

CREATE VIEW current_file AS
SELECT schematic_id, version, data, sha256, roles FROM schematic_version
WHERE version = (
    SELECT max(version) FROM schematic_version AS newest
    WHERE newest.schematic_id = schematic_version.schematic_id
);

-- Downloads and credits --------------------------------------------------

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
        CHECK (at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    FOREIGN KEY (schematic_id, version)
        REFERENCES schematic_version (schematic_id, version) ON DELETE CASCADE
) STRICT;
INSERT INTO download_new (id, schematic_id, user_id, version, kind, at)
SELECT id, schematic_id, user_id, version, kind, at FROM download;
DROP TABLE download;
ALTER TABLE download_new RENAME TO download;
CREATE INDEX download_by_schematic ON download (schematic_id, at);
CREATE INDEX download_by_user ON download (user_id, at);

CREATE TABLE credit_new (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('author', 'contributor')),
    name TEXT NOT NULL COLLATE NOCASE CHECK (
        name <> '' AND name NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    -- Its place among the build's names of its role, from 0.
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (schematic_id, role, name),
    UNIQUE (schematic_id, role, position)
) STRICT;
INSERT INTO credit_new (schematic_id, role, name, position)
SELECT schematic_id, role, name, position FROM credit;
DROP TABLE credit;
ALTER TABLE credit_new RENAME TO credit;

-- Nothing may point nowhere: a row here fails the whole change.
CREATE TEMP TABLE dangling (n INTEGER NOT NULL CHECK (n = 0));
INSERT INTO dangling SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE dangling;

COMMIT;
PRAGMA foreign_keys = ON;
BEGIN;

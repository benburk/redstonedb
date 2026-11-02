-- The whole schema, as it stood when the migrations before it were folded
-- into one. Later changes are migrations of their own after this one.

-- Accounts --------------------------------------------------------------

-- A person, signed in with Discord.
CREATE TABLE user (
    id BLOB NOT NULL PRIMARY KEY,
    discord_id TEXT NOT NULL UNIQUE,
    -- The Discord username, refreshed at each login.
    username TEXT NOT NULL UNIQUE,
    display_name TEXT,
    -- The Discord avatar's hash, from which its image address follows.
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
    -- opens without asking Schemati: the player Schemati greeted when it
    -- last answered with it, and when that was; both NULL until Schemati
    -- next answers with it.
    schemati_player TEXT,
    schemati_checked_at TEXT,
    -- When Schemati first said it no longer knows the session; NULL while it
    -- still does. Answering with it again, or a new session, clears it.
    schemati_expired_at TEXT,
    -- Their block theme, as JSON; NULL for the default.
    palette TEXT CHECK (palette IS NULL OR json_valid(palette)),
    -- Whether they have been shown the welcome steps, which a new account
    -- sees once, at its first login.
    welcomed INTEGER NOT NULL DEFAULT 0 CHECK (welcomed IN (0, 1))
) STRICT;

CREATE UNIQUE INDEX user_minecraft_uuid ON user (minecraft_uuid);

CREATE TABLE session (
    id BLOB NOT NULL PRIMARY KEY,
    user_id BLOB NOT NULL REFERENCES user (id) ON DELETE CASCADE,
    -- Microseconds since the Unix epoch.
    expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX session_user_id ON session (user_id);

-- Builds ------------------------------------------------------------------

CREATE TABLE schematic (
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
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- When a person last changed its details; NULL if never.
    edited_at TEXT,
    downloads INTEGER NOT NULL DEFAULT 0 CHECK (downloads >= 0)
) STRICT;

CREATE INDEX schematic_uploaded_by ON schematic (uploaded_by);

-- Each file a build has had, the newest its current one.
CREATE TABLE schematic_version (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    data BLOB NOT NULL,
    sha256 TEXT NOT NULL,
    -- What it changed, in its uploader's words; NULL for the first.
    note TEXT CHECK (note <> '' AND length(note) <= 500),
    -- The IP address it was uploaded from; NULL for one imported with rsdb.
    added_from TEXT CHECK (added_from <> '' AND length(added_from) <= 45),
    -- For a recognized build, what each building block must be, as
    -- `roles::roles` reads it, for palettes: JSON of `solid`, `transparent`,
    -- and `either`, each a list of [x, y, z]; NULL for any other build.
    needs TEXT CHECK (needs IS NULL OR json_valid(needs)),
    -- The colours its builder gave its building blocks, where they differ
    -- from the stored file's: JSON of [[x, y, z], block] pairs; NULL when
    -- none are kept.
    colours TEXT CHECK (colours IS NULL OR json_valid(colours)),
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- The account that uploaded it; NULL for one imported with rsdb.
    added_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    PRIMARY KEY (schematic_id, version)
) STRICT;

-- No two versions, of one build or of two, are the same file.
CREATE UNIQUE INDEX schematic_version_sha256 ON schematic_version (sha256);
CREATE INDEX schematic_version_added_by ON schematic_version (added_by);

CREATE VIEW current_file AS
SELECT schematic_id, version, data, sha256 FROM schematic_version
WHERE version = (
    SELECT max(version) FROM schematic_version AS newest
    WHERE newest.schematic_id = schematic_version.schematic_id
);

-- A build's flags, from its category's.
CREATE TABLE schematic_flag (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    PRIMARY KEY (schematic_id, name)
) STRICT;

-- Who built a build, in-game names in order.
CREATE TABLE credit (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('builder', 'contributor')),
    name TEXT NOT NULL COLLATE NOCASE CHECK (
        name <> '' AND name NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    position INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, role, name)
) STRICT;

CREATE TABLE tag (
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

CREATE TABLE schematic_tag (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tag (id) ON DELETE CASCADE,
    PRIMARY KEY (schematic_id, tag_id)
) STRICT;

-- Each way copies of a build stack, a step of so many blocks along each
-- axis.
CREATE TABLE stack_direction (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    west INTEGER NOT NULL DEFAULT 0 CHECK (west >= 0),
    up INTEGER NOT NULL DEFAULT 0 CHECK (up >= 0),
    north INTEGER NOT NULL DEFAULT 0 CHECK (north >= 0),
    -- The most copies that work; NULL for no limit.
    max_copies INTEGER CHECK (max_copies >= 2),
    -- The box later copies repeat, when testing copies chose it over the
    -- one the box rule finds: the layers it leaves out on each face, as
    -- `{"low": [west, bottom, north], "high": [east, top, south]}`. NULL for
    -- the box rule's.
    faces TEXT CHECK (faces IS NULL OR json_valid(faces)),
    CHECK (west + up + north > 0),
    PRIMARY KEY (schematic_id, west, up, north)
) STRICT;

-- How a stackable build does stacked to a leaderboard's width.
CREATE TABLE stacked_width (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    width INTEGER NOT NULL CHECK (width > 0),
    copies INTEGER NOT NULL CHECK (copies >= 2),
    latency INTEGER CHECK (latency >= 0),
    synchronized INTEGER CHECK (synchronized IN (0, 1)),
    throughput INTEGER CHECK (throughput > 0),
    PRIMARY KEY (schematic_id, width)
) STRICT;

CREATE TABLE favorite (
    user_id BLOB NOT NULL REFERENCES user (id) ON DELETE CASCADE,
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, schematic_id)
) STRICT;

-- Results kept so they are not worked out again ---------------------------

-- A build's latest test result.
CREATE TABLE simulation (
    schematic_id TEXT NOT NULL PRIMARY KEY
        REFERENCES schematic (id) ON DELETE CASCADE,
    -- The test run, such as `adder`.
    test TEXT NOT NULL CHECK (test <> ''),
    -- The hash of the file it ran on.
    sha256 TEXT NOT NULL,
    -- The key of the compiled circuit it ran on, so a new file holding the
    -- same circuit takes the result over; NULL if it could not be compiled.
    circuit TEXT,
    -- Whether the build got cases wrong or is broken.
    failed INTEGER NOT NULL DEFAULT 0 CHECK (failed IN (0, 1)),
    -- `simulate::Report` as JSON.
    report TEXT NOT NULL CHECK (json_valid(report)),
    ran_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

-- How alike two builds are, each pair once, the lesser id first.
CREATE TABLE similarity (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    other_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    permille INTEGER NOT NULL CHECK (permille BETWEEN 0 AND 1000),
    PRIMARY KEY (schematic_id, other_id),
    CHECK (schematic_id < other_id)
) STRICT;

CREATE INDEX similarity_other_id ON similarity (other_id);

-- A Minecraft player, as Mojang last answered for their name.
CREATE TABLE player (
    -- In lowercase, as names are the same in any capitals.
    name TEXT NOT NULL PRIMARY KEY CHECK (name <> '' AND name = lower(name)),
    uuid TEXT CHECK (length(uuid) = 32 AND uuid NOT GLOB '*[^0-9a-f]*'),
    -- The spelling the player chose.
    spelling TEXT CHECK (spelling <> ''),
    checked_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

-- A player's head, drawn from their skin; NULL png for none to draw.
CREATE TABLE head (
    uuid TEXT NOT NULL PRIMARY KEY CHECK (
        length(uuid) = 32 AND uuid NOT GLOB '*[^0-9a-f]*'
    ),
    png BLOB,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

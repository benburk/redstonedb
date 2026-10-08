-- The whole schema, squashed from the migrations that grew it.

-- An account, made by logging in with Discord.
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
    )
) STRICT;

CREATE TABLE session (
    id BLOB NOT NULL PRIMARY KEY,
    user_id BLOB NOT NULL REFERENCES user (id) ON DELETE CASCADE,
    -- Microseconds since the Unix epoch.
    expires_at INTEGER NOT NULL
) STRICT;

-- A standard category, such as adder. A build may also name a category off
-- this list.
CREATE TABLE category (
    name TEXT NOT NULL PRIMARY KEY CHECK (
        name <> '' AND name NOT GLOB '*[^-a-z0-9_]*'
    ),
    -- As people read it, such as RAM.
    label TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (label <> ''),
    -- For many builds, such as ALUs; NULL for the label with an s.
    plural TEXT CHECK (plural <> ''),
    -- Whether its builds have carry lines worth recording.
    carries INTEGER NOT NULL DEFAULT 0 CHECK (carries IN (0, 1)),
    -- Whether each build gives its own name, as bussing's do.
    named INTEGER NOT NULL DEFAULT 0 CHECK (named IN (0, 1))
) STRICT;

-- A design within a category, such as the carry lookahead adder. Every
-- build's design is one of these or `other`.
CREATE TABLE design (
    category TEXT NOT NULL REFERENCES category (name) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    -- The usual short name, such as CLA; NULL when there is none.
    abbreviation TEXT CHECK (abbreviation <> ''),
    PRIMARY KEY (category, name)
) STRICT;

CREATE TABLE tag (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL UNIQUE CHECK (
        name <> '' AND name NOT GLOB '*[^a-z0-9_]*'
    ),
    -- Which colours its badge.
    kind TEXT NOT NULL CHECK (kind IN ('restriction', 'timing', 'other')),
    -- A short form, such as comb, that commands and search accept.
    token TEXT UNIQUE CHECK (token <> '' AND token NOT GLOB '*[^a-z0-9_]*')
) STRICT;

-- A build: its file and what is known about it. NULL means unknown.
CREATE TABLE schematic (
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
    -- Set only for a category with carry lines.
    carry_in INTEGER CHECK (carry_in IN (0, 1)),
    carry_out INTEGER CHECK (carry_out IN (0, 1)),
    -- The build less any outer layer of nothing but its interface.
    size_x INTEGER NOT NULL CHECK (size_x > 0),
    size_y INTEGER NOT NULL CHECK (size_y > 0),
    size_z INTEGER NOT NULL CHECK (size_z > 0),
    -- Whether it follows the building conventions, so it is linted.
    conventional INTEGER NOT NULL DEFAULT 1 CHECK (conventional IN (0, 1)),
    description TEXT CHECK (description <> ''),
    built_on TEXT CHECK (
        built_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    ),
    -- The in-game name of whoever imported it with rsdb.
    imported_by TEXT CHECK (
        imported_by <> '' AND imported_by NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    uploaded_by BLOB REFERENCES user (id) ON DELETE SET NULL,
    sha256 TEXT NOT NULL UNIQUE,
    data BLOB NOT NULL,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- When a person last changed its details; NULL if never.
    edited_at TEXT,
    downloads INTEGER NOT NULL DEFAULT 0 CHECK (downloads >= 0)
) STRICT;

CREATE TABLE schematic_tag (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tag (id) ON DELETE CASCADE,
    PRIMARY KEY (schematic_id, tag_id)
) STRICT;

-- A person credited on a build, in the order given within their role. Names
-- keep their capitals but compare without them.
CREATE TABLE credit (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('builder', 'contributor')),
    name TEXT NOT NULL COLLATE NOCASE CHECK (
        name <> '' AND name NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    position INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, role, name)
) STRICT;

-- One direction a stackable build repeats along: how far each copy sits
-- from the one before, and the most copies that work.
CREATE TABLE stack_direction (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    west INTEGER NOT NULL DEFAULT 0 CHECK (west >= 0),
    up INTEGER NOT NULL DEFAULT 0 CHECK (up >= 0),
    north INTEGER NOT NULL DEFAULT 0 CHECK (north >= 0),
    max_copies INTEGER CHECK (max_copies >= 2),
    CHECK (west + up + north > 0),
    PRIMARY KEY (schematic_id, west, up, north)
) STRICT;

INSERT INTO category (name, label, plural, carries, named) VALUES
    ('adder', 'Adder', NULL, 1, 0),
    ('alu', 'ALU', 'ALUs', 0, 0),
    ('barrel_shifter', 'Barrel Shifter', NULL, 0, 0),
    ('bcd_to_binary', 'BCD to Binary', 'BCD to Binary', 0, 0),
    ('binary_to_bcd', 'Binary to BCD', 'Binary to BCD', 0, 0),
    ('bussing', 'Bussing', 'Bussing', 0, 1),
    ('carry_save_adder', 'Carry Save Adder', NULL, 0, 0),
    ('counter', 'Counter', NULL, 0, 0),
    ('dataloop', 'Dataloop', NULL, 0, 0),
    ('decoder', 'Decoder', NULL, 0, 0),
    ('display', 'Display', NULL, 0, 0),
    ('divider', 'Divider', NULL, 0, 0),
    ('line_drawer', 'Line Drawer', NULL, 0, 0),
    ('multiplier', 'Multiplier', NULL, 0, 0),
    ('ram', 'RAM', 'RAM', 0, 0),
    ('register', 'Register', NULL, 0, 0),
    ('serial_write_memory', 'Serial Write Memory', 'Serial Write Memories', 0, 0),
    ('shift_register', 'Shift Register', NULL, 0, 0),
    ('subtractor', 'Subtractor', NULL, 1, 0);

INSERT INTO design (category, name, abbreviation) VALUES
    ('adder', 'ripple_carry', 'RCA'),
    ('adder', 'carry_lookahead', 'CLA'),
    ('adder', 'carry_cancel', 'CCA'),
    ('adder', 'carry_look_everywhere', 'CLE');

INSERT INTO tag (name, kind, token) VALUES
    ('alpha', 'restriction', NULL),
    ('torchless', 'restriction', NULL),
    ('combinational', 'timing', 'comb'),
    ('pipelined', 'timing', 'pipe'),
    ('sequential', 'timing', 'seq');

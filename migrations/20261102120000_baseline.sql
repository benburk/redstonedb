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

-- The catalogue -----------------------------------------------------------

-- A standard category. A build may name another, in the same form, off the
-- list.
CREATE TABLE category (
    name TEXT NOT NULL PRIMARY KEY CHECK (
        name <> '' AND name NOT GLOB '*[^-a-z0-9_]*'
    ),
    -- As people read it, such as RAM.
    label TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (label <> ''),
    -- For many builds, such as ALUs; NULL for the label with an s.
    plural TEXT CHECK (plural <> ''),
    -- Whether each build gives its own name, as bussing's do.
    named INTEGER NOT NULL DEFAULT 0 CHECK (named IN (0, 1)),
    -- The sidebar's heading it sits under; NULL puts it with the categories
    -- off the list, under Other.
    section TEXT CHECK (section IN (
        'arithmetic', 'logic', 'gates', 'conversion', 'memory', 'display',
        'wiring'
    )),
    -- Its builds' usual logic; NULL for none.
    logic TEXT CHECK (logic IN ('stateless', 'stateful'))
) STRICT;

-- A category's designs, such as an adder's carry lookahead. Builds name
-- them, so a category with designs cannot be deleted.
CREATE TABLE design (
    category TEXT NOT NULL REFERENCES category (name) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    -- The usual short name, such as CLA; NULL when there is none.
    abbreviation TEXT CHECK (abbreviation <> ''),
    -- How people read it, such as LFSR; NULL for the name's words.
    label TEXT CHECK (label <> ''),
    PRIMARY KEY (category, name)
) STRICT;

-- The yes-or-no facts a category's builds may have, such as signed.
CREATE TABLE flag (
    category TEXT NOT NULL REFERENCES category (name) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (name <> '' AND name NOT GLOB '*[^a-z0-9_]*'),
    label TEXT NOT NULL CHECK (label <> ''),
    -- Its place among its category's flags.
    position INTEGER NOT NULL,
    PRIMARY KEY (category, name)
) STRICT;

-- Another word search takes for a category. Its category need not be a
-- standard one.
CREATE TABLE category_alias (
    alias TEXT NOT NULL CHECK (alias <> '' AND alias NOT GLOB '*[^a-z0-9 ]*'),
    category TEXT NOT NULL CHECK (
        category <> '' AND category NOT GLOB '*[^-a-z0-9_]*'
    ),
    PRIMARY KEY (alias, category)
) STRICT;

-- What a category's builds are measured and named by, in order.
CREATE TABLE category_measure (
    category TEXT NOT NULL REFERENCES category (name) ON DELETE CASCADE,
    measure TEXT NOT NULL CHECK (
        measure IN ('width', 'bytes', 'throughput', 'at_least', 'pulse')
    ),
    -- Its place among its category's measures.
    position INTEGER NOT NULL,
    PRIMARY KEY (category, measure)
) STRICT;

-- Builds ------------------------------------------------------------------

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

-- The standard catalogue ------------------------------------------------

INSERT INTO category (name, label, plural, named, section, logic) VALUES
    ('absolute_value', 'Absolute Value', NULL, 0, 'arithmetic', 'stateless'),
    ('adder', 'Adder', NULL, 0, 'arithmetic', 'stateless'),
    ('alu', 'ALU', 'ALUs', 0, 'arithmetic', 'stateless'),
    ('and_gate', 'AND Gate', NULL, 0, 'gates', 'stateless'),
    ('barrel_shifter', 'Barrel Shifter', NULL, 0, 'logic', 'stateless'),
    ('bcd_to_binary', 'BCD to Binary', 'BCD to Binary', 0, 'conversion', 'stateless'),
    ('binary_to_bcd', 'Binary to BCD', 'Binary to BCD', 0, 'conversion', 'stateless'),
    ('binary_to_hex', 'Binary to Hex', 'Binary to Hex', 0, 'conversion', 'stateless'),
    ('buffer', 'Buffer', NULL, 0, 'memory', 'stateful'),
    ('bussing', 'Bussing', 'Bussing', 1, 'wiring', NULL),
    ('carry_out', 'Carry Out', NULL, 0, 'arithmetic', 'stateless'),
    ('carry_save_adder', 'Carry Save Adder', NULL, 0, 'arithmetic', 'stateless'),
    ('counter', 'Counter', NULL, 0, 'memory', 'stateful'),
    ('dataloop', 'Dataloop', NULL, 0, 'memory', 'stateful'),
    ('decoder', 'Decoder', NULL, 0, 'logic', 'stateless'),
    ('decrementer', 'Decrementer', NULL, 0, 'arithmetic', 'stateless'),
    ('display', 'Display', NULL, 1, 'display', NULL),
    ('divider', 'Divider', NULL, 0, 'arithmetic', 'stateless'),
    ('dual_edge_monostable', 'Dual-Edge Monostable', NULL, 0, 'logic', 'stateless'),
    ('equality', 'Equality', 'Equality', 0, 'arithmetic', 'stateless'),
    ('greater_than', 'Greater Than', 'Greater Than', 0, 'arithmetic', 'stateless'),
    ('hex_adder', 'Hex Adder', NULL, 0, 'arithmetic', 'stateless'),
    ('hex_to_binary', 'Hex to Binary', 'Hex to Binary', 0, 'conversion', 'stateless'),
    ('inc_dec', 'Inc/Dec', 'Inc/Dec', 0, 'arithmetic', 'stateless'),
    ('incrementer', 'Incrementer', NULL, 0, 'arithmetic', 'stateless'),
    ('inverter', 'Inverter', NULL, 0, 'logic', 'stateless'),
    ('line_drawer', 'Line Drawer', NULL, 0, 'display', NULL),
    ('monostable', 'Monostable', NULL, 0, 'logic', 'stateless'),
    ('multiplier', 'Multiplier', NULL, 0, 'arithmetic', 'stateless'),
    ('nand_gate', 'NAND Gate', NULL, 0, 'gates', 'stateless'),
    ('nand_nor_gate', 'NAND+NOR Gate', NULL, 0, 'gates', 'stateless'),
    ('nand_xnor_gate', 'NAND+XNOR Gate', NULL, 0, 'gates', 'stateless'),
    ('nor_gate', 'NOR Gate', NULL, 0, 'gates', 'stateless'),
    ('or_gate', 'OR Gate', NULL, 0, 'gates', 'stateless'),
    ('popcount', 'Popcount', NULL, 0, 'arithmetic', 'stateless'),
    ('ram', 'RAM', 'RAM', 0, 'memory', 'stateful'),
    ('register', 'Register', NULL, 0, 'memory', 'stateful'),
    ('rng', 'RNG', 'RNGs', 0, 'logic', 'stateful'),
    ('serial_adder', 'Serial Adder', NULL, 0, 'arithmetic', 'stateful'),
    ('serial_write_memory', 'Serial Write Memory', 'Serial Write Memories', 0, 'memory', 'stateful'),
    ('shift_register', 'Shift Register', NULL, 0, 'memory', 'stateful'),
    ('subtractor', 'Subtractor', NULL, 0, 'arithmetic', 'stateless'),
    ('t_flip_flop', 'T Flip-Flop', NULL, 0, 'memory', 'stateful'),
    ('threshold', 'Threshold', NULL, 0, 'logic', 'stateless'),
    ('xnor_gate', 'XNOR Gate', NULL, 0, 'gates', 'stateless'),
    ('xor_gate', 'XOR Gate', NULL, 0, 'gates', 'stateless');

INSERT INTO category_measure (category, measure, position) VALUES
    ('absolute_value', 'width', 1),
    ('adder', 'width', 1),
    ('alu', 'width', 1),
    ('and_gate', 'width', 1),
    ('barrel_shifter', 'width', 1),
    ('bcd_to_binary', 'width', 1),
    ('binary_to_bcd', 'width', 1),
    ('binary_to_hex', 'width', 1),
    ('buffer', 'width', 1),
    ('bussing', 'width', 1),
    ('carry_out', 'width', 1),
    ('carry_save_adder', 'width', 1),
    ('counter', 'width', 1),
    ('dataloop', 'width', 1),
    ('decoder', 'width', 1),
    ('decrementer', 'width', 1),
    ('display', 'width', 1),
    ('divider', 'width', 1),
    ('dual_edge_monostable', 'pulse', 1),
    ('equality', 'width', 1),
    ('greater_than', 'width', 1),
    ('hex_adder', 'width', 1),
    ('hex_to_binary', 'width', 1),
    ('inc_dec', 'width', 1),
    ('incrementer', 'width', 1),
    ('inverter', 'width', 1),
    ('line_drawer', 'width', 1),
    ('monostable', 'pulse', 1),
    ('multiplier', 'width', 1),
    ('nand_gate', 'width', 1),
    ('nand_nor_gate', 'width', 1),
    ('nand_xnor_gate', 'width', 1),
    ('nor_gate', 'width', 1),
    ('or_gate', 'width', 1),
    ('popcount', 'width', 1),
    ('ram', 'width', 1),
    ('ram', 'bytes', 2),
    ('register', 'width', 1),
    ('rng', 'width', 1),
    ('serial_adder', 'throughput', 1),
    ('serial_write_memory', 'width', 1),
    ('serial_write_memory', 'bytes', 2),
    ('shift_register', 'width', 1),
    ('subtractor', 'width', 1),
    ('threshold', 'width', 1),
    ('threshold', 'at_least', 2),
    ('xnor_gate', 'width', 1),
    ('xor_gate', 'width', 1);

INSERT INTO design (category, name, abbreviation, label) VALUES
    ('adder', 'carry_cancel', 'CCA', NULL),
    ('adder', 'carry_look_everywhere', 'CLE', NULL),
    ('adder', 'carry_lookahead', 'CLA', NULL),
    ('adder', 'hybrid', NULL, NULL),
    ('adder', 'ripple_carry', 'RCA', NULL),
    ('hex_adder', 'carry_cancel', 'CCA', NULL),
    ('hex_adder', 'carry_look_everywhere', 'CLE', NULL),
    ('hex_adder', 'carry_lookahead', 'CLA', NULL),
    ('hex_adder', 'hybrid', NULL, NULL),
    ('hex_adder', 'ripple_carry', 'RCA', NULL),
    ('rng', 'dropper', NULL, NULL),
    ('rng', 'lfsr', NULL, 'LFSR'),
    ('rng', 'xorshift', NULL, NULL),
    ('subtractor', 'carry_cancel', 'CCA', NULL),
    ('subtractor', 'carry_look_everywhere', 'CLE', NULL),
    ('subtractor', 'carry_lookahead', 'CLA', NULL),
    ('subtractor', 'hybrid', NULL, NULL),
    ('subtractor', 'ripple_carry', 'RCA', NULL);

INSERT INTO flag (category, name, label, position) VALUES
    ('adder', 'carry_in', 'Carry in', 1),
    ('adder', 'carry_out', 'Carry out', 2),
    ('adder', 'subtract', 'Subtract', 3),
    ('adder', 'inverted_carry_in', 'Inverted carry in', 5),
    ('alu', 'and', 'AND', 1),
    ('alu', 'or', 'OR', 2),
    ('alu', 'xor', 'XOR', 3),
    ('alu', 'shl', 'SHL', 4),
    ('alu', 'shr', 'SHR', 5),
    ('alu', 'carry_in', 'Carry in', 8),
    ('alu', 'carry_out', 'Carry out', 9),
    ('alu', 'overflow', 'Overflow', 10),
    ('alu', 'zero', 'Zero', 11),
    ('alu', 'nand', 'NAND', 12),
    ('alu', 'nor', 'NOR', 13),
    ('alu', 'xnor', 'XNOR', 14),
    ('alu', 'asr', 'ASR', 15),
    ('alu', 'nadd', 'NADD', 16),
    ('alu', 'nsub', 'NSUB', 17),
    ('barrel_shifter', 'bidirectional', 'Bidirectional', 1),
    ('barrel_shifter', 'rotate', 'Rotate', 2),
    ('barrel_shifter', 'arithmetic', 'Arithmetic', 3),
    ('dual_edge_monostable', 'inverted', 'Inverted', 1),
    ('greater_than', 'signed', 'Signed', 1),
    ('hex_adder', 'carry_in', 'Carry in', 1),
    ('hex_adder', 'carry_out', 'Carry out', 2),
    ('hex_adder', 'subtract', 'Subtract', 3),
    ('hex_adder', 'inverted_carry_in', 'Inverted carry in', 5),
    ('monostable', 'inverted', 'Inverted', 1),
    ('monostable', 'falling_edge', 'Falling edge', 2),
    ('multiplier', 'upper_product', 'Upper product', 1),
    ('multiplier', 'fma', 'FMA (A×B+C)', 2),
    ('register', 'inverted', 'Inverted reads', 1),
    ('rng', 'seedable', 'Seedable', 1),
    ('shift_register', 'bidirectional', 'Bidirectional', 1),
    ('shift_register', 'writeable', 'Writeable', 2),
    ('subtractor', 'carry_in', 'Carry in', 1),
    ('subtractor', 'carry_out', 'Carry out', 2),
    ('subtractor', 'inverted_carry_in', 'Inverted carry in', 5);

INSERT INTO category_alias (alias, category) VALUES
    ('abs', 'absolute_value'),
    ('and', 'and_gate'),
    ('and nand gate', 'and_gate'),
    ('and nand gate', 'nand_gate'),
    ('bcd', 'bcd_to_binary'),
    ('bcd', 'binary_to_bcd'),
    ('comparator', 'equality'),
    ('comparator', 'greater_than'),
    ('conditional inverter', 'inverter'),
    ('connect 4', 'four_in_a_row_detector'),
    ('connect4', 'four_in_a_row_detector'),
    ('controlled inverter', 'inverter'),
    ('cout', 'carry_out'),
    ('csa', 'carry_save_adder'),
    ('dec', 'decrementer'),
    ('eq', 'equality'),
    ('flip flop', 't_flip_flop'),
    ('gate', 'and_gate'),
    ('gate', 'nand_gate'),
    ('gate', 'nand_nor_gate'),
    ('gate', 'nand_xnor_gate'),
    ('gate', 'nor_gate'),
    ('gate', 'or_gate'),
    ('gate', 'xnor_gate'),
    ('gate', 'xor_gate'),
    ('gt', 'greater_than'),
    ('inc', 'incrementer'),
    ('majority', 'threshold'),
    ('mono', 'dual_edge_monostable'),
    ('mono', 'monostable'),
    ('monostable', 'dual_edge_monostable'),
    ('nand', 'nand_gate'),
    ('nor', 'nor_gate'),
    ('or', 'or_gate'),
    ('or nor gate', 'nor_gate'),
    ('or nor gate', 'or_gate'),
    ('pulse generator', 'dual_edge_monostable'),
    ('pulse generator', 'monostable'),
    ('tff', 't_flip_flop'),
    ('toggle', 't_flip_flop'),
    ('xnor', 'xnor_gate'),
    ('xor', 'xor_gate'),
    ('xor bank', 'inverter'),
    ('xor xnor gate', 'xnor_gate'),
    ('xor xnor gate', 'xor_gate');

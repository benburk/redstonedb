-- Minecraft players looked up by name, and their heads, kept so a page
-- never waits on Mojang for one asked before. A name no player has is
-- kept too, with no UUID, so it isn't asked about on every view.
CREATE TABLE player (
    -- In lowercase, as names are the same in any capitals.
    name TEXT NOT NULL PRIMARY KEY CHECK (name <> '' AND name = lower(name)),
    uuid TEXT CHECK (
        length(uuid) = 32 AND uuid NOT GLOB '*[^0-9a-f]*'
    ),
    -- The spelling the player chose.
    spelling TEXT CHECK (spelling <> ''),
    checked_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

-- A player's head, the 8×8 face of their skin as a PNG; NULL for a player
-- with no skin to draw.
CREATE TABLE head (
    uuid TEXT NOT NULL PRIMARY KEY CHECK (
        length(uuid) = 32 AND uuid NOT GLOB '*[^0-9a-f]*'
    ),
    png BLOB,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

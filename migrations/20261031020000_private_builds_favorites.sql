-- A build may be private: seen only by its uploader and admins, in no
-- listing, search, or leaderboard. Every build so far is public.
ALTER TABLE schematic ADD COLUMN public INTEGER NOT NULL DEFAULT 1
    CHECK (public IN (0, 1));

-- The builds each account has favourited.
CREATE TABLE favorite (
    user_id BLOB NOT NULL REFERENCES user (id) ON DELETE CASCADE,
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, schematic_id)
) STRICT;

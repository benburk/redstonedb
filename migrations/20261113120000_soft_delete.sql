-- A deleted build is kept, hidden from everyone, so an admin can restore
-- it from the moderation log. `deleted_at` holds when, NULL while it is
-- shown; `deletion` keeps each delete and restore for the log.

ALTER TABLE schematic ADD COLUMN deleted_at TEXT
    CHECK (deleted_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');

CREATE TABLE deletion (
    id INTEGER PRIMARY KEY,
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    -- NULL once the account is deleted.
    user_id BLOB REFERENCES user (id) ON DELETE SET NULL,
    -- 1 for a restore, 0 for a delete.
    restored INTEGER NOT NULL DEFAULT 0 CHECK (restored IN (0, 1)),
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        CHECK (at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
) STRICT;

CREATE INDEX deletion_at ON deletion (at);
CREATE INDEX deletion_schematic_id ON deletion (schematic_id);

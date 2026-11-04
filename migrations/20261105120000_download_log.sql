-- Downloading a build takes an account, and each download is kept: who
-- took which build, which version of it, what part, and when. The
-- counter on schematic stays as it was, counting the current file's
-- downloads alone.

CREATE TABLE download (
    id INTEGER PRIMARY KEY,
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    -- NULL once the account is deleted.
    user_id BLOB REFERENCES user (id) ON DELETE SET NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    -- The whole file, a stacking part of it, an earlier version, or the
    -- file sent to the account's Schemati.
    kind TEXT NOT NULL CHECK (kind IN ('file', 'part', 'version', 'schemati')),
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

CREATE INDEX download_by_schematic ON download (schematic_id, at);
CREATE INDEX download_by_user ON download (user_id, at);

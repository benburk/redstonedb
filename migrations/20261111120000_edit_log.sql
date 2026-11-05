-- Each edit of a build's details is kept, for the moderation log: who
-- edited which build, what the edit changed, and when. Edits from before
-- this was kept left only the build's edited_at, and no row here.

CREATE TABLE edit (
    id INTEGER PRIMARY KEY,
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    -- NULL once the account is deleted.
    user_id BLOB REFERENCES user (id) ON DELETE SET NULL,
    -- What it changed, as the log names it, such as `["description",
    -- "authors"]`; never empty, as an edit changing nothing is not kept.
    changed TEXT NOT NULL CHECK (
        json_valid(changed) AND json_type(changed) = 'array'
        AND json_array_length(changed) > 0
    ),
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        CHECK (at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
) STRICT;

CREATE INDEX edit_at ON edit (at);

-- The log reads every kind of event newest first.
CREATE INDEX download_at ON download (at);
CREATE INDEX schematic_version_added_at ON schematic_version (added_at);
CREATE INDEX user_created_at ON user (created_at);

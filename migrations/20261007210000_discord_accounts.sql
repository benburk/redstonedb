-- Accounts come from Discord: no passwords, emails, or invite codes. Every
-- account has a role. A user uploads builds; a mod also edits any build's
-- details and tags; an admin also sets roles.
DROP TABLE invite_code;
DROP TABLE session;
DROP TABLE user;

CREATE TABLE user (
    id BLOB NOT NULL PRIMARY KEY,
    discord_id TEXT NOT NULL UNIQUE,
    -- The Discord username, refreshed at each login.
    username TEXT NOT NULL UNIQUE,
    display_name TEXT,
    -- The Discord avatar's hash, from which its image address follows.
    avatar TEXT,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'mod', 'admin')),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE session (
    id BLOB NOT NULL PRIMARY KEY,
    user_id BLOB NOT NULL REFERENCES user (id) ON DELETE CASCADE,
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) STRICT;

-- The account that uploaded a build through the site. Builds imported
-- with rsdb name their importer in imported_by instead.
ALTER TABLE schematic ADD COLUMN uploaded_by BLOB
    REFERENCES user (id) ON DELETE SET NULL;

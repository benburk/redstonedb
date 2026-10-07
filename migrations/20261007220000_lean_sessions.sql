-- A session is only a random id, the account it logs in, and when it
-- expires. Nothing about the browser or its address is kept, and accounts
-- keep no join time. Recreating the table logs everyone out once.
DROP TABLE session;
CREATE TABLE session (
    id BLOB NOT NULL PRIMARY KEY,
    user_id BLOB NOT NULL REFERENCES user (id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
) STRICT;

ALTER TABLE user DROP COLUMN created_at;

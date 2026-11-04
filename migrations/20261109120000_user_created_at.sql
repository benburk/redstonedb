-- An account keeps one time it was created, a best guess for one from
-- before the time was kept, with no mark of which are guesses.

ALTER TABLE user DROP COLUMN joined_estimated;
ALTER TABLE user RENAME COLUMN joined_at TO created_at;

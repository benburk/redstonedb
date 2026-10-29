-- Whether the person has been shown the welcome steps, which a new account
-- sees once, at its first login; every account before them had no need.
ALTER TABLE user ADD COLUMN welcomed INTEGER NOT NULL DEFAULT 0
    CHECK (welcomed IN (0, 1));
UPDATE user SET welcomed = 1;

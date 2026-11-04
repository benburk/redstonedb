-- When each account joined: the moment of its first login from now on.
-- An account from before is given the earliest thing it is known to have
-- done, an upload, a version, a favourite, or a download, marked as an
-- estimate, as it joined on that day or before; one that has done none
-- keeps no date, as it joined before join dates were kept.

ALTER TABLE user ADD COLUMN joined_at TEXT;
ALTER TABLE user ADD COLUMN joined_estimated INTEGER NOT NULL DEFAULT 0
    CHECK (joined_estimated IN (0, 1));

UPDATE user SET joined_estimated = 1, joined_at = (
    SELECT min(at) FROM (
        SELECT added_at AS at FROM schematic WHERE uploaded_by = user.id
        UNION ALL
        SELECT added_at FROM schematic_version WHERE added_by = user.id
        UNION ALL
        SELECT added_at FROM favorite WHERE user_id = user.id
        UNION ALL
        SELECT at FROM download WHERE user_id = user.id
    )
)
WHERE id <> X'000000000000000000000000000000DB';

-- One with nothing to go by keeps no date, so marks no estimate.
UPDATE user SET joined_estimated = 0 WHERE joined_at IS NULL;

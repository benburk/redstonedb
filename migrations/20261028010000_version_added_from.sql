-- The IP address a version was uploaded from, as the proxy in front of
-- the site saw it; NULL for one imported with rsdb or uploaded before it
-- was kept.
ALTER TABLE schematic_version ADD COLUMN added_from TEXT
    CHECK (added_from <> '' AND length(added_from) <= 45);

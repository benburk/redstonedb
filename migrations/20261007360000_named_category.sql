-- A category may let each build give its own name, as bussing does: a bus
-- crossing and a bus splitter are both bussing.
ALTER TABLE category ADD COLUMN named INTEGER NOT NULL DEFAULT 0
    CHECK (named IN (0, 1));
UPDATE category SET named = 1 WHERE name = 'bussing';

ALTER TABLE schematic ADD COLUMN given_name TEXT
    CHECK (given_name <> '' AND length(given_name) <= 60);

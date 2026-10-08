-- How many times each build has been downloaded, stacks and all; the 3D
-- viewer's fetches are not downloads.
ALTER TABLE schematic ADD COLUMN downloads INTEGER NOT NULL DEFAULT 0
    CHECK (downloads >= 0);

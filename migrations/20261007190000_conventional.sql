-- Some builds, such as displays, follow their own layout rather than the
-- building conventions. They are imported without the lint, and the site
-- does not read an interface from them.
ALTER TABLE schematic ADD COLUMN conventional INTEGER NOT NULL DEFAULT 1
    CHECK (conventional IN (0, 1));

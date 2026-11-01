-- The colours a version's builder gave its building blocks, where they
-- differ from the stored file's: JSON of [[x, y, z], block] pairs in the
-- stored file's positions; NULL when none are kept.
ALTER TABLE schematic_version ADD COLUMN colours TEXT
    CHECK (colours IS NULL OR json_valid(colours));

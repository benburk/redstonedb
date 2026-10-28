-- The blocks the person likes builds shown and downloaded in, as JSON of
-- `palette::Palette`; NULL for the site's own.
ALTER TABLE user ADD COLUMN palette TEXT
    CHECK (palette IS NULL OR json_valid(palette));

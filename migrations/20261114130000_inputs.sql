-- How many inputs a build takes, for a category ranked by it, as a hex max
-- is by its digits in: read from its file, as its width is; NULL for any
-- other build.
ALTER TABLE build ADD COLUMN inputs INTEGER CHECK (inputs > 0);

-- How many outputs a build gives, for a category ranked by them, as a hex
-- input spread is by the copies of its digit: read from its file, as its
-- width is; NULL for any other build.
ALTER TABLE build ADD COLUMN outputs INTEGER CHECK (outputs > 0);

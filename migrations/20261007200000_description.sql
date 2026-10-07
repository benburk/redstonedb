-- A few sentences about a build: what it does, how to use it, quirks.
-- NULL means none written yet.
ALTER TABLE schematic ADD COLUMN description TEXT CHECK (description <> '');

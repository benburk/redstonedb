-- Who imported a build matters more than the filename it arrived under.
-- imported_by is an in-game name, like author; NULL means unknown. Every
-- build so far was imported by embizone.
ALTER TABLE schematic ADD COLUMN imported_by TEXT CHECK (
    imported_by <> '' AND imported_by NOT GLOB '*[^a-z0-9_]*'
);
UPDATE schematic SET imported_by = 'embizone';
ALTER TABLE schematic DROP COLUMN source_name;

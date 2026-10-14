-- The oldest Minecraft: Java Edition release a build works in, as
-- src/works_from.rs reads it from the redstone components and mechanics
-- its file uses, such as `1.5` for one with a comparator, and the component
-- or mechanic that sets it; NULL until read. A build that works from Alpha
-- is alpha, which is read from the file rather than tagged, so the stored
-- alpha tag goes.
ALTER TABLE schematic ADD COLUMN works_from TEXT CHECK (works_from <> '');
ALTER TABLE schematic ADD COLUMN works_from_by TEXT CHECK (works_from_by <> '');

DELETE FROM tag WHERE name = 'alpha';

-- How many redstone torches a build's file has, standing or on a wall,
-- counted whenever the file is stored; NULL until counted. A build with
-- none is torchless, which is read from the file rather than tagged, so
-- the stored torchless tag goes.
ALTER TABLE schematic ADD COLUMN torches INTEGER CHECK (torches >= 0);

DELETE FROM tag WHERE name = 'torchless';

-- Connect 4 is the game a four-in-a-row detector plays, so the tag only
-- said its category again.
DELETE FROM tag WHERE name = 'connect4';

-- Sizes were the schematic's full bounding box. Every build carries glass
-- corner markers in a one-block shell around it, so a build's size is the
-- box shrunk by one block on each face.
UPDATE schematic
SET size_x = size_x - 2, size_y = size_y - 2, size_z = size_z - 2;

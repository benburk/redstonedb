-- How much a memory holds, in bytes, as in a 32-byte RAM. A category says
-- whether its builds have a capacity.
ALTER TABLE category ADD COLUMN holds_bytes INTEGER NOT NULL DEFAULT 0
    CHECK (holds_bytes IN (0, 1));
ALTER TABLE schematic ADD COLUMN bytes INTEGER CHECK (bytes > 0);

UPDATE category SET holds_bytes = 1 WHERE name IN ('ram', 'serial_write_memory');

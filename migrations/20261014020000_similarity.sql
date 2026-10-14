-- How alike two builds of one category are, as src/similarity.rs measures
-- it: the share of their redstone components that line up, in tenths of a
-- percent. Each pair is kept once, the lesser id first.
CREATE TABLE similarity (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    other_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    permille INTEGER NOT NULL CHECK (permille BETWEEN 0 AND 1000),
    PRIMARY KEY (schematic_id, other_id),
    CHECK (schematic_id < other_id)
) STRICT;

-- The category and file hash, as `category/sha256`, a build was last
-- compared under; NULL until it is. A build whose file or category has
-- changed since is compared again.
ALTER TABLE schematic ADD COLUMN compared_as TEXT;

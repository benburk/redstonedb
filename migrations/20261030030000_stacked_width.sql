-- A stackable build stacked into a wider one, at each standard width above
-- its own its copies reach: how many copies, and what its timing measured
-- with them, so it competes on that width's leaderboard. NULL for what
-- could not be measured.
CREATE TABLE stacked_width (
    schematic_id TEXT NOT NULL
        REFERENCES "schematic" (id) ON DELETE CASCADE,
    width INTEGER NOT NULL CHECK (width > 0),
    copies INTEGER NOT NULL CHECK (copies >= 2),
    latency INTEGER,
    synchronized INTEGER CHECK (synchronized IN (0, 1)),
    throughput INTEGER,
    PRIMARY KEY (schematic_id, width)
) STRICT;

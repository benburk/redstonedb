-- Whether a category's designs all do the same job, so a leaderboard may
-- rank them together: every adder adds, but an XOR gate and a NAND+NOR gate
-- compute different things.
ALTER TABLE category ADD COLUMN designs_interchangeable INTEGER NOT NULL
    DEFAULT 1 CHECK (designs_interchangeable IN (0, 1));

UPDATE category SET designs_interchangeable = 0 WHERE name = 'gate';

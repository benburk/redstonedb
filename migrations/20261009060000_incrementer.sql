-- An incrementer adds 1, subtracts 1, or does either as a control line says.
-- Which it does is its design, and builds that do different things never
-- share a leaderboard. Its design names the build in place of the category,
-- as in "8-bit Decrementer".
ALTER TABLE category ADD COLUMN named_by_design INTEGER NOT NULL DEFAULT 0
    CHECK (named_by_design IN (0, 1));

INSERT INTO category
    (name, label, plural, section, designs_interchangeable, named_by_design)
VALUES ('incrementer', 'Incrementer', NULL, 'arithmetic', 0, 1);

INSERT INTO design (category, name, label) VALUES
    ('incrementer', 'increment', 'Incrementer'),
    ('incrementer', 'decrement', 'Decrementer'),
    ('incrementer', 'inc_dec', 'Inc/Dec');

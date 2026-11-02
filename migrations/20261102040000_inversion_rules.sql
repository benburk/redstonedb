-- Only a control line, such as a carry in, may read inverted; an output
-- that does is a wrong answer. A decoder's outputs are always inverted,
-- its selected bit off, and a buffer's never are, so neither has an
-- inverted flag, and an adder's, hex adder's, or subtractor's output and
-- carry out no longer read inverted. A register's reads, a monostable's
-- pulse, and any carry in keep theirs.
CREATE TEMPORARY TABLE dropped_flag (
    category TEXT NOT NULL,
    name TEXT NOT NULL
);
INSERT INTO dropped_flag (category, name) VALUES
    ('decoder', 'inverted'),
    ('buffer', 'inverted'),
    ('adder', 'inverted_output'),
    ('adder', 'inverted_carry_out'),
    ('hex_adder', 'inverted_output'),
    ('hex_adder', 'inverted_carry_out'),
    ('subtractor', 'inverted_output'),
    ('subtractor', 'inverted_carry_out');

DELETE FROM schematic_flag
WHERE EXISTS (
    SELECT 1 FROM dropped_flag
    JOIN schematic ON schematic.category = dropped_flag.category
    WHERE schematic.id = schematic_flag.schematic_id
        AND dropped_flag.name = schematic_flag.name
);

DELETE FROM flag
WHERE EXISTS (
    SELECT 1 FROM dropped_flag
    WHERE dropped_flag.category = flag.category
        AND dropped_flag.name = flag.name
);

DROP TABLE dropped_flag;

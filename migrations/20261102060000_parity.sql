-- Parity: input A and one output, on when an odd number of A's bits are,
-- the xor of them all, or with the flag Even, on when an even number are,
-- its inverse, as one torch on the output makes either of the other. The
-- simulation ticks it for a build that gives the even. Named and ranked by
-- width, A's, and filed under logic beside the threshold, the other build
-- that gives one bit of A's bits.
INSERT INTO category (name, label, plural, named, section, logic)
SELECT 'parity', 'Parity', NULL, 0, section, logic
FROM category WHERE name = 'threshold';

INSERT INTO category_measure (category, measure, position)
VALUES ('parity', 'width', 1);

INSERT INTO flag (category, name, label, position)
VALUES ('parity', 'even', 'Even', 1);

-- Searching what it is used as, or what it does, finds it.
INSERT INTO category_alias (alias, category) VALUES
    ('parity checker', 'parity'),
    ('parity generator', 'parity'),
    ('xor reduce', 'parity');

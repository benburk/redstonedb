-- Parity: input A and one output, on when an odd number of A's bits are,
-- the xor of them all. One on for an even number gives the inverse, which
-- is wrong. Named and ranked by width, A's, and filed under logic beside
-- the threshold, the other build that gives one bit of A's bits.
INSERT INTO category (name, label, plural, named, section, logic)
SELECT 'parity', 'Parity', NULL, 0, section, logic
FROM category WHERE name = 'threshold';

INSERT INTO category_measure (category, measure, position)
VALUES ('parity', 'width', 1);

-- Searching what it is used as, or what it does, finds it.
INSERT INTO category_alias (alias, category) VALUES
    ('parity checker', 'parity'),
    ('parity generator', 'parity'),
    ('xor reduce', 'parity');

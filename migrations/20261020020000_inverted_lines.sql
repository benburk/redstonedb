-- Lines read inverted, as a torch on each of their bits gives, which do
-- not change what a build is: a subtractor is an adder whose B is
-- inverted, its carries borrows, and an adder may read its output, carry
-- in, or carry out inverted; a register may read its cells inverted, and a
-- buffer show them so.
INSERT INTO flag (category, name, label, position)
SELECT 'adder', name, label, (SELECT max(position) FROM flag WHERE category = 'adder') + ordinal
FROM (
    SELECT 'subtract' AS name, 'Subtract' AS label, 1 AS ordinal
    UNION ALL SELECT 'inverted_output', 'Inverted output', 2
    UNION ALL SELECT 'inverted_carry_in', 'Inverted carry in', 3
    UNION ALL SELECT 'inverted_carry_out', 'Inverted carry out', 4
);
INSERT INTO flag (category, name, label, position)
SELECT category, 'inverted', label, coalesce((SELECT max(position) FROM flag AS f WHERE f.category = c.category), 0) + 1
FROM (
    SELECT 'register' AS category, 'Inverted reads' AS label
    UNION ALL SELECT 'buffer', 'Inverted'
) AS c;

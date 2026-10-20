-- What a decoder's and a barrel shifter's tests find: a decoder whose
-- outputs are inverted, the one A names off and the rest on, and a barrel
-- shifter's shifts, both ways, rotating, or keeping the sign.
INSERT INTO flag (category, name, label, position)
SELECT category, name, label, coalesce((SELECT max(position) FROM flag AS f WHERE f.category = c.category), 0) + ordinal
FROM (
    SELECT 'decoder' AS category, 'inverted' AS name, 'Inverted' AS label, 1 AS ordinal
    UNION ALL SELECT 'barrel_shifter', 'bidirectional', 'Bidirectional', 1
    UNION ALL SELECT 'barrel_shifter', 'rotate', 'Rotate', 2
    UNION ALL SELECT 'barrel_shifter', 'arithmetic', 'Arithmetic', 3
) AS c;

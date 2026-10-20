-- An ALU's add and subtract with every bit of the result inverted, as its
-- control lines' signs name them and its test checks them: NADD and NSUB,
-- which hand a result on through a torch the right way up.
INSERT INTO flag (category, name, label, position)
SELECT 'alu', name, label, (SELECT max(position) FROM flag WHERE category = 'alu') + ordinal
FROM (
    SELECT 'nadd' AS name, 'NADD' AS label, 1 AS ordinal
    UNION ALL SELECT 'nsub', 'NSUB', 2
);

-- The rest of the operations an ALU may offer beyond adding and
-- subtracting, as its control lines' signs name them and its test checks
-- them: NAND, NOR, XNOR, and the arithmetic shift right, which keeps the
-- sign bit.
INSERT INTO flag (category, name, label, position)
SELECT 'alu', name, label, (SELECT max(position) FROM flag WHERE category = 'alu') + ordinal
FROM (
    SELECT 'nand' AS name, 'NAND' AS label, 1 AS ordinal
    UNION ALL SELECT 'nor', 'NOR', 2
    UNION ALL SELECT 'xnor', 'XNOR', 3
    UNION ALL SELECT 'asr', 'ASR', 4
);

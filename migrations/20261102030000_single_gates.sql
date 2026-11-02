-- Each gate is a category of its own, so the builds ranked together do
-- one thing: XOR/XNOR Gate becomes XOR Gate and XNOR Gate, AND/NAND Gate
-- AND Gate and NAND Gate, and OR/NOR Gate OR Gate and NOR Gate. A build
-- flagged as the inverse becomes that gate, the flag then said by its
-- category.
INSERT INTO category (name, label, plural, named, section, logic)
SELECT gate.name, gate.label, NULL, pair.named, pair.section, pair.logic
FROM (
    SELECT 'xor_gate' AS name, 'XOR Gate' AS label, 'xor_xnor_gate' AS pair
    UNION ALL SELECT 'xnor_gate', 'XNOR Gate', 'xor_xnor_gate'
    UNION ALL SELECT 'and_gate', 'AND Gate', 'and_nand_gate'
    UNION ALL SELECT 'nand_gate', 'NAND Gate', 'and_nand_gate'
    UNION ALL SELECT 'or_gate', 'OR Gate', 'or_nor_gate'
    UNION ALL SELECT 'nor_gate', 'NOR Gate', 'or_nor_gate'
) AS gate
JOIN category AS pair ON pair.name = gate.pair;

CREATE TEMPORARY TABLE split_gate (
    pair TEXT NOT NULL,
    flag TEXT NOT NULL,
    plain TEXT NOT NULL,
    inverse TEXT NOT NULL
);
INSERT INTO split_gate (pair, flag, plain, inverse) VALUES
    ('xor_xnor_gate', 'xnor', 'xor_gate', 'xnor_gate'),
    ('and_nand_gate', 'nand', 'and_gate', 'nand_gate'),
    ('or_nor_gate', 'nor', 'or_gate', 'nor_gate');

INSERT INTO category_measure (category, measure, position)
SELECT gate, measure, position
FROM category_measure
JOIN (
    SELECT pair, plain AS gate FROM split_gate
    UNION ALL SELECT pair, inverse FROM split_gate
) ON category = pair;

UPDATE schematic SET category = (
    SELECT CASE
        WHEN EXISTS (
            SELECT 1 FROM schematic_flag
            WHERE schematic_id = schematic.id AND name = split_gate.flag
        ) THEN inverse
        ELSE plain
    END
    FROM split_gate WHERE pair = schematic.category
)
WHERE category IN (SELECT pair FROM split_gate);

DELETE FROM schematic_flag
WHERE name IN (SELECT flag FROM split_gate)
    AND schematic_id IN (
        SELECT id FROM schematic
        WHERE category IN (SELECT plain FROM split_gate)
            OR category IN (SELECT inverse FROM split_gate)
    );

-- A gate's stored result holds under its own gate's test, named for its
-- category, when it gave that gate's cases; any other runs again.
UPDATE simulation SET test = (
    SELECT category FROM schematic WHERE id = schematic_id
)
WHERE test IN (SELECT pair FROM split_gate)
    AND json_extract(report, '$.pattern') = (
        SELECT CASE category
            WHEN 'xor_gate' THEN 'A xor B'
            WHEN 'xnor_gate' THEN 'A xnor B'
            WHEN 'and_gate' THEN 'A and B'
            WHEN 'nand_gate' THEN 'A nand B'
            WHEN 'or_gate' THEN 'A or B'
            WHEN 'nor_gate' THEN 'A nor B'
        END
        FROM schematic WHERE id = schematic_id
    );

DELETE FROM simulation WHERE test IN (SELECT pair FROM split_gate);

-- Searching a gate finds it, `gate` finds every one, and a pair's old
-- name finds both its gates.
DELETE FROM category_alias WHERE category IN (SELECT pair FROM split_gate);
INSERT INTO category_alias (alias, category) VALUES
    ('xor', 'xor_gate'),
    ('xnor', 'xnor_gate'),
    ('and', 'and_gate'),
    ('nand', 'nand_gate'),
    ('or', 'or_gate'),
    ('nor', 'nor_gate'),
    ('gate', 'xor_gate'),
    ('gate', 'xnor_gate'),
    ('gate', 'and_gate'),
    ('gate', 'nand_gate'),
    ('gate', 'or_gate'),
    ('gate', 'nor_gate'),
    ('xor xnor gate', 'xor_gate'),
    ('xor xnor gate', 'xnor_gate'),
    ('and nand gate', 'and_gate'),
    ('and nand gate', 'nand_gate'),
    ('or nor gate', 'or_gate'),
    ('or nor gate', 'nor_gate');

-- Their flags and measures go with them.
DELETE FROM category WHERE name IN (SELECT pair FROM split_gate);

DROP TABLE split_gate;

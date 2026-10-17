-- XOR and XNOR gates are one category, XOR/XNOR Gate, as a torch on the
-- output makes either of the other, so they compete on one leaderboard.
-- Its flag XNOR marks a gate whose output is inverted; the XNOR gates
-- stored take it.
INSERT INTO category (name, label, plural, named, section, logic)
VALUES ('xor_xnor_gate', 'XOR/XNOR Gate', NULL, 0, 'gates', 'stateless');

INSERT INTO category_measure (category, measure, position)
VALUES ('xor_xnor_gate', 'width', 1);

INSERT INTO flag (category, name, label, position)
VALUES ('xor_xnor_gate', 'xnor', 'XNOR', 1);

INSERT INTO schematic_flag (schematic_id, name)
SELECT id, 'xnor' FROM schematic WHERE category = 'xnor_gate';

UPDATE schematic SET category = 'xor_xnor_gate'
WHERE category IN ('xor_gate', 'xnor_gate');

-- Searching `xor`, `xnor`, or `gate` finds it.
DELETE FROM category_alias WHERE category IN ('xor_gate', 'xnor_gate');
INSERT INTO category_alias (alias, category) VALUES
    ('xor', 'xor_xnor_gate'),
    ('xnor', 'xor_xnor_gate'),
    ('gate', 'xor_xnor_gate');

-- Their measures, designs, and flags go with them.
DELETE FROM category WHERE name IN ('xor_gate', 'xnor_gate');

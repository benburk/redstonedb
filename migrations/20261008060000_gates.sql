-- How people read a design when that is not just its name's words
-- capitalized, such as XOR; NULL for the name's words.
ALTER TABLE design ADD COLUMN label TEXT CHECK (label <> '');

INSERT INTO category (name, label, plural) VALUES ('gate', 'Gate', NULL);

INSERT INTO design (category, name, abbreviation, label) VALUES
    ('gate', 'xor', NULL, 'XOR'),
    ('gate', 'xnor', NULL, 'XNOR'),
    ('gate', 'nand_xnor', NULL, 'NAND+XNOR'),
    ('gate', 'nand_nor', NULL, 'NAND+NOR');

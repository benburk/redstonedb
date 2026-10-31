-- AND and OR gates join XOR as categories of two-input gates, each with
-- its inverse, as a torch on the output makes it: AND/NAND Gate and
-- OR/NOR Gate, as XOR/XNOR Gate is, the inverse taking a flag.
INSERT INTO category (name, label, plural, named, section, logic) VALUES
    ('and_nand_gate', 'AND/NAND Gate', NULL, 0, 'gates', 'stateless'),
    ('or_nor_gate', 'OR/NOR Gate', NULL, 0, 'gates', 'stateless');

INSERT INTO category_measure (category, measure, position) VALUES
    ('and_nand_gate', 'width', 1),
    ('or_nor_gate', 'width', 1);

INSERT INTO flag (category, name, label, position) VALUES
    ('and_nand_gate', 'nand', 'NAND', 1),
    ('or_nor_gate', 'nor', 'NOR', 1);

-- Searching `and`, `nand`, `or`, `nor`, or `gate` finds them.
INSERT INTO category_alias (alias, category) VALUES
    ('and', 'and_nand_gate'),
    ('nand', 'and_nand_gate'),
    ('gate', 'and_nand_gate'),
    ('or', 'or_nor_gate'),
    ('nor', 'or_nor_gate'),
    ('gate', 'or_nor_gate');

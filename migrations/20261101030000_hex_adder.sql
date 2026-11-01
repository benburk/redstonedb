-- Hex adders: adders whose A and B each take a hex digit per block, a
-- signal strength through dust, ranked apart from adders driven by levers.
INSERT INTO category (name, label, plural, named, section, logic)
SELECT 'hex_adder', 'Hex Adder', NULL, named, section, logic
FROM category WHERE name = 'adder';

INSERT INTO category_measure (category, measure, position)
SELECT 'hex_adder', measure, position
FROM category_measure WHERE category = 'adder';

INSERT INTO flag (category, name, label, position)
SELECT 'hex_adder', name, label, position FROM flag WHERE category = 'adder';

INSERT INTO design (category, name, abbreviation, label)
SELECT 'hex_adder', name, abbreviation, label
FROM design WHERE category = 'adder';

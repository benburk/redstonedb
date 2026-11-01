-- Subtractors: adders whose B is subtracted, A − B − borrow in, ranked
-- apart from adders. Each adder flagged subtract becomes one, the flag
-- then said by its category.
INSERT INTO category (name, label, plural, named, section, logic)
SELECT 'subtractor', 'Subtractor', NULL, named, section, logic
FROM category WHERE name = 'adder';

INSERT INTO category_measure (category, measure, position)
SELECT 'subtractor', measure, position
FROM category_measure WHERE category = 'adder';

INSERT INTO flag (category, name, label, position)
SELECT 'subtractor', name, label, position
FROM flag WHERE category = 'adder' AND name <> 'subtract';

INSERT INTO design (category, name, abbreviation, label)
SELECT 'subtractor', name, abbreviation, label
FROM design WHERE category = 'adder';

UPDATE schematic SET category = 'subtractor'
WHERE category = 'adder'
    AND id IN (SELECT schematic_id FROM schematic_flag WHERE name = 'subtract');

DELETE FROM schematic_flag
WHERE name = 'subtract'
    AND schematic_id IN (SELECT id FROM schematic WHERE category = 'subtractor');

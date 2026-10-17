-- A signed comparator is a Greater Than with the flag Signed, as inverting
-- each operand's top bit, two torches at any width, makes either of the
-- other. The simulation ticks it for a build that compares signed.
INSERT INTO flag (category, name, label, position)
VALUES ('greater_than', 'signed', 'Signed', 1);

INSERT INTO schematic_flag (schematic_id, name)
SELECT id, 'signed' FROM schematic WHERE category = 'signed_greater_than';

UPDATE schematic SET category = 'greater_than'
WHERE category = 'signed_greater_than';

DELETE FROM category_alias WHERE category = 'signed_greater_than';

-- Its measures go with it.
DELETE FROM category WHERE name = 'signed_greater_than';

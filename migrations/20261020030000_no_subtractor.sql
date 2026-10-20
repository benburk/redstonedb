-- A subtractor is an adder whose B is inverted, flagged subtract, so the
-- category has no builds of its own.
DELETE FROM category_alias WHERE category = 'subtractor';
DELETE FROM category_measure WHERE category = 'subtractor';
DELETE FROM design WHERE category = 'subtractor';
DELETE FROM flag WHERE category = 'subtractor';
DELETE FROM category WHERE name = 'subtractor';

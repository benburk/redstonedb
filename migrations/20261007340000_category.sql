-- What a build does, such as adder or bussing, is its category; it was called
-- its function. Bussing joins the list.
ALTER TABLE function RENAME TO category;
ALTER TABLE design RENAME COLUMN function TO category;
ALTER TABLE schematic RENAME COLUMN function TO category;

INSERT INTO category (name, label, plural, position)
VALUES ('bussing', 'Bussing', 'Bussing', (SELECT max(position) + 1 FROM category));

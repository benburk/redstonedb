-- A subtractor's lone lines are its borrow in and borrow out, as its test
-- reads them, and it takes the adder's designs, as the same carry chains
-- borrow as they carry.
UPDATE schematic_flag SET name = 'borrow_in'
WHERE name = 'carry_in'
    AND schematic_id IN (SELECT id FROM schematic WHERE category = 'subtractor');
UPDATE schematic_flag SET name = 'borrow_out'
WHERE name = 'carry_out'
    AND schematic_id IN (SELECT id FROM schematic WHERE category = 'subtractor');
UPDATE flag SET name = 'borrow_in', label = 'Borrow in'
WHERE category = 'subtractor' AND name = 'carry_in';
UPDATE flag SET name = 'borrow_out', label = 'Borrow out'
WHERE category = 'subtractor' AND name = 'carry_out';

INSERT INTO design (category, name, abbreviation, label)
SELECT 'subtractor', name, abbreviation, label FROM design WHERE category = 'adder';

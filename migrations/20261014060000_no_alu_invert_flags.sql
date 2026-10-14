-- An ALU's invert A and invert B lines are how it builds operations, such
-- as subtracting by inverting B with a carry in, not operations of their
-- own: two ALUs with the same operations have the same truth table however
-- they wire their inverters, so they are no flags. What the lines give is
-- found by trying them together.
DELETE FROM schematic_flag
WHERE name IN ('invert_a', 'invert_b')
    AND schematic_id IN (SELECT id FROM schematic WHERE category = 'alu');

DELETE FROM flag WHERE category = 'alu' AND name IN ('invert_a', 'invert_b');

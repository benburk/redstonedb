-- The function list holds only the standard functions; a build of any
-- other kind names its function freely. Every build has a design, `other`
-- unless its function lists a better one, so `other` belongs to no single
-- function.
DELETE FROM function WHERE name NOT IN (
    'adder', 'subtractor', 'multiplier', 'divider', 'register'
);
DELETE FROM design WHERE name = 'other';
UPDATE schematic SET design = 'other' WHERE design IS NULL;
-- With `other` always allowed, no function needs to require a design.
ALTER TABLE function DROP COLUMN design_required;

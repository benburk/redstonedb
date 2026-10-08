-- The sidebar's heading a standard category sits under. NULL puts it with
-- the categories off the list, under Other.
ALTER TABLE category ADD COLUMN section TEXT CHECK (section IN (
    'arithmetic', 'logic', 'conversion', 'memory', 'display', 'wiring'
));

UPDATE category SET section = 'arithmetic' WHERE name IN (
    'adder', 'subtractor', 'carry_save_adder', 'multiplier', 'divider', 'alu',
    'popcount'
);
UPDATE category SET section = 'logic' WHERE name IN (
    'gate', 'threshold', 'decoder', 'barrel_shifter'
);
UPDATE category SET section = 'conversion' WHERE name IN (
    'bcd_to_binary', 'binary_to_bcd', 'binary_to_hex', 'hex_to_binary'
);
UPDATE category SET section = 'memory' WHERE name IN (
    'ram', 'register', 'shift_register', 'serial_write_memory', 'dataloop',
    'counter'
);
UPDATE category SET section = 'display' WHERE name IN ('display', 'line_drawer');
UPDATE category SET section = 'wiring' WHERE name = 'bussing';

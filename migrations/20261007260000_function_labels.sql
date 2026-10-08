-- Each standard function gets the name people read, such as `RAM` or
-- `BCD to Binary`, and a place in the list.
ALTER TABLE function ADD COLUMN label TEXT NOT NULL DEFAULT '';
ALTER TABLE function ADD COLUMN position INTEGER NOT NULL DEFAULT 0;

INSERT OR IGNORE INTO function (name) VALUES
    ('serial_write_memory'),
    ('ram'),
    ('dataloop'),
    ('alu'),
    ('bcd_to_binary'),
    ('binary_to_bcd'),
    ('counter'),
    ('shift_register'),
    ('barrel_shifter'),
    ('carry_save_adder'),
    ('display');

UPDATE function SET (label, position) = (
    SELECT label, position FROM (
        SELECT 'adder' AS name, 'Adder' AS label, 1 AS position
        UNION ALL SELECT 'subtractor', 'Subtractor', 2
        UNION ALL SELECT 'multiplier', 'Multiplier', 3
        UNION ALL SELECT 'divider', 'Divider', 4
        UNION ALL SELECT 'register', 'Register', 5
        UNION ALL SELECT 'serial_write_memory', 'Serial Write Memory', 6
        UNION ALL SELECT 'ram', 'RAM', 7
        UNION ALL SELECT 'dataloop', 'Dataloop', 8
        UNION ALL SELECT 'alu', 'ALU', 9
        UNION ALL SELECT 'bcd_to_binary', 'BCD to Binary', 10
        UNION ALL SELECT 'binary_to_bcd', 'Binary to BCD', 11
        UNION ALL SELECT 'counter', 'Counter', 12
        UNION ALL SELECT 'shift_register', 'Shift Register', 13
        UNION ALL SELECT 'barrel_shifter', 'Barrel Shifter', 14
        UNION ALL SELECT 'carry_save_adder', 'Carry Save Adder', 15
        UNION ALL SELECT 'display', 'Display', 16
    ) AS labels WHERE labels.name = function.name
)
WHERE name IN (
    'adder', 'subtractor', 'multiplier', 'divider', 'register',
    'serial_write_memory', 'ram', 'dataloop', 'alu', 'bcd_to_binary',
    'binary_to_bcd', 'counter', 'shift_register', 'barrel_shifter',
    'carry_save_adder', 'display'
);

-- A build the site has no category for is under Other, named by its
-- author, rather than in a category of its own: `8-bit_serial_write_memory`
-- becomes Other, named `8-bit serial write memory`. A name made for a
-- category the site once meant to recognize, as `unrecognized_display`,
-- loses that word.
ALTER TABLE build ADD COLUMN name TEXT;

CREATE TEMP TABLE listed (name TEXT NOT NULL PRIMARY KEY);
INSERT INTO listed (name) VALUES
    ('adder'), ('hex_adder'), ('subtractor'), ('inverted_adder'), ('alu'),
    ('multiplier'), ('carry_save_adder'), ('incrementer'), ('decrementer'),
    ('equality'), ('greater_than'), ('hex_max'), ('carry_out'),
    ('absolute_value'), ('negator'), ('popcount'), ('threshold'),
    ('parity'), ('binary_to_hex'), ('hex_to_binary'), ('hex_input_spread'),
    ('xor_gate'), ('xnor_gate'), ('and_gate'), ('nand_gate'), ('or_gate'),
    ('nor_gate'), ('nand_xnor_gate'), ('inverter'), ('decoder'),
    ('barrel_shifter'), ('monostable'), ('dual_edge_monostable'),
    ('register'), ('ram'), ('buffer'), ('counter'), ('shift_register'),
    ('t_flip_flop');

UPDATE build SET
    name = replace(
        CASE WHEN category GLOB 'unrecognized_?*'
            THEN substr(category, length('unrecognized_') + 1)
            ELSE category END,
        '_', ' '),
    category = 'other'
WHERE category NOT IN (SELECT name FROM listed);

DROP TABLE listed;

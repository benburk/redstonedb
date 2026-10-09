-- A serial build takes its operands a bit at a time; its period is the
-- ticks from one bit to the next, as in a 2-tick serial adder. A category
-- says whether its builds have one.
ALTER TABLE category ADD COLUMN periodic INTEGER NOT NULL DEFAULT 0
    CHECK (periodic IN (0, 1));
ALTER TABLE schematic ADD COLUMN period INTEGER CHECK (period > 0);

INSERT INTO category (name, label, plural, section, periodic) VALUES
    ('serial_adder', 'Serial Adder', NULL, 'arithmetic', 1);

-- A monostable: input A turning on, or for a falling-edge one turning
-- off, gives a pulse of a set number of ticks on its output, and the other
-- edge gives nothing. A dual-edge monostable pulses on both, which no torch
-- on its input or output makes of a single-edge one, so it is a category of
-- its own. Their outputs rest the same either way, so they are stateless.
-- Each is named and ranked by its pulse, as in a 3-tick monostable;
-- Inverted marks one whose output rests on, so its pulse turns it off.
INSERT INTO category (name, label, plural, named, section, logic) VALUES
    ('monostable', 'Monostable', NULL, 0, 'logic', 'stateless'),
    ('dual_edge_monostable', 'Dual-Edge Monostable', NULL, 0, 'logic',
        'stateless');

-- A monostable's pulse, in ticks; NULL when unknown.
ALTER TABLE schematic ADD COLUMN pulse INTEGER CHECK (pulse > 0);

-- The measures' check gains the pulse. It is rebuilt to change the check.
CREATE TABLE category_measure_new (
    category TEXT NOT NULL REFERENCES category (name) ON DELETE CASCADE,
    measure TEXT NOT NULL CHECK (
        measure IN ('width', 'bytes', 'throughput', 'at_least', 'pulse')
    ),
    -- Its place among its category's measures.
    position INTEGER NOT NULL,
    PRIMARY KEY (category, measure)
) STRICT;
INSERT INTO category_measure_new (category, measure, position)
SELECT category, measure, position FROM category_measure;
DROP TABLE category_measure;
ALTER TABLE category_measure_new RENAME TO category_measure;

INSERT INTO category_measure (category, measure, position) VALUES
    ('monostable', 'pulse', 1),
    ('dual_edge_monostable', 'pulse', 1);

INSERT INTO flag (category, name, label, position) VALUES
    ('monostable', 'inverted', 'Inverted', 1),
    ('monostable', 'falling_edge', 'Falling edge', 2),
    ('dual_edge_monostable', 'inverted', 'Inverted', 1);

INSERT INTO category_alias (alias, category) VALUES
    ('mono', 'monostable'),
    ('mono', 'dual_edge_monostable'),
    ('monostable', 'dual_edge_monostable'),
    ('pulse generator', 'monostable'),
    ('pulse generator', 'dual_edge_monostable');

-- An inverter: a bus, A, and one control line that inverts every bit of
-- it, A xor the line, a row of XOR gates sharing one input. Stateless,
-- named and ranked by width; its copies add bits, sharing the line.
INSERT INTO category (name, label, plural, named, section, logic)
VALUES ('inverter', 'Inverter', NULL, 0, 'logic', 'stateless');

INSERT INTO category_measure (category, measure, position)
VALUES ('inverter', 'width', 1);

INSERT INTO category_alias (alias, category) VALUES
    ('conditional inverter', 'inverter'),
    ('controlled inverter', 'inverter'),
    ('xor bank', 'inverter');

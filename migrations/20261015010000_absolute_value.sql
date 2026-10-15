-- Absolute value: input A taken as a signed number, two's complement, and
-- its magnitude out, unsigned in as many bits, so the most negative A,
-- which has no positive, gives itself, as negating it does. Named and
-- ranked by width, and searched as abs.
INSERT INTO category (name, label, plural, named, section, logic)
VALUES ('absolute_value', 'Absolute Value', NULL, 0, 'arithmetic', 'comb');

INSERT INTO category_measure (category, measure, position)
VALUES ('absolute_value', 'width', 1);

INSERT INTO category_alias (alias, category)
VALUES ('abs', 'absolute_value');

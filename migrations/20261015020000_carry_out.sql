-- A carry out: of A + B, and a carry in when it has one, only the carry,
-- as a fast carry or an A >= -B test takes it, without the sum. Named and
-- ranked by width, and searched as cout.
INSERT INTO category (name, label, plural, named, section, logic)
VALUES ('carry_out', 'Carry Out', NULL, 0, 'arithmetic', 'comb');

INSERT INTO category_measure (category, measure, position)
VALUES ('carry_out', 'width', 1);

INSERT INTO category_alias (alias, category)
VALUES ('cout', 'carry_out');

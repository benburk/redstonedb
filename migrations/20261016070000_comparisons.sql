-- Comparisons of two operands of one width, each giving one line: whether
-- A is greater than B, unsigned or in two's complement, which disagree
-- whenever the top bits differ, so each is a category, and whether A
-- equals B, the same either way. Named and ranked by width; `comparator`
-- finds all three.
-- Their labels read as many, as BCD to Binary's does.
INSERT INTO category (name, label, plural, named, section, logic) VALUES
    ('greater_than', 'Greater Than', 'Greater Than', 0, 'arithmetic',
        'stateless'),
    ('signed_greater_than', 'Signed Greater Than', 'Signed Greater Than', 0,
        'arithmetic', 'stateless'),
    ('equality', 'Equality', 'Equality', 0, 'arithmetic', 'stateless');

INSERT INTO category_measure (category, measure, position) VALUES
    ('greater_than', 'width', 1),
    ('signed_greater_than', 'width', 1),
    ('equality', 'width', 1);

INSERT INTO category_alias (alias, category) VALUES
    ('comparator', 'greater_than'),
    ('comparator', 'signed_greater_than'),
    ('comparator', 'equality'),
    ('gt', 'greater_than'),
    ('eq', 'equality');

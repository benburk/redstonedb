-- A buffer: a register without reads, each cell showing what it holds on
-- its own bus all the time, written by a button, or by a lever that passes
-- data in through while it is on. Stateful, named and ranked by width.
INSERT INTO category (name, label, plural, named, section, logic)
VALUES ('buffer', 'Buffer', NULL, 0, 'memory', 'stateful');

INSERT INTO category_measure (category, measure, position)
VALUES ('buffer', 'width', 1);


-- T flip-flops: one bit held, toggled by each pulse of one input, as a
-- monostable has no width to measure.
INSERT INTO category (name, label, plural, named, section, logic) VALUES
    ('t_flip_flop', 'T Flip-Flop', NULL, 0, 'memory', 'stateful');

-- Searching `tff`, `toggle`, or `flip flop` finds them.
INSERT INTO category_alias (alias, category) VALUES
    ('tff', 't_flip_flop'),
    ('toggle', 't_flip_flop'),
    ('flip flop', 't_flip_flop');

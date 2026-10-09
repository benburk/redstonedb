-- The operations an ALU offers beyond adding and subtracting, and whether
-- it can invert either operand first.
INSERT INTO flag (category, name, label, position) VALUES
    ('alu', 'and', 'AND', 1),
    ('alu', 'or', 'OR', 2),
    ('alu', 'xor', 'XOR', 3),
    ('alu', 'shl', 'SHL', 4),
    ('alu', 'shr', 'SHR', 5),
    ('alu', 'invert_a', 'Invert A', 6),
    ('alu', 'invert_b', 'Invert B', 7);

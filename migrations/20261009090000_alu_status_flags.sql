-- What an ALU reports and takes besides its result: carry out, overflow,
-- and zero status lines, and a carry in. The carry flags share the
-- adder's names, so the same signs tick them.
INSERT INTO flag (category, name, label, position) VALUES
    ('alu', 'carry_in', 'Carry in', 8),
    ('alu', 'carry_out', 'Carry out', 9),
    ('alu', 'overflow', 'Overflow', 10),
    ('alu', 'zero', 'Zero', 11);

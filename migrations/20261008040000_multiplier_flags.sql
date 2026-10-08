-- A multiplier may output the upper half of its product as well, so 8 by 8
-- bits gives 16, and may add a third operand, as a fused multiply-add.
INSERT INTO flag (category, name, label, position) VALUES
    ('multiplier', 'upper_product', 'Upper product', 1),
    ('multiplier', 'fma', 'FMA (A×B+C)', 2);

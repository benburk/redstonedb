-- How the sidebar names many builds of a function, such as `Adders`; empty
-- means the label with an `s`.
ALTER TABLE function ADD COLUMN plural TEXT NOT NULL DEFAULT '';

UPDATE function SET plural = CASE name
    WHEN 'serial_write_memory' THEN 'Serial Write Memories'
    WHEN 'ram' THEN 'RAM'
    WHEN 'alu' THEN 'ALUs'
    WHEN 'bcd_to_binary' THEN 'BCD to Binary'
    WHEN 'binary_to_bcd' THEN 'Binary to BCD'
    ELSE ''
END;

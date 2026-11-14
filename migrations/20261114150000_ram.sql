-- RAM is a standard category: `8-bit_ram` is a RAM of 8 bits, its width
-- kept when it has none, and `unrecognized_ram` a RAM. A reason stored
-- already stays; a build with none takes the one a build kept as built
-- has, until `rsdb refresh` judges it again and says what keeps it from
-- being recognized.
UPDATE build SET
    category = 'ram',
    width = coalesce(width, nullif(CAST(category AS INTEGER), 0)),
    unverified = coalesce(unverified, 'Couldn''t read its inputs and outputs')
WHERE category GLOB '[0-9]*-bit_ram'
    AND substr(category, 1, instr(category, '-bit_') - 1) NOT GLOB '*[^0-9]*';

UPDATE build SET
    category = 'ram',
    unverified = coalesce(unverified, 'Couldn''t read its inputs and outputs')
WHERE category = 'unrecognized_ram';

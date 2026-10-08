-- In-game names keep their capitals, such as FEARLESS_Z_, but compare
-- without them, so one person is credited once whichever way they are typed.
CREATE TABLE credit_new (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('builder', 'contributor')),
    name TEXT NOT NULL COLLATE NOCASE CHECK (
        name <> '' AND name NOT GLOB '*[^A-Za-z0-9_]*'
    ),
    position INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, role, name)
) STRICT;

INSERT INTO credit_new (schematic_id, role, name, position)
SELECT schematic_id, role, name, position FROM credit;

DROP TABLE credit;
ALTER TABLE credit_new RENAME TO credit;

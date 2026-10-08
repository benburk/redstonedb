-- Builders and contributors share one table: each credit names a person, in
-- the order given within its role.
CREATE TABLE credit (
    schematic_id TEXT NOT NULL REFERENCES schematic (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('builder', 'contributor')),
    name TEXT NOT NULL CHECK (
        name <> '' AND name NOT GLOB '*[^a-z0-9_]*'
    ),
    position INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, role, name)
) STRICT;

INSERT INTO credit (schematic_id, role, name, position)
SELECT schematic_id, 'builder', name, position FROM builder;

DROP TABLE builder;

-- Logic is stored by the names people read it by: `stateless` for what was
-- `comb`, a build whose outputs depend only on its inputs, and `stateful`
-- for what was `seq`, one that remembers. A column's check goes with the
-- column, so each table gains the renamed column and drops the old one.
ALTER TABLE schematic ADD COLUMN logic_named TEXT
    CHECK (logic_named IN ('stateless', 'stateful'));
UPDATE schematic SET logic_named = CASE logic
    WHEN 'comb' THEN 'stateless'
    WHEN 'seq' THEN 'stateful'
END;
ALTER TABLE schematic DROP COLUMN logic;
ALTER TABLE schematic RENAME COLUMN logic_named TO logic;

ALTER TABLE category ADD COLUMN logic_named TEXT
    CHECK (logic_named IN ('stateless', 'stateful'));
UPDATE category SET logic_named = CASE logic
    WHEN 'comb' THEN 'stateless'
    WHEN 'seq' THEN 'stateful'
END;
ALTER TABLE category DROP COLUMN logic;
ALTER TABLE category RENAME COLUMN logic_named TO logic;

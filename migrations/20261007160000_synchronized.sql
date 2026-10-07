-- Whether every output bit settles on the same tick is one yes-or-no fact
-- that may be unknown, so it is a column rather than a pair of tags. NULL
-- means unknown.
ALTER TABLE schematic ADD COLUMN synchronized INTEGER CHECK (
    synchronized IN (0, 1)
);

UPDATE schematic SET synchronized = 1 WHERE id IN (
    SELECT schematic_id FROM schematic_tag
    JOIN tag ON tag.id = schematic_tag.tag_id
    WHERE tag.name = 'synchronized'
);
UPDATE schematic SET synchronized = 0 WHERE id IN (
    SELECT schematic_id FROM schematic_tag
    JOIN tag ON tag.id = schematic_tag.tag_id
    WHERE tag.name = 'unsynchronized'
);

DELETE FROM schematic_tag WHERE tag_id IN (
    SELECT id FROM tag WHERE name IN ('synchronized', 'unsynchronized')
);
DELETE FROM tag WHERE name IN ('synchronized', 'unsynchronized');

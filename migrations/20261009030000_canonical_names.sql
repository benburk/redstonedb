-- A Minecraft name is the same name in any capitals, but its player spells
-- it one way. Once an admin confirms an account's name, its spelling is
-- that one: credits and importers already stored take it.
UPDATE credit SET name = (
    SELECT minecraft_name FROM user
    WHERE minecraft_verified AND minecraft_name = credit.name COLLATE NOCASE
) WHERE EXISTS (
    SELECT 1 FROM user
    WHERE minecraft_verified AND minecraft_name = credit.name COLLATE NOCASE
);

UPDATE schematic SET imported_by = (
    SELECT minecraft_name FROM user
    WHERE minecraft_verified
        AND minecraft_name = schematic.imported_by COLLATE NOCASE
) WHERE EXISTS (
    SELECT 1 FROM user
    WHERE minecraft_verified
        AND minecraft_name = schematic.imported_by COLLATE NOCASE
);

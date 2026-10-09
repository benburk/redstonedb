-- The player behind an account's Minecraft name, as Mojang knows them: it
-- stays theirs through renames. NULL when Mojang could not be asked.
ALTER TABLE user ADD COLUMN minecraft_uuid TEXT CHECK (
    length(minecraft_uuid) = 32 AND minecraft_uuid NOT GLOB '*[^0-9a-f]*'
);
CREATE UNIQUE INDEX user_minecraft_uuid ON user (minecraft_uuid);

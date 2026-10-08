-- Each account may give its Minecraft name, so its builds can be found. One
-- name belongs to one account, whatever its case.
ALTER TABLE user ADD COLUMN minecraft_name TEXT COLLATE NOCASE CHECK (
    minecraft_name <> ''
    AND length(minecraft_name) <= 16
    AND minecraft_name NOT GLOB '*[^A-Za-z0-9_]*'
);

CREATE UNIQUE INDEX user_minecraft_name ON user (minecraft_name);

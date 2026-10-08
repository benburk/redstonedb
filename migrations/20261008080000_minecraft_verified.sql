-- Whether an admin has confirmed the account's Minecraft name is the
-- person's own. Names are typed by their owners, so only a confirmed one
-- lets its account edit the builds crediting it as builder.
ALTER TABLE user ADD COLUMN minecraft_verified INTEGER NOT NULL DEFAULT 0
    CHECK (minecraft_verified IN (0, 1));

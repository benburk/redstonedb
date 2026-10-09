-- Whether the build's file keeps where its uploader's //paste put it,
-- rather than pasting from one block south of its south-east-bottom
-- corner, as files do unless the uploader asks.
ALTER TABLE schematic ADD COLUMN keeps_paste INTEGER NOT NULL DEFAULT 0
    CHECK (keeps_paste IN (0, 1));

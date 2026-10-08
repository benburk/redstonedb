-- When a person last changed a build's details, through the edit page or
-- `rsdb set`; NULL for a build never edited.
ALTER TABLE schematic ADD COLUMN edited_at TEXT;

-- A tag's restriction, timing, or other is its kind, now that a build's
-- category means what it does.
ALTER TABLE tag RENAME COLUMN category TO kind;

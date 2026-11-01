-- Why a build of a category the site recognizes builds of isn't
-- recognized as one, such as `It does not pass the adder test`; NULL for
-- one that is, or any other build. Such a build is kept as built, under
-- the category its uploader gave, and listed only when asked for.
ALTER TABLE schematic ADD COLUMN unverified TEXT
    CHECK (unverified IS NULL OR unverified <> '');

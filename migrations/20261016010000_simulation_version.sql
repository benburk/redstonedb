-- A stored result is stale when its file or its test has changed since it
-- ran: `version` is the test's version it ran under (see
-- `simulate::Test::version`), and every result before versions were kept
-- takes 0, so each runs once more. `failed` says whether the build got
-- cases wrong or is broken, as `simulate::Report::failed` does, so a
-- listing need not read the report.
ALTER TABLE simulation ADD COLUMN version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE simulation ADD COLUMN failed INTEGER NOT NULL DEFAULT 0
    CHECK (failed IN (0, 1));
UPDATE simulation SET failed = (
    coalesce(json_extract(report, '$.wrong'), 0) > 0
    OR json_extract(report, '$.outcome') = 'broken'
);

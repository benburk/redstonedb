-- A result no longer records the version of its test: a change to a test
-- leaves stored results as they are until a build is simulated again, by
-- hand, so nothing reruns on its own.
ALTER TABLE simulation DROP COLUMN version;

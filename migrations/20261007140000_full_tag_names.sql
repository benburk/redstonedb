-- Tags are named in full words, with underscores between them, for people.
-- A tag whose filename token is an abbreviation keeps it in `token`; a tag
-- without one is written in filenames under its name.
ALTER TABLE tag ADD COLUMN token TEXT CHECK (
    token <> '' AND token NOT GLOB '*[^a-z0-9_]*'
);
CREATE UNIQUE INDEX tag_token ON tag (token);

UPDATE tag SET token = name, name = 'sequential' WHERE name = 'seq';
UPDATE tag SET token = name, name = 'combinational' WHERE name = 'comb';
UPDATE tag SET token = name, name = 'pipelined' WHERE name = 'pipe';
UPDATE tag SET token = name, name = 'synchronized' WHERE name = 'sync';
UPDATE tag SET token = name, name = 'unsynchronized' WHERE name = 'unsync';
UPDATE tag SET name = 'carry_lookahead' WHERE name = 'cla';

-- Random number generators. Width is the bits of each number. LFSR and
-- xorshift are pseudo-random and repeat once their state cycles; a dropper
-- design reads the game's own randomness. All give random numbers, so they
-- may share a leaderboard.
INSERT INTO category (name, label, plural, section) VALUES
    ('rng', 'RNG', 'RNGs', 'logic');

INSERT INTO design (category, name, label) VALUES
    ('rng', 'lfsr', 'LFSR'),
    ('rng', 'xorshift', NULL),
    ('rng', 'dropper', NULL);

INSERT INTO flag (category, name, label, position) VALUES
    ('rng', 'seedable', 'Seedable', 1);

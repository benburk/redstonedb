-- Popcount outputs how many of its inputs are on; a threshold outputs
-- whether at least k are. Each is as wide as its inputs. A threshold's
-- design is its k, and builds of different k compute different things, so
-- never share a leaderboard.
INSERT INTO category (name, label, plural, designs_interchangeable) VALUES
    ('popcount', 'Popcount', NULL, 1),
    ('threshold', 'Threshold', NULL, 0);

-- Labelled by their words, such as At Least 3, so filenames stay typeable
-- in Minecraft's chat, as `//schem load` needs.
INSERT INTO design (category, name) VALUES
    ('threshold', 'at_least_2'),
    ('threshold', 'at_least_3'),
    ('threshold', 'at_least_4'),
    ('threshold', 'at_least_5'),
    ('threshold', 'at_least_6'),
    ('threshold', 'at_least_7'),
    ('threshold', 'at_least_8');

-- What each of a version's lines is for, its role, kept apart from its
-- file, whose blocks keep the colours its builder gave them: probing finds
-- the roles, and a theme shows them in its colours.

-- JSON of `inputs` and `outputs`, each a list of lines, each with its
-- `role` (`A`, `B`, and on for an operand, `control`, `output`, `output 2`,
-- and on for an output bus, `status`, or null for a lever no role was
-- found for), its `name`, whether a sign names it (`signed`), and its
-- `bits`, each [x, y, z], least significant first, as `interface::Kept`
-- writes them; NULL until the version's roles are found.
ALTER TABLE schematic_version ADD COLUMN roles TEXT
    CHECK (roles IS NULL OR json_valid(roles));

DROP VIEW current_file;

CREATE VIEW current_file AS
SELECT schematic_id, version, data, sha256, roles FROM schematic_version
WHERE version = (
    SELECT max(version) FROM schematic_version AS newest
    WHERE newest.schematic_id = schematic_version.schematic_id
);

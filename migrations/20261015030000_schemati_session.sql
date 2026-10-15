-- The account's Schemati session, the `sessionId` cookie Schemati's own
-- Discord login sets, which opens the folder of the player linked to it.
-- NULL when none is given. Schemati forgets a session within a day.
ALTER TABLE user ADD COLUMN schemati_session TEXT;

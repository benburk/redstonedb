-- What the account's Schemati session is known to be, so a page can say
-- whose folder it opens without asking Schemati. The player is the one
-- Schemati greeted when it last answered with the session, and
-- `schemati_checked_at` when that was; both NULL for a session given
-- before they were kept, until Schemati next answers with it.
ALTER TABLE user ADD COLUMN schemati_player TEXT;
ALTER TABLE user ADD COLUMN schemati_checked_at TEXT;
-- When Schemati first said it no longer knows the session; NULL while it
-- still does. Answering with it again, or a new session, clears it.
ALTER TABLE user ADD COLUMN schemati_expired_at TEXT;

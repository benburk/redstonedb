-- Add email field to user table
-- Email is optional (nullable) for backwards compatibility with existing users

ALTER TABLE user ADD COLUMN email TEXT;

-- Create index for email lookups
CREATE INDEX idx_user_email ON user(email);

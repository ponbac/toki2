-- Add push_enabled column to notification_rules table
ALTER TABLE notification_rules
ADD COLUMN push_enabled BOOLEAN NOT NULL DEFAULT false;

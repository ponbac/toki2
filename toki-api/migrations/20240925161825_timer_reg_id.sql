-- Add registration_id column to timer_history table
ALTER TABLE timer_history
ADD COLUMN registration_id TEXT;

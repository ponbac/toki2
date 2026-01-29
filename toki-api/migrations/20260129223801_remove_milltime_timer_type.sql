-- Remove deprecated Milltime timer type
-- All timers are now Standalone type

-- First, update any existing 'milltime' timer_type values to 'standalone'
UPDATE timer_history SET timer_type = 'standalone' WHERE timer_type = 'milltime';

-- Drop the old constraint and add a new one that only allows 'standalone'
ALTER TABLE timer_history DROP CONSTRAINT IF EXISTS timer_history_timer_type_check;
ALTER TABLE timer_history ADD CONSTRAINT timer_history_timer_type_check CHECK (timer_type = 'standalone');

-- Update default to be explicit about standalone
ALTER TABLE timer_history ALTER COLUMN timer_type SET DEFAULT 'standalone';

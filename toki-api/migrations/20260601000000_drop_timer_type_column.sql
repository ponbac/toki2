-- Completely remove timer_type column from timer_history table
-- The timer_type concept is deprecated - all timers are standalone now.

-- Drop the constraint first
ALTER TABLE timer_history DROP CONSTRAINT IF EXISTS timer_history_timer_type_check;

-- Drop the column entirely
ALTER TABLE timer_history DROP COLUMN IF EXISTS timer_type;

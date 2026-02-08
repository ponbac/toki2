-- Remove deprecated Milltime timer type
-- Delete all milltime timers and drop the timer_type column entirely

-- Delete any milltime timer entries (user doesn't need to keep them)
DELETE FROM timer_history WHERE timer_type = 'milltime';

-- Drop the constraint
ALTER TABLE timer_history DROP CONSTRAINT IF EXISTS timer_history_timer_type_check;

-- Drop the column entirely - the concept of timer types is deprecated
ALTER TABLE timer_history DROP COLUMN IF EXISTS timer_type;

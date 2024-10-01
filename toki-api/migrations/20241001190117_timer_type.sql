ALTER TABLE timer_history
ADD COLUMN timer_type TEXT NOT NULL DEFAULT 'milltime'
CHECK (timer_type IN ('milltime', 'standalone'));

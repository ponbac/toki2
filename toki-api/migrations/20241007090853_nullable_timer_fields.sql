ALTER TABLE timer_history
    ALTER COLUMN project_id DROP NOT NULL,
    ALTER COLUMN project_name DROP NOT NULL,
    ALTER COLUMN activity_id DROP NOT NULL,
    ALTER COLUMN activity_name DROP NOT NULL,
    ALTER COLUMN note DROP NOT NULL;

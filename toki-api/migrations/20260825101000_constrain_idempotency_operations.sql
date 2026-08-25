ALTER TABLE time_tracking_idempotency
    ADD CONSTRAINT time_tracking_idempotency_operation_check
    CHECK (operation IN ('save_active_timer', 'create_time_entry'));

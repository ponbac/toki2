use super::*;
use crate::domain::models::{
    BeginTimeEntryWrite, CreateAbsenceDay, PendingTimeEntryWrite, PreparedTimeEntry,
    TimeEntryWriteIntent, TimeEntryWriteOperation, TimeEntryWriteOrigin, TimeEntryWriteResolution,
    TimerHistoryId, TimerId,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};
use time::Duration;

enum MockBegin {
    Fresh,
    Pending(String),
    Replay(PreparedTimeEntry, String),
    PayloadMismatch,
}

#[derive(Default)]
struct MockTimeTrackingClient {
    created_request: Mutex<Option<CreateTimeEntryRequest>>,
    created_count: AtomicUsize,
    create_error: Mutex<Option<TimeTrackingError>>,
    found_entry: Mutex<Option<TimerId>>,
    existing_entry: Mutex<Option<TimeEntry>>,
    day_statuses: Vec<TimeEntryDayStatus>,
    absence_types: Vec<AbsenceType>,
    absence_children: Vec<AbsenceChild>,
}

#[async_trait]
impl TimeTrackingClient for MockTimeTrackingClient {
    async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        Ok(vec![Project::new("project-1", "Project")])
    }

    async fn get_activities(
        &self,
        _project_id: &ProjectId,
        _date_range: (Date, Date),
    ) -> Result<Vec<Activity>, TimeTrackingError> {
        Ok(vec![Activity::new("activity-1", "Activity", "project-1")])
    }

    async fn get_time_info(
        &self,
        _date_range: (Date, Date),
    ) -> Result<WeeklyStats, TimeTrackingError> {
        unused_mock_method()
    }

    async fn get_time_entries(
        &self,
        _date_range: (Date, Date),
    ) -> Result<Vec<TimeEntry>, TimeTrackingError> {
        unused_mock_method()
    }

    async fn get_time_entry_day_statuses(
        &self,
        _date_range: (Date, Date),
    ) -> Result<Vec<TimeEntryDayStatus>, TimeTrackingError> {
        Ok(self.day_statuses.clone())
    }

    async fn create_time_entry(
        &self,
        request: &CreateTimeEntryRequest,
        _operation_id: &str,
    ) -> Result<TimerId, TimeTrackingError> {
        self.created_count.fetch_add(1, Ordering::SeqCst);
        *self.created_request.lock().unwrap() = Some(request.clone());
        if let Some(error) = self.create_error.lock().unwrap().take() {
            return Err(error);
        }
        Ok(TimerId::new("entry-1"))
    }

    async fn find_time_entry_by_operation_id(
        &self,
        _operation_id: &str,
        _date: Date,
    ) -> Result<Option<TimerId>, TimeTrackingError> {
        Ok(self.found_entry.lock().unwrap().clone())
    }

    async fn get_time_entry(&self, _registration_id: &str) -> Result<TimeEntry, TimeTrackingError> {
        self.existing_entry
            .lock()
            .unwrap()
            .clone()
            .ok_or(TimeTrackingError::TimerNotFound)
    }

    async fn edit_time_entry(
        &self,
        _registration_id: &str,
        _request: &EditTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError> {
        unused_mock_method()
    }

    async fn delete_time_entry(&self, _registration_id: &str) -> Result<(), TimeTrackingError> {
        unused_mock_method()
    }

    async fn get_absence_types(&self) -> Result<Vec<AbsenceType>, TimeTrackingError> {
        Ok(self.absence_types.clone())
    }

    async fn get_absence_children(&self) -> Result<Vec<AbsenceChild>, TimeTrackingError> {
        Ok(self.absence_children.clone())
    }

    async fn get_absences(
        &self,
        _date_range: (Date, Date),
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
        unused_mock_method()
    }

    async fn get_absence_day_defaults(
        &self,
        _date_range: (Date, Date),
    ) -> Result<Vec<AbsenceDayDefault>, TimeTrackingError> {
        unused_mock_method()
    }

    async fn create_absences(
        &self,
        _request: &CreateAbsencesRequest,
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
        unused_mock_method()
    }

    async fn delete_absence(
        &self,
        _absence_id: &str,
        _date: Date,
    ) -> Result<(), TimeTrackingError> {
        unused_mock_method()
    }
}

#[derive(Default)]
struct MockTimerHistoryRepository {
    active_timer: Mutex<Option<ActiveTimer>>,
    saved_end_time: Mutex<Option<OffsetDateTime>>,
    begins: Mutex<VecDeque<MockBegin>>,
    pending_write: Mutex<Option<PendingTimeEntryWrite>>,
    completed: Mutex<Vec<TimeEntry>>,
    cancellations: AtomicUsize,
    fail_save_once: AtomicBool,
}

#[async_trait]
impl TimerHistoryRepository for MockTimerHistoryRepository {
    async fn get_active_timer(
        &self,
        _user_id: &UserId,
    ) -> Result<Option<ActiveTimer>, TimeTrackingError> {
        Ok(self.active_timer.lock().unwrap().clone())
    }

    async fn create_timer(
        &self,
        _user_id: &UserId,
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError> {
        let mut active = self.active_timer.lock().unwrap();
        if active.is_some() {
            return Err(TimeTrackingError::TimerAlreadyRunning);
        }
        *active = Some(timer.clone());
        Ok(())
    }

    async fn update_timer(
        &self,
        _user_id: &UserId,
        _timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError> {
        unused_mock_method()
    }

    async fn delete_timer(&self, _user_id: &UserId) -> Result<(), TimeTrackingError> {
        *self.active_timer.lock().unwrap() = None;
        Ok(())
    }

    async fn get_history(
        &self,
        _user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
        unused_mock_method()
    }

    async fn get_by_registration_id(
        &self,
        _registration_id: &str,
    ) -> Result<Option<TimerHistoryEntry>, TimeTrackingError> {
        unused_mock_method()
    }

    async fn update_times(
        &self,
        _registration_id: &str,
        _start_time: &OffsetDateTime,
        _end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        unused_mock_method()
    }

    async fn update_registration_and_times(
        &self,
        _old_registration_id: &str,
        _new_registration_id: &str,
        _start_time: &OffsetDateTime,
        _end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        unused_mock_method()
    }
}

#[async_trait]
impl TimeEntryWriteStore for MockTimerHistoryRepository {
    async fn begin(
        &self,
        intent: TimeEntryWriteIntent,
    ) -> Result<BeginTimeEntryWrite, TimeTrackingError> {
        let write = match intent {
            TimeEntryWriteIntent::SaveActiveTimer {
                user_id,
                key,
                operation_id,
                stopped_at,
                note_override,
                ..
            } => {
                let Some(timer) = self.active_timer.lock().unwrap().clone() else {
                    return Ok(BeginTimeEntryWrite::NoActiveTimer);
                };
                PendingTimeEntryWrite {
                    user_id,
                    operation: TimeEntryWriteOperation::SaveActiveTimer,
                    key,
                    operation_id,
                    entry: PreparedTimeEntry {
                        origin: TimeEntryWriteOrigin::ActiveTimer(TimerHistoryId::new(1)),
                        project_id: timer.project_id.unwrap_or_else(|| ProjectId::new("")),
                        project_name: timer.project_name.unwrap_or_default(),
                        activity_id: timer.activity_id.unwrap_or_else(|| "".into()),
                        activity_name: timer.activity_name.unwrap_or_default(),
                        start_time: timer.started_at,
                        end_time: stopped_at,
                        note: note_override.unwrap_or(timer.note),
                    },
                }
            }
            TimeEntryWriteIntent::CreateTimeEntry {
                user_id,
                key,
                operation_id,
                entry,
                ..
            } => PendingTimeEntryWrite {
                user_id,
                operation: TimeEntryWriteOperation::CreateTimeEntry,
                key,
                operation_id,
                entry,
            },
        };

        Ok(match self.begins.lock().unwrap().pop_front() {
            Some(MockBegin::Pending(operation_id)) => {
                let mut pending = self.pending_write.lock().unwrap().clone().unwrap_or(write);
                pending.operation_id = operation_id;
                BeginTimeEntryWrite::Pending(pending)
            }
            Some(MockBegin::Replay(entry, registration_id)) => BeginTimeEntryWrite::Replay {
                entry,
                registration_id,
            },
            Some(MockBegin::PayloadMismatch) => BeginTimeEntryWrite::PayloadMismatch,
            Some(MockBegin::Fresh) | None => {
                *self.pending_write.lock().unwrap() = Some(write.clone());
                BeginTimeEntryWrite::Fresh(write)
            }
        })
    }

    async fn resolve(
        &self,
        write: &PendingTimeEntryWrite,
        resolution: TimeEntryWriteResolution<'_>,
    ) -> Result<(), TimeTrackingError> {
        match resolution {
            TimeEntryWriteResolution::Cancel => {
                self.cancellations.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            TimeEntryWriteResolution::Complete(registration_id) => {
                if self.fail_save_once.swap(false, Ordering::SeqCst) {
                    return Err(TimeTrackingError::unknown("simulated local failure"));
                }
                *self.saved_end_time.lock().unwrap() = Some(write.entry.end_time);
                if matches!(write.entry.origin, TimeEntryWriteOrigin::ActiveTimer(_)) {
                    *self.active_timer.lock().unwrap() = None;
                }
                self.completed
                    .lock()
                    .unwrap()
                    .push(write.entry.completed(registration_id));
                Ok(())
            }
        }
    }
}

fn unused_mock_method<T>() -> Result<T, TimeTrackingError> {
    panic!("test called an unexpected mock method")
}

fn sample_create_request(date: Date) -> CreateTimeEntryRequest {
    let start_time = date.with_hms(9, 0, 0).unwrap().assume_utc();
    CreateTimeEntryRequest {
        project_id: ProjectId::new("project-1"),
        activity_id: crate::domain::models::ActivityId::new("activity-1"),
        start_time,
        end_time: start_time + Duration::hours(1),
        note: "Work".to_string(),
    }
}

fn sample_time_entry(date: Date, status: TimeEntryStatus) -> TimeEntry {
    let request = sample_create_request(date);
    TimeEntry::new(
        "entry-1",
        request.project_id,
        "Project",
        request.activity_id,
        "Activity",
        date,
        1.0,
    )
    .with_note(request.note)
    .with_times(Some(request.start_time), Some(request.end_time))
    .with_week_number(date.iso_week())
    .with_status(status)
}

fn sample_prepared_entry(date: Date) -> PreparedTimeEntry {
    let request = sample_create_request(date);
    PreparedTimeEntry {
        origin: TimeEntryWriteOrigin::Direct,
        project_id: request.project_id,
        project_name: "Project".to_string(),
        activity_id: request.activity_id,
        activity_name: "Activity".to_string(),
        start_time: request.start_time,
        end_time: request.end_time,
        note: request.note,
    }
}

#[tokio::test]
async fn save_timer_uses_current_time_without_future_bonus() {
    let started_at = OffsetDateTime::now_utc() - Duration::minutes(20);
    let active_timer = ActiveTimer::new(started_at)
        .with_project("project-1", "Project")
        .with_activity("activity-1", "Activity")
        .with_note("note");
    let client = Arc::new(MockTimeTrackingClient::default());
    let repo = Arc::new(MockTimerHistoryRepository {
        active_timer: Mutex::new(Some(active_timer)),
        ..Default::default()
    });
    let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());
    let user_id = UserId::new(1);

    let before_save = OffsetDateTime::now_utc();
    let saved_entry = service
        .save_timer(&user_id, None, "save-test-1")
        .await
        .unwrap();
    let after_save = OffsetDateTime::now_utc();

    let provider_request = client.created_request.lock().unwrap().clone().unwrap();
    let history_end_time = repo.saved_end_time.lock().unwrap().unwrap();

    assert!(provider_request.end_time >= before_save);
    assert!(provider_request.end_time <= after_save);
    assert_eq!(history_end_time, provider_request.end_time);
    assert_eq!(saved_entry.end_time, Some(provider_request.end_time));
}

#[tokio::test]
async fn idempotent_create_replays_the_original_result_without_provider_calls() {
    let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
    let expected = sample_time_entry(date, TimeEntryStatus::Open);
    let client = Arc::new(MockTimeTrackingClient::default());
    let repo = Arc::new(MockTimerHistoryRepository {
        begins: Mutex::new(VecDeque::from([MockBegin::Replay(
            sample_prepared_entry(date),
            "entry-1".to_string(),
        )])),
        ..Default::default()
    });
    let service = TimeTrackingServiceImpl::new(client.clone(), repo);

    let actual = service
        .create_time_entry(
            &UserId::new(1),
            &sample_create_request(date),
            "create-replay",
        )
        .await
        .unwrap();

    assert_eq!(actual, expected);
    assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn idempotency_payload_mismatch_is_a_conflict() {
    let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
    let client = Arc::new(MockTimeTrackingClient::default());
    let repo = Arc::new(MockTimerHistoryRepository {
        begins: Mutex::new(VecDeque::from([MockBegin::PayloadMismatch])),
        ..Default::default()
    });
    let service = TimeTrackingServiceImpl::new(client.clone(), repo);

    let error = service
        .create_time_entry(&UserId::new(1), &sample_create_request(date), "reused-key")
        .await
        .unwrap_err();

    assert!(matches!(error, TimeTrackingError::IdempotencyConflict));
    assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn create_rejects_locked_periods_before_provider_writes() {
    let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
    let client = Arc::new(MockTimeTrackingClient {
        day_statuses: vec![TimeEntryDayStatus {
            date,
            status: TimeEntryStatus::Approved,
        }],
        ..Default::default()
    });
    let repo = Arc::new(MockTimerHistoryRepository::default());
    let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());

    let error = service
        .create_time_entry(
            &UserId::new(1),
            &sample_create_request(date),
            "locked-create",
        )
        .await
        .unwrap_err();

    assert!(matches!(error, TimeTrackingError::LockedPeriod));
    assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
    assert_eq!(repo.cancellations.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn save_recovers_provider_success_after_local_failure_without_duplicate_creation() {
    let started_at = OffsetDateTime::now_utc() - Duration::minutes(20);
    let active_timer = ActiveTimer::new(started_at)
        .with_project("project-1", "Project")
        .with_activity("activity-1", "Activity")
        .with_note("note");
    let operation_id = "toki-op-crash-window".to_string();
    let client = Arc::new(MockTimeTrackingClient {
        found_entry: Mutex::new(Some(TimerId::new("entry-1"))),
        ..Default::default()
    });
    let repo = Arc::new(MockTimerHistoryRepository {
        active_timer: Mutex::new(Some(active_timer)),
        begins: Mutex::new(VecDeque::from([
            MockBegin::Fresh,
            MockBegin::Pending(operation_id),
        ])),
        fail_save_once: AtomicBool::new(true),
        ..Default::default()
    });
    let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());
    let user_id = UserId::new(1);

    let first_error = service
        .save_timer(&user_id, None, "save-crash-window")
        .await
        .unwrap_err();
    assert!(matches!(first_error, TimeTrackingError::Unknown(_)));
    let submitted_end_time = client
        .created_request
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .end_time;

    let recovered = service
        .save_timer(&user_id, None, "save-crash-window")
        .await
        .unwrap();

    assert_eq!(recovered.registration_id, "entry-1");
    assert_eq!(recovered.end_time, Some(submitted_end_time));
    assert_eq!(client.created_count.load(Ordering::SeqCst), 1);
    assert_eq!(repo.completed.lock().unwrap().len(), 1);
    assert_eq!(repo.cancellations.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn resumed_write_never_submits_a_second_provider_create() {
    let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
    let client = Arc::new(MockTimeTrackingClient::default());
    let repo = Arc::new(MockTimerHistoryRepository {
        begins: Mutex::new(VecDeque::from([MockBegin::Pending(
            "toki-op-ambiguous".to_string(),
        )])),
        ..Default::default()
    });
    let service = TimeTrackingServiceImpl::new(client.clone(), repo);

    let error = service
        .create_time_entry(
            &UserId::new(1),
            &sample_create_request(date),
            "ambiguous-create",
        )
        .await
        .unwrap_err();

    assert!(matches!(error, TimeTrackingError::IdempotencyInProgress));
    assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_starts_leave_exactly_one_active_timer() {
    let client = Arc::new(MockTimeTrackingClient::default());
    let repo = Arc::new(MockTimerHistoryRepository::default());
    let service = TimeTrackingServiceImpl::new(client, repo.clone());
    let user_id = UserId::new(1);
    let request = StartTimerRequest {
        project_id: None,
        activity_id: None,
        note: String::new(),
    };

    let (first, second) = tokio::join!(
        service.start_timer(&user_id, &request),
        service.start_timer(&user_id, &request)
    );

    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(matches!(
        first.err().or_else(|| second.err()),
        Some(TimeTrackingError::TimerAlreadyRunning)
    ));
    assert!(repo.active_timer.lock().unwrap().is_some());
}

#[tokio::test]
async fn timer_selection_rejects_an_activity_without_a_project() {
    let service = TimeTrackingServiceImpl::new(
        Arc::new(MockTimeTrackingClient::default()),
        Arc::new(MockTimerHistoryRepository::default()),
    );
    let request = StartTimerRequest {
        project_id: None,
        activity_id: Some(crate::domain::models::ActivityId::new("activity-1")),
        note: String::new(),
    };

    let error = service
        .start_timer(&UserId::new(1), &request)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        TimeTrackingError::InvalidProjectActivity(_)
    ));
}

#[tokio::test]
async fn delete_rejects_locked_owned_entries() {
    let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
    let client = Arc::new(MockTimeTrackingClient {
        existing_entry: Mutex::new(Some(sample_time_entry(date, TimeEntryStatus::Approved))),
        ..Default::default()
    });
    let service =
        TimeTrackingServiceImpl::new(client, Arc::new(MockTimerHistoryRepository::default()));

    let error = service.delete_time_entry("entry-1").await.unwrap_err();

    assert!(matches!(error, TimeTrackingError::LockedPeriod));
}

#[test]
fn create_absences_validation_rejects_invalid_requests() {
    let date = Date::from_calendar_date(2026, time::Month::May, 20).unwrap();
    let valid_day = CreateAbsenceDay { date, hours: 8.0 };
    let valid = CreateAbsencesRequest {
        absence_type: AbsenceType::Vacation,
        child: None,
        comment: String::new(),
        days: vec![valid_day.clone()],
    };

    assert!(TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&valid).is_ok());

    let mut invalid = valid.clone();
    invalid.days = Vec::new();
    assert!(matches!(
            TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
            Err(TimeTrackingError::InvalidInput(_))
        ));

    let mut invalid = valid.clone();
    invalid.days = vec![valid_day.clone(), valid_day.clone()];
    assert!(matches!(
            TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
            Err(TimeTrackingError::InvalidInput(_))
        ));

    for hours in [0.0, -1.0, f64::NAN, 24.25] {
        let mut invalid = valid.clone();
        invalid.days = vec![CreateAbsenceDay { date, hours }];
        assert!(matches!(
                TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
                Err(TimeTrackingError::InvalidInput(_))
            ));
    }

    let invalid = CreateAbsencesRequest {
        absence_type: AbsenceType::ParentalLeave,
        child: Some(" ".to_string()),
        comment: String::new(),
        days: vec![valid_day.clone()],
    };
    assert!(matches!(
            TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
            Err(TimeTrackingError::InvalidInput(_))
        ));

    let valid = CreateAbsencesRequest {
        absence_type: AbsenceType::Furlough,
        child: None,
        comment: String::new(),
        days: vec![valid_day],
    };
    assert!(TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&valid).is_ok());
}

#[tokio::test]
async fn create_absences_rejects_unavailable_absence_type() {
    let date = Date::from_calendar_date(2026, time::Month::May, 20).unwrap();
    let client = Arc::new(MockTimeTrackingClient {
        created_request: Mutex::new(None),
        absence_types: vec![AbsenceType::Vacation],
        absence_children: Vec::new(),
        ..Default::default()
    });
    let repo = Arc::new(MockTimerHistoryRepository {
        active_timer: Mutex::new(None),
        ..Default::default()
    });
    let service = TimeTrackingServiceImpl::new(client, repo);

    let error = service
        .create_absences(&CreateAbsencesRequest {
            absence_type: AbsenceType::Sick,
            child: None,
            comment: String::new(),
            days: vec![CreateAbsenceDay { date, hours: 8.0 }],
        })
        .await
        .unwrap_err();

    assert!(
        matches!(error, TimeTrackingError::InvalidInput(message) if message == "This absence type is not available in Kleer")
    );
}

#[test]
fn registered_child_validation_requires_matching_child() {
    let date = Date::from_calendar_date(2026, time::Month::May, 20).unwrap();
    let request = CreateAbsencesRequest {
        absence_type: AbsenceType::Childcare,
        child: Some("Barn1".to_string()),
        comment: String::new(),
        days: vec![CreateAbsenceDay { date, hours: 8.0 }],
    };
    let children = vec![AbsenceChild {
        name: "Barn1".to_string(),
        birth_date: Some(date),
    }];

    let child = TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::trimmed_child_name(&request).unwrap();
    assert!(TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_registered_child(child, &children).is_ok());

    let error = TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_registered_child(child, &[])
            .unwrap_err();
    assert!(
        matches!(error, TimeTrackingError::InvalidInput(message) if message == "Register a child in Kleer before reporting this absence type")
    );

    let error = TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_registered_child(
            "Other",
            &children,
        )
        .unwrap_err();
    assert!(
        matches!(error, TimeTrackingError::InvalidInput(message) if message == "Select a registered child for this absence type")
    );
}

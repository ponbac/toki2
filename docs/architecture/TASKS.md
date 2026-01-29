# Refactoring Tasks

Detailed tasks for hexagonal architecture migration. Each task is designed to be small, focused, and independently completable.

**Legend:**
- `[ ]` = Not started
- `[~]` = In progress
- `[x]` = Completed
- `[!]` = Blocked (see notes)

---

## Phase 1: Foundation (Domain Models & Newtypes)

### 1.1 Audit Existing Domain Models

- [x] **T1.1.1** Read and document current domain models in `toki-api/src/domain/`
  - List all structs and their fields
  - Note which models have validation logic
  - Identify Milltime-specific types that should be generalized
  - Save findings to `LEARNINGS.md`

- [x] **T1.1.2** Identify all places where `MilltimeClient` is used directly
  - Search for `milltime::` imports across the codebase
  - Document each usage location and what it does
  - Save to `LEARNINGS.md`

### 1.2 Create Core Newtypes

- [x] **T1.2.1** Create `domain/models/mod.rs` with module structure
  - Add `pub mod ids;`
  - Add `pub mod timer;`
  - Add `pub mod project;`
  - Re-export key types

- [x] **T1.2.2** Create `UserId` newtype in `domain/models/ids.rs`
  - Wrap `i32` (database uses SERIAL, not UUID)
  - Implement: `Debug, Clone, Copy, PartialEq, Eq, Hash`
  - Add `new(i32)` constructor
  - Add `as_i32(&self) -> i32` getter
  - Implement `Display`
  - Implement `From<i32>`, `From<UserId> for i32`, and `AsRef<i32>`

- [x] **T1.2.3** Create `ProjectId` newtype in `domain/models/ids.rs`
  - Wrap `String` (Milltime uses string IDs like "300000000000241970")
  - Implement standard derives
  - Add `new(impl Into<String>)` constructor
  - Implement `Display`, `AsRef<str>`

- [x] **T1.2.4** Create `ActivityId` newtype in `domain/models/ids.rs`
  - Same pattern as `ProjectId`

- [x] **T1.2.5** Create `TimerId` newtype in `domain/models/ids.rs`
  - Same pattern as `ProjectId`

- [x] **T1.2.6** Verify `Email` newtype in existing code
  - ✅ Has proper validation via TryFrom<&str>
  - ✅ Implements Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash
  - ✅ Implements Deref, AsRef<str>, Display
  - ✅ Has good test coverage

### 1.3 Create Provider-Agnostic Timer Models

- [x] **T1.3.1** Create `Project` domain model in `domain/models/project.rs`
  - id: ProjectId, name: String, code: Option<String>
  - Builder pattern with with_code()

- [x] **T1.3.2** Create `Activity` domain model in `domain/models/project.rs`
  - id: ActivityId, name: String, project_id: ProjectId

- [x] **T1.3.3** Create `ActiveTimer` domain model in `domain/models/timer.rs`
  - id: TimerId, user_id: UserId, started_at: OffsetDateTime
  - project: Option<Project>, activity: Option<Activity>, note: String
  - Builder pattern methods

- [x] **T1.3.4** Create `TimerEntry` domain model in `domain/models/timer.rs`
  - Completed time entries with date, start/end time, hours

- [x] **T1.3.5** Create `StartTimerRequest` in `domain/models/timer.rs`
  - project_id, project_name, activity_id, activity_name, note

- [x] **T1.3.6** Create `SaveTimerRequest` in `domain/models/timer.rs`
  - project_id, project_name, activity_id, activity_name, note, date, duration

### 1.4 Create Domain Errors

- [x] **T1.4.1** Create `domain/error.rs` with `TimeTrackingError`
  - Created with variants: TimerNotFound, TimerAlreadyRunning, NoTimerRunning, AuthenticationFailed, ProjectNotFound, ActivityNotFound, InvalidDateRange, Unknown
  - Used String for Unknown instead of anyhow::Error (not available in crate)

- [x] **T1.4.2** Create `RepositoryError` in `domain/error.rs`
  - Already exists in `repositories/repo_error.rs` with DatabaseError and NotFound variants
  - Will leave in place for now, can move to domain later if needed

- [x] **T1.4.3** Run `cargo check` to verify all new types compile
  - ✅ All Phase 1 types compile (with expected dead_code warnings)

---

## Phase 2: Define Ports (Traits)

### 2.1 Create Port Module Structure

- [x] **T2.1.1** Create directory structure:
  ```
  domain/ports/
  ├── mod.rs
  ├── inbound/
  │   └── mod.rs
  └── outbound/
      └── mod.rs
  ```

- [x] **T2.1.2** Update `domain/mod.rs` to include `pub mod ports;`

### 2.2 Outbound Ports

- [x] **T2.2.1** Create `TimeTrackingClient` trait in `domain/ports/outbound/time_tracking.rs`
  - Methods: get_active_timer, start_timer, stop_timer, save_timer, get_projects, get_activities
  - Authentication handled separately (credentials managed by adapter)

- [ ] **T2.2.2** Create `UserRepository` trait in `domain/ports/outbound/user_repository.rs`
  - Base on existing `UserRepositoryImpl` methods
  - Use domain types (`UserId`, `Email`) in signatures
  - Return `Result<_, RepositoryError>`
  - **SKIP for now** - focus on TimeTracking first

- [ ] **T2.2.3** Create `TimerRepository` trait in `domain/ports/outbound/timer_repository.rs`
  - Base on existing `TimerRepositoryImpl`
  - Use domain types
  - **SKIP for now** - focus on TimeTracking first

- [ ] **T2.2.4** Create `NotificationRepository` trait in `domain/ports/outbound/notification_repository.rs`
  - Base on existing impl
  - **SKIP for now** - focus on TimeTracking first

- [x] **T2.2.5** Update `domain/ports/outbound/mod.rs` to export all traits

- [x] **T2.2.6** Run `cargo check` - compiles with expected dead_code warnings

### 2.3 Inbound Ports

- [x] **T2.3.1** Create `TimeTrackingService` trait in `domain/ports/inbound/time_tracking.rs`
  - Methods: get_active_timer, start_timer, stop_and_save_timer, get_projects, get_activities

- [x] **T2.3.2** Update `domain/ports/inbound/mod.rs` to export trait

- [x] **T2.3.3** Run `cargo check` - compiles with expected dead_code warnings

---

## Phase 3: Implement Adapters

### 3.1 Create Adapter Directory Structure

- [x] **T3.1.1** Create directory structure:
  ```
  adapters/
  ├── mod.rs
  ├── inbound/
  │   ├── mod.rs
  │   └── http/
  │       └── mod.rs
  └── outbound/
      ├── mod.rs
      ├── postgres/
      │   └── mod.rs
      └── milltime/
          └── mod.rs
  ```

- [x] **T3.1.2** Add `mod adapters;` to `main.rs`

### 3.2 Milltime Adapter

- [x] **T3.2.1** Create `MilltimeAdapter` struct in `adapters/outbound/milltime/mod.rs`
  - Holds MilltimeClient and UserId
  - Constructor takes Credentials and UserId

- [x] **T3.2.2** Implement `TimeTrackingClient` for `MilltimeAdapter` - authentication
  - Authentication handled by passing Credentials to constructor
  - Adapter creates MilltimeClient internally

- [x] **T3.2.3** Implement `TimeTrackingClient` for `MilltimeAdapter` - get_active_timer
  - Calls fetch_timer(), handles "no timer" case as None

- [x] **T3.2.4** Implement `TimeTrackingClient` for `MilltimeAdapter` - start_timer
  - Creates StartTimerOptions, starts timer, fetches result

- [x] **T3.2.5** Implement `TimeTrackingClient` for `MilltimeAdapter` - stop_timer
  - Delegates to milltime client

- [x] **T3.2.6** Implement `TimeTrackingClient` for `MilltimeAdapter` - save_timer
  - Uses SaveTimerPayload, returns registration ID

- [x] **T3.2.7** Implement `TimeTrackingClient` for `MilltimeAdapter` - get_projects
  - Uses ProjectSearchFilter("Overview")

- [x] **T3.2.8** Implement `TimeTrackingClient` for `MilltimeAdapter` - get_activities
  - Uses ActivityFilter with date range

- [x] **T3.2.9** Add conversion functions for Milltime types <-> Domain types
  - Created in `conversions.rs`: to_domain_active_timer, to_domain_project, to_domain_activity

- [x] **T3.2.10** Run `cargo check` - compiles with expected dead_code warnings

### 3.3 PostgreSQL Adapters

- [ ] **T3.3.1** Create `PostgresUserRepository` in `adapters/outbound/postgres/user_repo.rs`
  - Move/wrap existing `UserRepositoryImpl`
  - Implement `UserRepository` trait

- [ ] **T3.3.2** Create `PostgresTimerRepository` in `adapters/outbound/postgres/timer_repo.rs`
  - Move/wrap existing `TimerRepositoryImpl`
  - Implement `TimerRepository` trait

- [ ] **T3.3.3** Create `PostgresNotificationRepository`
  - Move/wrap existing implementation
  - Implement trait

- [ ] **T3.3.4** Update `adapters/outbound/postgres/mod.rs` exports

- [ ] **T3.3.5** Run `cargo check` and `cargo sqlx prepare` if needed

---

## Phase 4: Service Layer

### 4.1 Implement Services

- [x] **T4.1.1** Create `domain/services/mod.rs`

- [x] **T4.1.2** Create `TimeTrackingServiceImpl` in `domain/services/time_tracking.rs`
  - Generic over `TimeTrackingClient`
  - Hold reference to client

- [x] **T4.1.3** Implement `TimeTrackingService` trait for `TimeTrackingServiceImpl`
  - Implement `get_active_timer` - delegate to client
  - Implement `start_timer` - check if timer running, then start
  - Implement `stop_and_save_timer` - stop then save
  - Implement `get_projects` - delegate to client
  - Implement `get_activities` - delegate to client

- [x] **T4.1.4** Add business logic validation in service methods
  - E.g., prevent starting timer when one is running
  - E.g., validate project/activity exist before saving

- [x] **T4.1.5** Run `cargo check`

---

## Phase 5: Wire It Together

### 5.1 Update AppState

- [x] **T5.1.1** Read current `app_state.rs` and document dependencies
  - Save to `LEARNINGS.md`
  - **Finding:** Milltime client is created per-request from cookies, not stored in AppState
  - **Decision:** TimeTrackingService will also be per-request, no AppState changes needed

- [x] **T5.1.2** ~~Create new `AppState` with trait objects~~ SKIPPED
  - Not needed - TimeTrackingService is per-request (credentials from cookies)

- [x] **T5.1.3** ~~Update `main.rs` to construct adapters and services~~ SKIPPED
  - Not needed - service created per-request in route handlers

- [x] **T5.1.4** Run `cargo check` - already passing

### 5.2 Update HTTP Routes

- [x] **T5.2.1** Create `adapters/inbound/http/time_tracking.rs`
  - Added `TimeTrackingServiceExt` trait for creating service from cookies
  - Added `TimeTrackingServiceError` for HTTP-level errors

**Note:** The following tasks involve changing API response types, which would break the frontend.
These are deferred to a future phase. The architecture is in place for incremental migration.

- [ ] **T5.2.2** ~~Update timer routes to use service~~ DEFERRED
  - Would require changing response types (Milltime types have more fields than domain types)
  - Can be done incrementally when API changes are acceptable

- [ ] **T5.2.3** ~~Update project routes to use service~~ DEFERRED
  - Same as above

- [ ] **T5.2.4** ~~Create HTTP response types~~ DEFERRED
  - Requires defining minimal API contract
  - Should be done when migrating routes

- [ ] **T5.2.5** ~~Create HTTP request types~~ DEFERRED
  - Domain request types exist, HTTP parsing can be added when routes migrate

- [ ] **T5.2.6** ~~Update router~~ DEFERRED
  - No changes needed until routes are migrated

- [x] **T5.2.7** Run `cargo check` - passing with dead_code warnings (expected)

- [ ] **T5.2.8** ~~Test manually~~ DEFERRED until routes are migrated

### 5.3 Cleanup

All cleanup tasks are DEFERRED until routes are migrated.

- [ ] **T5.3.1** ~~Remove old route implementations~~ DEFERRED
- [ ] **T5.3.2** ~~Remove direct `milltime::` imports~~ DEFERRED
- [ ] **T5.3.3** ~~Update `MilltimeCookieJarExt`~~ DEFERRED
  - `TimeTrackingServiceExt` is the new replacement, can coexist for now
- [ ] **T5.3.4** ~~Run full tests~~ DEFERRED

---

## Phase 6: Verify & Document

### 6.1 Verification

- [x] **T6.1.1** Verify the app compiles
  - `cargo check` passes with expected dead_code warnings
  - Routes still work unchanged (architecture is additive)

- [ ] **T6.1.2** ~~Verify no Milltime types leak to HTTP layer~~ NOT YET
  - Routes still use Milltime types directly (API backward compatibility)
  - Can be addressed when routes are migrated

- [ ] **T6.1.3** Write at least one test with a mock `TimeTrackingClient`
  - Optional but recommended

### 6.2 Documentation

- [x] **T6.2.1** Update `CLAUDE.md` with new architecture notes

- [x] **T6.2.2** Document how to add a new time tracking provider
  - Added to CLAUDE.md under "Time Tracking Architecture"

- [x] **T6.2.3** `LEARNINGS.md` updated throughout the process

---

## Future Tasks (Post-Migration)

These are not part of the core refactoring but should be considered:

- [ ] **F1** Generalize `timer_type` column to `provider` in database
- [ ] **F2** Add feature flag for provider selection
- [ ] **F3** Consider refactoring Azure DevOps integration similarly
- [ ] **F4** Add integration tests with test database
- [ ] **F5** Consider adding OpenTelemetry tracing at adapter boundaries

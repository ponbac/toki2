# Learnings

This document captures important discoveries, decisions, and gotchas encountered during the refactoring. Update this as you work.

---

## Codebase Discoveries

### Current Architecture (Pre-Refactoring)

#### Domain Models (Audit completed 2026-01-29)

**Location:** `toki-api/src/domain/`

| File | Types | Notes |
|------|-------|-------|
| `email.rs` | `Email`, `EmailError` | ✅ Well-designed newtype with TryFrom validation |
| `milltime_password.rs` | `MilltimePassword` | ⚠️ Milltime-specific, uses AES encryption via env key |
| `user.rs` | `User`, `Role` | Uses `i32` for id, implements `axum_login::AuthUser` |
| `repo_key.rs` | `RepoKey` | ✅ Good composite key pattern (org/project/repo) |
| `repository.rs` | `Repository` | Simple DB model with i32 id |
| `pull_request.rs` | `PullRequest`, `PullRequestDiff` | Tightly coupled to `az_devops::*` types |
| `pr_change_event.rs` | `PRChangeEvent` | Depends on `az_devops::Thread`, `az_devops::Comment` |
| `notification_preference.rs` | `DbNotificationType`, `NotificationRule`, `PrNotificationException`, `Notification` | DB models for notifications |
| `notification_handler.rs` | `NotificationHandler` | ⚠️ Business logic but uses concrete repos |
| `push_notification.rs` | `PushNotification` | Simple DTO for web push |
| `push_subscription.rs` | `PushSubscription`, `PushSubscriptionInfo` | DB model + DTO |
| `repo_config.rs` | `RepoConfig` | Config model, creates `az_devops::RepoClient` |
| `repo_differ.rs` | `RepoDiffer`, `RepoDifferStatus`, `RepoDifferMessage`, `CachedIdentities` | ⚠️ Complex worker with business logic |

**Key Observations:**

1. **No timer models in domain** - All timer types come directly from `milltime` crate
2. **User.id is i32, not Uuid** - No UserId newtype exists
3. **No generic credentials type** - MilltimePassword is provider-specific
4. **Heavy az_devops coupling** - PullRequest, PRChangeEvent embed az_devops types directly

**Milltime Usage Locations:** (T1.1.2 completed 2026-01-29)

| File | Usage |
|------|-------|
| `routes/milltime/mod.rs` | `MilltimeCookieJarExt` trait, `MilltimeClient::new()`, `Credentials::new()` |
| `routes/milltime/authenticate.rs` | `Credentials::new()` for login |
| `routes/milltime/timer.rs` | `TimerRegistration`, `StartTimerOptions`, `SaveTimerPayload`, `EditTimerPayload`, `MilltimeFetchError` |
| `routes/milltime/calendar.rs` | `TimeInfo`, `DateFilter`, `TimeEntry`, `ProjectRegistrationPayload`, `ProjectRegistrationEditPayload` |
| `routes/milltime/projects.rs` | `ProjectSearchItem`, `Activity`, `ProjectSearchFilter`, `ActivityFilter` |
| `router.rs` | Just nests the milltime routes |
| `domain/milltime_password.rs` | Encryption for Milltime password storage |

**Key Milltime Types to Abstract:**

| Milltime Type | Purpose | Domain Equivalent Needed |
|---------------|---------|-------------------------|
| `MilltimeClient` | API client | `TimeTrackingClient` trait |
| `Credentials` | Authentication | `TimeTrackingCredentials` or handled in adapter |
| `TimerRegistration` | Active timer | `ActiveTimer` |
| `StartTimerOptions` | Start timer request | `StartTimerRequest` |
| `SaveTimerPayload` | Save timer request | `SaveTimerRequest` |
| `ProjectSearchItem` | Project info | `Project` |
| `Activity` | Activity info | `Activity` |
| `TimeInfo`/`TimeEntry` | Completed time entries | `TimerEntry` |
| `DateFilter` | Date range query | Generic `DateRange` or keep in adapter |

**Existing Good Patterns:**
- `Email` newtype with proper validation via `TryFrom<&str>`
- `EmailError` with descriptive variants
- `RepoKey` composite key with Display impl
- Tests in `email.rs` and `milltime_password.rs`

**Pain Points:**
- `MilltimePassword` is Milltime-specific and reads env var directly (`MT_CRYPTO_KEY`)
- `NotificationHandler` uses concrete repository implementations
- `RepoDiffer` has infrastructure concerns mixed with business logic
- No abstraction layer for time tracking - routes call milltime crate directly

---

## Key Decisions

### Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-01-29 | `UserId` wraps `i32`, not `Uuid` | Database uses SERIAL (i32) for users.id, not UUID |
| 2026-01-29 | `ProjectId`, `ActivityId`, `TimerId` wrap `String` | Milltime uses string IDs like "300000000000241970" |
| 2026-01-29 | Keep `RepoKey` as-is | Already follows newtype pattern well, no changes needed |

---

## Type Mappings

### Milltime Types → Domain Types

| Milltime Type | Domain Type | Notes |
|---------------|-------------|-------|
| `milltime::Timer` | `ActiveTimer` | |
| `milltime::Project` | `Project` | |
| `milltime::Activity` | `Activity` | |

*Fill this in as you implement adapters.*

---

## Gotchas & Warnings

### Things That Can Go Wrong

1. **SQLx Offline Mode**
   - After changing SQL queries, run `cargo sqlx prepare`
   - The `.sqlx/` directory must be committed

2. **Milltime Authentication**
   - Uses encrypted cookies stored client-side
   - `MT_CRYPTO_KEY` env var required for encryption
   - See `MilltimeCookieJarExt` trait for current implementation

3. **Cookie Handling**
   - Milltime adapter needs access to request cookies
   - Consider how to pass credentials to adapter

4. *Add more as you discover them*

---

## API Contract Notes

### Milltime API (Reverse-Engineered)

*Document any Milltime API quirks here as you work with the adapter.*

- Endpoint: ...
- Quirk: ...

---

## Testing Notes

### How to Test Changes

```bash
# Backend compile check
cargo check

# Run the server
cd toki-api && bacon run

# Frontend type check (if API changes)
cd app && bun tsc && bun lint
```

### Manual Testing Checklist

- [ ] Can authenticate with Milltime
- [ ] Can start a timer
- [ ] Can stop a timer
- [ ] Can save a timer entry
- [ ] Can get projects list
- [ ] Can get activities for a project

---

## Files Modified

*Track which files you've changed for easier review.*

### Phase 1 (completed 2026-01-29)
- [x] `domain/mod.rs` - added models and error modules
- [x] `domain/models/mod.rs` - created
- [x] `domain/models/ids.rs` - created (UserId, ProjectId, ActivityId, TimerId)
- [x] `domain/models/project.rs` - created (Project, Activity)
- [x] `domain/models/timer.rs` - created (ActiveTimer, TimerEntry, StartTimerRequest, SaveTimerRequest)
- [x] `domain/error.rs` - created (TimeTrackingError)

### Phase 2 (completed 2026-01-29)
- [x] `domain/mod.rs` - added ports module
- [x] `domain/ports/mod.rs` - created
- [x] `domain/ports/inbound/mod.rs` - created
- [x] `domain/ports/inbound/time_tracking.rs` - created (TimeTrackingService trait)
- [x] `domain/ports/outbound/mod.rs` - created
- [x] `domain/ports/outbound/time_tracking.rs` - created (TimeTrackingClient trait)

### Phase 3 (complete)
- [x] `adapters/mod.rs` - created
- [x] `adapters/inbound/mod.rs` - created
- [x] `adapters/inbound/http/mod.rs` - exports TimeTrackingServiceExt
- [x] `adapters/outbound/mod.rs` - created
- [x] `adapters/outbound/postgres/mod.rs` - placeholder
- [x] `adapters/outbound/milltime/mod.rs` - MilltimeAdapter implementing TimeTrackingClient
- [x] `adapters/outbound/milltime/conversions.rs` - type conversion functions
- [x] `main.rs` - added adapters module

### Phase 4 (complete)
- [x] `domain/services/mod.rs` - created
- [x] `domain/services/time_tracking.rs` - TimeTrackingServiceImpl

### Phase 5 (partial)
- [x] `adapters/inbound/http/time_tracking.rs` - TimeTrackingServiceExt trait
- [x] `domain/mod.rs` - added services module

### Phase 6 (documentation)
- [x] `CLAUDE.md` - added time tracking architecture section

---

## Open Questions

*Questions that need answers or decisions.*

1. **How should authentication flow work with the adapter?** ✅ RESOLVED
   - **Answer:** Create adapter per-request from cookies (same as current pattern)
   - Current flow: `CookieJar` → `Credentials` → `MilltimeClient`
   - New flow: `CookieJar` → `Credentials` → `MilltimeAdapter` → `TimeTrackingServiceImpl`
   - The `MilltimeCookieJarExt` trait will be updated to return a service instead of a raw client

2. **Should we keep backward compatibility during migration?**
   - Option A: Keep old routes, add new ones, deprecate later
   - Option B: Clean swap
   - **Decision:** Clean swap - the API contract doesn't change, only internal implementation

3. *Add more questions as they arise*

---

## AppState Analysis (T5.1.1)

### Current Dependencies

| Field | Type | Purpose |
|-------|------|---------|
| `app_url` | `Url` | Frontend URL for redirects |
| `api_url` | `Url` | API URL for cookie domain |
| `cookie_domain` | `String` | Domain for setting cookies |
| `db_pool` | `Arc<PgPool>` | Database connection pool |
| `user_repo` | `Arc<UserRepositoryImpl>` | User database operations |
| `repository_repo` | `Arc<RepoRepositoryImpl>` | Azure DevOps repo DB operations |
| `push_subscriptions_repo` | `Arc<PushSubscriptionRepositoryImpl>` | Push subscription storage |
| `milltime_repo` | `Arc<TimerRepositoryImpl>` | Timer history in database |
| `notification_repo` | `Arc<NotificationRepositoryImpl>` | Notification settings |
| `repo_clients` | `Arc<RwLock<HashMap<RepoKey, RepoClient>>>` | Azure DevOps API clients |
| `differs` | `Arc<RwLock<HashMap<RepoKey, Arc<RepoDiffer>>>>` | PR monitoring workers |
| `differ_txs` | `Arc<Mutex<HashMap<...>>>` | Channels to differ workers |
| `web_push_client` | `IsahcWebPushClient` | Web push notification sender |
| `notification_handler` | `Arc<NotificationHandler>` | Processes notifications |

### Key Insight: Milltime Not in AppState

The `MilltimeClient` is NOT stored in `AppState` because:
- It requires per-user credentials
- Credentials come from cookies (encrypted username/password)
- Each request creates its own client via `MilltimeCookieJarExt::into_milltime_client`

### Integration Strategy

The `TimeTrackingService` cannot be stored in `AppState` either. Instead:
1. Keep per-request service creation pattern
2. Update `MilltimeCookieJarExt` to create `TimeTrackingServiceImpl<MilltimeAdapter>`
3. Route handlers get a service instance from cookies, use it, done

This matches the existing architecture - we're just adding an abstraction layer.

### API Response Types Decision

**Issue:** Domain types (`Project`, `Activity`) have fewer fields than Milltime types.
- `milltime::ProjectSearchItem` has: is_favorite, is_member, leader_name, customer_names, etc.
- `domain::Project` has: id, name, code

**Decision:** Keep existing API responses for now (return Milltime types directly).
- The service layer provides abstraction for *internal* use
- Response types can be migrated to domain-based in a future phase
- This maintains backward compatibility with the frontend

**Future consideration:** When adding a new provider, we'll need to:
1. Define the minimal API contract needed by the frontend
2. Create generic response types
3. Have each adapter fill in what it can

---

## Performance Considerations

*Note any performance-sensitive areas.*

- ...

---

## Dependencies Added/Removed

| Action | Crate | Reason |
|--------|-------|--------|
| | | |

*Example:*
| Added | `async-trait` | Needed for async trait definitions |

---

## Rollback Notes

If something goes wrong, here's how to recover:

1. All changes should be in separate commits
2. The old code paths remain until explicitly removed
3. Database migrations should be backward compatible

---

## Session Notes

*Use this section for free-form notes during work sessions.*

### Session: 2026-01-29 (Part 1)

**Progress:**
- Completed Phase 1: Created all domain models (UserId, ProjectId, ActivityId, TimerId, Project, Activity, ActiveTimer, TimerEntry, StartTimerRequest, SaveTimerRequest) and TimeTrackingError
- Completed Phase 2: Created TimeTrackingClient (outbound port) and TimeTrackingService (inbound port) traits
- Completed Phase 3 Milltime Adapter: Implemented MilltimeAdapter that wraps the milltime crate and implements TimeTrackingClient

**Key Design Decisions:**
- UserId wraps i32 (matching database SERIAL), not Uuid
- ProjectId, ActivityId, TimerId wrap String (Milltime uses string IDs)
- Adapter takes Credentials in constructor, creates MilltimeClient internally
- "No timer running" is handled by returning None from get_active_timer (not an error)

---

### Session: 2026-01-29 (Part 2)

**Progress:**
- Completed Phase 4: Created TimeTrackingServiceImpl with business logic
  - Checks for existing timer before starting
  - Checks for running timer before stopping/saving
- Completed Phase 5.1: Analyzed AppState, discovered per-request service pattern
- Completed Phase 5.2 (partial): Created TimeTrackingServiceExt HTTP helper trait
- Deferred route migration: API response types would need frontend changes
- Completed Phase 6.2: Updated CLAUDE.md with architecture docs

**What's Done:**
1. Domain models for time tracking (provider-agnostic)
2. Port traits (TimeTrackingClient, TimeTrackingService)
3. MilltimeAdapter implementing TimeTrackingClient
4. TimeTrackingServiceImpl with business logic
5. TimeTrackingServiceExt for creating service from cookies
6. Documentation in CLAUDE.md

**What's Deferred:**
1. Migrating routes to use domain types (would change API contract)
2. Creating HTTP response types (depends on route migration)
3. Cleanup of old code (depends on route migration)

**Architecture Ready For:**
- Adding a new time tracking provider (implement TimeTrackingClient)
- Unit testing business logic (mock TimeTrackingClient)
- Future API migration (infrastructure is in place)

**Key Learnings:**
1. Milltime client is created per-request from cookies (not stored in AppState)
2. Domain types have fewer fields than Milltime API types
3. Full API migration requires frontend coordination
4. "Standalone" timers exist (local-only, no Milltime sync)

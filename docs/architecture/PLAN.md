# Hexagonal Architecture Refactoring Plan

## Goal

Restructure `toki-api` to follow hexagonal architecture principles, making it easy to:
1. Swap out Milltime for a new time tracking provider
2. Test business logic without external dependencies
3. Keep the codebase simple and maintainable for a small team

## Guiding Principles

- **Pragmatic over Dogmatic**: This is a small app with few developers. Don't over-engineer.
- **Incremental Migration**: Each phase should result in working, shippable code.
- **Domain First**: Business logic should never import external crates directly.
- **Newtypes for Validation**: Use the newtype pattern to make invalid states unrepresentable.
- **Ports Define Contracts**: Traits in the domain define what adapters must provide.
- **Adapters Translate**: Adapters convert between domain types and external formats.

---

## Current State

```
toki-api/src/
├── main.rs
├── app_state.rs          # God object with all dependencies
├── auth/                 # Azure AD OAuth
├── domain/               # Mixed: models + some business logic
├── repositories/         # SQLx implementations (no traits)
├── routes/               # HTTP handlers calling repos directly
│   └── milltime/         # Milltime-specific routes
└── utils/
```

**Problems:**
1. Routes directly call `MilltimeClient` - tight coupling
2. Repositories are concrete implementations, not traits
3. Domain models leak infrastructure concerns (`MilltimePassword`)
4. No clear separation between domain logic and HTTP handling
5. `AppState` is a god object mixing concerns

---

## Target State

```
toki-api/src/
├── main.rs                    # Wiring only
├── domain/
│   ├── models/                # Pure domain types (User, Timer, Project, etc.)
│   │   ├── user.rs
│   │   ├── timer.rs
│   │   ├── project.rs
│   │   └── ...
│   ├── ports/
│   │   ├── inbound/           # Service traits (what handlers can do)
│   │   │   ├── time_tracking.rs
│   │   │   └── notifications.rs
│   │   └── outbound/          # Repository/client traits (what domain needs)
│   │       ├── time_tracking_client.rs
│   │       ├── user_repository.rs
│   │       └── ...
│   ├── services/              # Business logic implementations
│   │   ├── time_tracking_service.rs
│   │   └── notification_service.rs
│   └── error.rs               # Domain errors
├── adapters/
│   ├── inbound/
│   │   └── http/              # Axum routes
│   │       ├── mod.rs
│   │       ├── time_tracking.rs
│   │       ├── pull_requests.rs
│   │       └── ...
│   └── outbound/
│       ├── postgres/          # SQLx implementations
│       │   ├── user_repo.rs
│       │   └── ...
│       ├── milltime/          # Milltime client adapter
│       │   └── client.rs
│       └── azure_devops/      # ADO client adapter
│           └── client.rs
├── app_state.rs               # Simplified: just holds Arc<dyn Trait>s
└── config.rs
```

---

## Phase 1: Foundation (Domain Models & Newtypes)

**Goal**: Establish clean domain models with proper validation using newtypes.

### 1.1 Create Domain Newtypes

Create validated wrapper types for core identifiers:

```rust
// domain/models/ids.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn as_uuid(&self) -> &Uuid { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn new(id: impl Into<String>) -> Self { Self(id.into()) }
}
```

Types to create:
- `UserId`, `ProjectId`, `ActivityId`, `TimerId`
- `Email` (already exists, verify validation)
- `RepoKey` (already exists, good pattern to follow)

### 1.2 Create Provider-Agnostic Timer Model

```rust
// domain/models/timer.rs
pub struct ActiveTimer {
    pub id: TimerId,
    pub user_id: UserId,
    pub started_at: OffsetDateTime,
    pub project: Option<Project>,
    pub activity: Option<Activity>,
    pub note: String,
}

pub struct TimerEntry {
    pub user_id: UserId,
    pub date: Date,
    pub start_time: Time,
    pub end_time: Time,
    pub project: Project,
    pub activity: Activity,
    pub note: String,
    pub hours: f64,
}
```

### 1.3 Create Domain Errors

```rust
// domain/error.rs
#[derive(Debug, thiserror::Error)]
pub enum TimeTrackingError {
    #[error("timer not found")]
    TimerNotFound,
    #[error("timer already running")]
    TimerAlreadyRunning,
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("project not found: {0}")]
    ProjectNotFound(ProjectId),
    #[error(transparent)]
    Unknown(#[from] anyhow::Error),
}
```

---

## Phase 2: Define Ports (Traits)

**Goal**: Define the contracts that adapters must implement.

### 2.1 Outbound Port: TimeTrackingClient

```rust
// domain/ports/outbound/time_tracking_client.rs
#[async_trait]
pub trait TimeTrackingClient: Send + Sync + 'static {
    async fn get_active_timer(&self, user_id: &UserId) -> Result<Option<ActiveTimer>, TimeTrackingError>;
    async fn start_timer(&self, req: StartTimerRequest) -> Result<ActiveTimer, TimeTrackingError>;
    async fn stop_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError>;
    async fn save_timer(&self, req: SaveTimerRequest) -> Result<TimerId, TimeTrackingError>;
    async fn get_projects(&self, user_id: &UserId) -> Result<Vec<Project>, TimeTrackingError>;
    async fn get_activities(&self, project_id: &ProjectId) -> Result<Vec<Activity>, TimeTrackingError>;
    async fn get_time_entries(&self, user_id: &UserId, range: DateRange) -> Result<Vec<TimerEntry>, TimeTrackingError>;
}
```

### 2.2 Outbound Port: UserRepository

```rust
// domain/ports/outbound/user_repository.rs
#[async_trait]
pub trait UserRepository: Send + Sync + 'static {
    async fn get_by_id(&self, id: &UserId) -> Result<Option<User>, RepositoryError>;
    async fn get_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError>;
    async fn create(&self, user: &User) -> Result<UserId, RepositoryError>;
    async fn update(&self, user: &User) -> Result<(), RepositoryError>;
    // ... other methods
}
```

### 2.3 Inbound Port: TimeTrackingService

```rust
// domain/ports/inbound/time_tracking.rs
#[async_trait]
pub trait TimeTrackingService: Send + Sync + 'static {
    async fn get_active_timer(&self, user_id: &UserId) -> Result<Option<ActiveTimer>, TimeTrackingError>;
    async fn start_timer(&self, user_id: &UserId, req: StartTimerRequest) -> Result<ActiveTimer, TimeTrackingError>;
    async fn stop_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError>;
    async fn save_timer(&self, user_id: &UserId, req: SaveTimerRequest) -> Result<TimerId, TimeTrackingError>;
    // ...
}
```

---

## Phase 3: Implement Adapters

**Goal**: Wrap existing implementations behind the port traits.

### 3.1 Milltime Adapter

Move `milltime` crate interaction into an adapter:

```rust
// adapters/outbound/milltime/client.rs
pub struct MilltimeAdapter {
    credentials: MilltimeCredentials,
    http_client: reqwest::Client,
}

impl TimeTrackingClient for MilltimeAdapter {
    async fn get_active_timer(&self, user_id: &UserId) -> Result<Option<ActiveTimer>, TimeTrackingError> {
        let milltime_client = self.create_client()?;
        let mt_timer = milltime_client.get_timer().await
            .map_err(|e| TimeTrackingError::Unknown(e.into()))?;

        Ok(mt_timer.map(|t| t.into_domain()))
    }
    // ... implement other methods
}
```

### 3.2 PostgreSQL Adapter

Wrap existing repository implementations:

```rust
// adapters/outbound/postgres/user_repo.rs
pub struct PostgresUserRepository {
    pool: PgPool,
}

impl UserRepository for PostgresUserRepository {
    async fn get_by_id(&self, id: &UserId) -> Result<Option<User>, RepositoryError> {
        sqlx::query_as!(...)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Unknown(e.into()))
    }
}
```

### 3.3 HTTP Adapter (Routes)

Simplify routes to just translate HTTP <-> domain:

```rust
// adapters/inbound/http/time_tracking.rs
pub async fn get_timer<S: TimeTrackingService>(
    State(state): State<AppState<S>>,
    user: AuthenticatedUser,
) -> Result<Json<TimerResponse>, ApiError> {
    let timer = state.time_tracking
        .get_active_timer(&user.id)
        .await?;

    Ok(Json(timer.map(TimerResponse::from)))
}
```

---

## Phase 4: Service Layer

**Goal**: Implement business logic that orchestrates ports.

### 4.1 TimeTrackingServiceImpl

```rust
// domain/services/time_tracking_service.rs
pub struct TimeTrackingServiceImpl<C: TimeTrackingClient> {
    client: C,
}

impl<C: TimeTrackingClient> TimeTrackingService for TimeTrackingServiceImpl<C> {
    async fn start_timer(&self, user_id: &UserId, req: StartTimerRequest) -> Result<ActiveTimer, TimeTrackingError> {
        // Check if timer already running
        if let Some(_) = self.client.get_active_timer(user_id).await? {
            return Err(TimeTrackingError::TimerAlreadyRunning);
        }

        self.client.start_timer(req).await
    }
}
```

---

## Phase 5: Wire It All Together

**Goal**: Update `main.rs` and `AppState` to use the new architecture.

### 5.1 Simplified AppState

```rust
// app_state.rs
pub struct AppState {
    pub time_tracking: Arc<dyn TimeTrackingService>,
    pub user_repo: Arc<dyn UserRepository>,
    pub notification_service: Arc<dyn NotificationService>,
    // ... other services
}
```

### 5.2 Dependency Injection in main.rs

```rust
// main.rs
async fn main() -> Result<()> {
    let config = Config::load()?;
    let pool = PgPool::connect(&config.database_url).await?;

    // Create adapters
    let user_repo = Arc::new(PostgresUserRepository::new(pool.clone()));
    let milltime_client = Arc::new(MilltimeAdapter::new(config.milltime));

    // Create services
    let time_tracking = Arc::new(TimeTrackingServiceImpl::new(milltime_client));

    // Create app state
    let state = AppState {
        time_tracking,
        user_repo,
        // ...
    };

    // Build router and run
    let router = build_router(state);
    axum::serve(listener, router).await?;
}
```

---

## Phase 6: Migration Preparation

**Goal**: Prepare for swapping Milltime with a new provider.

### 6.1 Generalize Database Schema

```sql
-- Add provider column
ALTER TABLE timer_history
ADD COLUMN provider VARCHAR(50) DEFAULT 'milltime';

-- Make timer_type more generic
ALTER TABLE timer_history
RENAME COLUMN timer_type TO source_type;
```

### 6.2 Feature Flags (Optional)

```rust
pub enum TimeTrackingProvider {
    Milltime,
    NewTool,
}

impl AppState {
    pub fn time_tracking_for(&self, provider: TimeTrackingProvider) -> Arc<dyn TimeTrackingClient> {
        match provider {
            TimeTrackingProvider::Milltime => self.milltime_client.clone(),
            TimeTrackingProvider::NewTool => self.new_tool_client.clone(),
        }
    }
}
```

---

## Success Criteria

After completing all phases:

1. **Swappability**: Replacing Milltime requires only:
   - Implementing `TimeTrackingClient` for the new provider
   - Changing one line in `main.rs`

2. **Testability**: Can test services with mock implementations:
   ```rust
   #[tokio::test]
   async fn test_start_timer_when_already_running() {
       let mock_client = MockTimeTrackingClient::with_active_timer();
       let service = TimeTrackingServiceImpl::new(mock_client);

       let result = service.start_timer(&user_id, req).await;
       assert!(matches!(result, Err(TimeTrackingError::TimerAlreadyRunning)));
   }
   ```

3. **Clarity**: Any developer can understand:
   - Where business logic lives (domain/services)
   - Where external integrations live (adapters/outbound)
   - How HTTP maps to domain (adapters/inbound/http)

4. **Simplicity**: The architecture should feel natural, not forced. If something feels awkward, simplify it.

---

## Notes

- Keep the `milltime` crate as-is; wrap it in an adapter
- Keep the `az-devops` crate as-is; wrap it in an adapter
- Don't refactor Azure DevOps integration unless it helps the Milltime migration
- Each phase should be a separate PR for easier review
- Run `cargo check` and frontend type-check after each change

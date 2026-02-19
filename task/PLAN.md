# Work Items Board View

## Context

There's currently no way to view Azure DevOps work items directly in Toki2. Work items are only shown as linked references in the PR view. This plan adds a per-project sprint board with simplified columns (To Do / In Progress / Done) and a copy-to-Claude feature that formats full work item context for pasting into Claude Code.

The backend follows the **hexagonal architecture** pattern established by time tracking — domain models, ports (inbound/outbound), service implementation, adapters, and a factory for per-request service creation. This enables future GitHub Issues support without changing domain or route logic.

## Implementation Overview

Three layers: (1) extend `az-devops` crate with WIQL + iteration APIs, (2) add backend with hexagonal architecture (domain → adapters → factory → routes), (3) build frontend board UI.

---

## Phase 1: az-devops Crate

### 1.1 Extend `WorkItem` model
**File:** `az-devops/src/models/work_item.rs`

Add fields to `WorkItem` struct and extract from the `fields` HashMap in `From<AzureWorkItem>`:
- `description: Option<String>` — `System.Description` (HTML)
- `acceptance_criteria: Option<String>` — `Microsoft.VSTS.Common.AcceptanceCriteria` (HTML)
- `iteration_path: Option<String>` — `System.IterationPath`
- `area_path: Option<String>` — `System.AreaPath`
- `tags: Option<String>` — `System.Tags` (semicolon-separated)

### 1.2 Add `Iteration` model
**New file:** `az-devops/src/models/iteration.rs`

```rust
pub struct Iteration {
    pub id: i32,
    pub name: String,
    pub path: String,
    pub start_date: Option<OffsetDateTime>,
    pub finish_date: Option<OffsetDateTime>,
}
```

Register in `az-devops/src/models/mod.rs` and re-export from `az-devops/src/lib.rs`.

### 1.3 Add methods to `RepoClient`
**File:** `az-devops/src/repo_client.rs`

**WIQL query** — uses `wit::Client::wiql_client().query_by_wiql()`:
- Signature: `query_work_item_ids_wiql(&self, query: &str, team: &str) -> Result<Vec<i32>, RepoClientError>`
- Returns `WorkItemQueryResult` with `work_items: Vec<WorkItemReference>` where each has `id: Option<i32>`
- The `team` param is required for `@currentIteration` WIQL macro. Default to project name.

**Get iterations** — uses `wit::Client::classification_nodes_client().get(org, project, "iterations", "")`:
- Signature: `get_iterations(&self, depth: Option<i32>) -> Result<Vec<Iteration>, RepoClientError>`
- Returns `WorkItemClassificationNode` tree — recursively flatten, extract `attributes.startDate`/`finishDate` from JSON
- Use `.depth(10)` builder method

**Batch improvement** — add chunking (max 200 per batch) to existing `get_work_items()`.

**Add accessors**: `organization()` and `project()` getters.

### Key API types (verified in crate source)
- `wit::models::Wiql { query: Option<String> }` — WIQL query body
- `WorkItemQueryResult { work_items: Vec<WorkItemReference> }` — WIQL result
- `WorkItemReference { id: Option<i32> }` — work item ref
- `WorkItemClassificationNode { id, name, path, attributes, children, has_children }` — iteration tree node

---

## Phase 2: Backend — Hexagonal Architecture (toki-api)

Follow the time tracking hexagonal pattern: domain models → error → ports → service → adapter → factory → routes.

### 2.1 Domain models
**New file:** `toki-api/src/domain/models/work_item.rs`

Provider-agnostic domain types. Use **string-based IDs** (not `i32`) to support both ADO and future GitHub Issues.

```rust
pub struct WorkItem {
    pub id: String,                        // "12345" (ADO) or "owner/repo#42" (GitHub)
    pub title: String,
    pub state: String,                     // Raw provider state (e.g. "Active")
    pub board_state: BoardState,           // Mapped column: Todo/InProgress/Done
    pub category: WorkItemCategory,        // UserStory/Bug/Task/Feature/Epic/Other
    pub assigned_to: Option<WorkItemPerson>,
    pub priority: Option<i32>,
    pub description: Option<String>,       // Plain text (HTML stripped by adapter)
    pub acceptance_criteria: Option<String>,// Plain text (HTML stripped by adapter)
    pub iteration_path: Option<String>,
    pub area_path: Option<String>,
    pub tags: Vec<String>,
    pub parent: Option<WorkItemRef>,
    pub children: Vec<WorkItemRef>,
    pub related: Vec<WorkItemRef>,
}

pub enum BoardState { Todo, InProgress, Done }

pub enum WorkItemCategory { UserStory, Bug, Task, Feature, Epic, Other(String) }

pub struct WorkItemPerson {
    pub display_name: String,
    pub unique_name: Option<String>,
    pub image_url: Option<String>,
}

pub struct WorkItemRef {
    pub id: String,
    pub title: Option<String>,
}

pub struct Iteration {
    pub id: String,
    pub name: String,
    pub path: String,
    pub start_date: Option<OffsetDateTime>,
    pub finish_date: Option<OffsetDateTime>,
    pub is_current: bool,
}

pub struct WorkItemProject {
    pub organization: String,
    pub project: String,
}
```

Register in `toki-api/src/domain/models/mod.rs`.

### 2.2 Domain error
**New file:** `toki-api/src/domain/work_item_error.rs`

```rust
#[derive(Debug, Error)]
pub enum WorkItemError {
    #[error("project not found: {0}/{1}")]
    ProjectNotFound(String, String),

    #[error("work item not found: {0}")]
    WorkItemNotFound(String),

    #[error("provider error: {0}")]
    ProviderError(String),

    #[error("{0}")]
    Unknown(String),
}
```

Register in `toki-api/src/domain/mod.rs`.

### 2.3 Inbound port — `WorkItemService` trait
**New file:** `toki-api/src/domain/ports/inbound/work_items.rs`

```rust
#[async_trait]
pub trait WorkItemService: Send + Sync + 'static {
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError>;
    async fn get_board_items(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<WorkItem>, WorkItemError>;
    async fn get_work_item(&self, id: &str) -> Result<WorkItem, WorkItemError>;
}
```

Register in `toki-api/src/domain/ports/inbound/mod.rs`.

### 2.4 Outbound port — `WorkItemProvider` trait
**New file:** `toki-api/src/domain/ports/outbound/work_item_provider.rs`

```rust
#[async_trait]
pub trait WorkItemProvider: Send + Sync + 'static {
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError>;
    async fn query_work_item_ids(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<String>, WorkItemError>;
    async fn get_work_items(&self, ids: &[String]) -> Result<Vec<WorkItem>, WorkItemError>;
    async fn get_work_item(&self, id: &str) -> Result<WorkItem, WorkItemError>;
}
```

Register in `toki-api/src/domain/ports/outbound/mod.rs`.

### 2.5 Service implementation
**New file:** `toki-api/src/domain/services/work_items.rs`

```rust
pub struct WorkItemServiceImpl<P: WorkItemProvider> {
    provider: Arc<P>,
}

#[async_trait]
impl<P: WorkItemProvider> WorkItemService for WorkItemServiceImpl<P> {
    async fn get_board_items(...) -> Result<Vec<WorkItem>, WorkItemError> {
        let ids = self.provider.query_work_item_ids(iteration_path, team).await?;
        if ids.is_empty() { return Ok(vec![]); }
        let mut items = self.provider.get_work_items(&ids).await?;
        // Business logic: sort by priority, then by board state
        items.sort_by(|a, b| {
            a.board_state.cmp(&b.board_state)
                .then(a.priority.cmp(&b.priority))
        });
        Ok(items)
    }
    // ... delegate get_iterations and get_work_item to provider
}
```

Single generic type param `P` (no local DB needed, unlike time tracking which has `C` + `R`).

Register in `toki-api/src/domain/services/mod.rs`.

### 2.6 Azure DevOps adapter + conversions
**New directory:** `toki-api/src/adapters/outbound/azure_devops/`

**`mod.rs`** — `AzureDevOpsWorkItemAdapter`

```rust
pub struct AzureDevOpsWorkItemAdapter {
    client: RepoClient,
}

#[async_trait]
impl WorkItemProvider for AzureDevOpsWorkItemAdapter {
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError> {
        let ado_iterations = self.client.get_iterations(None).await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;
        Ok(ado_iterations.into_iter().map(to_domain_iteration).collect())
    }
    // ...
}
```

**`conversions.rs`** — ADO→domain type mapping:
- `to_domain_work_item(ado: az_devops::WorkItem) -> WorkItem` — maps state→BoardState, type→Category, strips HTML from description/acceptance_criteria
- `to_domain_iteration(ado: az_devops::Iteration) -> Iteration` — adds `is_current` calculation
- `map_state(state: &str) -> BoardState` — the STATE_MAP logic (New/Proposed/To Do → Todo, Active/In Progress → InProgress, Done/Closed → Done)
- `map_category(work_item_type: &str) -> WorkItemCategory`
- `strip_html(html: &str) -> String` — basic HTML tag removal for plain-text descriptions

### 2.7 Factory trait (inbound HTTP adapter)
**New file:** `toki-api/src/adapters/inbound/http/work_items.rs`

```rust
#[async_trait]
pub trait WorkItemServiceFactory: Send + Sync + 'static {
    /// Create a WorkItemService scoped to a specific organization and project.
    async fn create_service(
        &self,
        organization: &str,
        project: &str,
    ) -> Result<Box<dyn WorkItemService>, WorkItemServiceError>;

    /// Get all projects the user has access to (cross-project).
    async fn get_available_projects(
        &self,
        user_id: i32,
    ) -> Result<Vec<WorkItemProject>, WorkItemServiceError>;
}

pub struct WorkItemServiceError {
    pub status: StatusCode,
    pub message: String,
}
```

**Key differences from time tracking factory:**
- Takes `organization` + `project` (not `CookieJar`) — ADO uses PAT auth from repo_clients, not cookies
- `get_available_projects()` lives on the factory (cross-project concern), not the service (project-scoped)

Register in `toki-api/src/adapters/inbound/http/mod.rs`.

### 2.8 HTTP response types
**New file or extend:** `toki-api/src/adapters/inbound/http/responses.rs` (or `work_item_responses.rs`)

Serde-annotated response types for JSON serialization:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemResponse {
    pub id: String,
    pub title: String,
    pub state: String,
    pub board_state: String,     // "todo" | "inProgress" | "done"
    pub category: String,        // "userStory" | "bug" | "task" | ...
    pub assigned_to: Option<WorkItemPersonResponse>,
    pub priority: Option<i32>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub iteration_path: Option<String>,
    pub area_path: Option<String>,
    pub tags: Vec<String>,
    pub parent: Option<WorkItemRefResponse>,
    pub children: Vec<WorkItemRefResponse>,
    pub related: Vec<WorkItemRefResponse>,
}
```

Implement `From<WorkItem>` for `WorkItemResponse`, etc.

### 2.9 Factory implementation (composition root)
**File:** `toki-api/src/factory.rs`

Add `AzureDevOpsWorkItemServiceFactory`:

```rust
pub struct AzureDevOpsWorkItemServiceFactory {
    repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>,
    user_repo: Arc<UserRepositoryImpl>,
}

#[async_trait]
impl WorkItemServiceFactory for AzureDevOpsWorkItemServiceFactory {
    async fn create_service(
        &self,
        organization: &str,
        project: &str,
    ) -> Result<Box<dyn WorkItemService>, WorkItemServiceError> {
        // 1. Find any RepoClient matching org+project
        let clients = self.repo_clients.read().await;
        let client = clients.iter()
            .find(|(key, _)| key.organization == organization && key.project == project)
            .map(|(_, client)| client.clone())
            .ok_or_else(|| WorkItemServiceError {
                status: StatusCode::NOT_FOUND,
                message: format!("No client for {}/{}", organization, project),
            })?;

        // 2. Create adapter and service
        let adapter = AzureDevOpsWorkItemAdapter::new(client);
        let service = WorkItemServiceImpl::new(Arc::new(adapter));
        Ok(Box::new(service))
    }

    async fn get_available_projects(
        &self,
        user_id: i32,
    ) -> Result<Vec<WorkItemProject>, WorkItemServiceError> {
        // Get followed repos, deduplicate into unique (org, project) pairs
        let repos = self.user_repo.followed_repositories(user_id).await
            .map_err(|e| WorkItemServiceError { ... })?;
        // Deduplicate and return
    }
}
```

### 2.10 AppState changes
**File:** `toki-api/src/app_state.rs`

Add field and wire up:

```rust
pub struct AppState {
    // ... existing fields ...
    pub work_item_factory: Arc<dyn WorkItemServiceFactory>,
}
```

In `AppState::new()`:
- Create `AzureDevOpsWorkItemServiceFactory` with clones of `repo_clients` Arc and `user_repo` Arc
- Store as `Arc<dyn WorkItemServiceFactory>`

### 2.11 Error integration
**File:** `toki-api/src/routes/error.rs`

Add `From` impls:

```rust
impl From<WorkItemError> for ApiError {
    fn from(err: WorkItemError) -> Self {
        match err {
            WorkItemError::ProjectNotFound(_, _) => Self::not_found(err.to_string()),
            WorkItemError::WorkItemNotFound(_) => Self::not_found(err.to_string()),
            WorkItemError::ProviderError(_) => Self::internal(err.to_string()),
            WorkItemError::Unknown(_) => Self::internal(err.to_string()),
        }
    }
}

impl From<WorkItemServiceError> for ApiError {
    fn from(err: WorkItemServiceError) -> Self {
        Self::new(err.status, err.message)
    }
}
```

### 2.12 Route handlers
**New file:** `toki-api/src/routes/work_items/mod.rs`

Handlers use **factory only** — never touch `RepoClient` or repositories directly:

```rust
// GET /work-items/projects
async fn get_projects(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<WorkItemProjectResponse>>, ApiError> {
    let user = user.ok_or(ApiError::unauthorized("Not authenticated"))?;
    let projects = app_state.work_item_factory.get_available_projects(user.id).await?;
    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

// GET /work-items/iterations?organization=X&project=Y
async fn get_iterations(
    Query(params): Query<ProjectQuery>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<IterationResponse>>, ApiError> {
    let service = app_state.work_item_factory
        .create_service(&params.organization, &params.project).await?;
    let iterations = service.get_iterations().await?;
    Ok(Json(iterations.into_iter().map(Into::into).collect()))
}

// GET /work-items/board?organization=X&project=Y[&iterationPath=Z][&team=T]
async fn get_board(
    Query(params): Query<BoardQuery>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<WorkItemResponse>>, ApiError> {
    let service = app_state.work_item_factory
        .create_service(&params.organization, &params.project).await?;
    let items = service.get_board_items(
        params.iteration_path.as_deref(),
        params.team.as_deref(),
    ).await?;
    Ok(Json(items.into_iter().map(Into::into).collect()))
}
```

### 2.13 Route registration
- `toki-api/src/routes/mod.rs` — add `pub(crate) mod work_items;`
- `toki-api/src/router.rs` — add `.nest("/work-items", routes::work_items::router())`

---

## Phase 3: Frontend

### 3.1 Query factory
**New file:** `app/src/lib/api/queries/workItems.ts`

Types use **string IDs** and receive `boardState`/`category` from the backend (no frontend state mapping needed):

```typescript
type WorkItemProject = { organization: string; project: string };

type BoardWorkItem = {
  id: string;
  title: string;
  state: string;
  boardState: "todo" | "inProgress" | "done";
  category: "userStory" | "bug" | "task" | "feature" | "epic" | "other";
  assignedTo: { displayName: string; uniqueName?: string; imageUrl?: string } | null;
  priority: number | null;
  description: string | null;
  acceptanceCriteria: string | null;
  iterationPath: string | null;
  areaPath: string | null;
  tags: string[];
  parent: { id: string; title?: string } | null;
  children: { id: string; title?: string }[];
  related: { id: string; title?: string }[];
};

type Iteration = {
  id: string;
  name: string;
  path: string;
  startDate: string | null;
  finishDate: string | null;
  isCurrent: boolean;
};
```

Query factory:
- `projects()` — `GET work-items/projects`
- `iterations(org, project)` — `GET work-items/iterations?organization=...&project=...`
- `board(params)` — `GET work-items/board?...`, with `enabled` flag based on org+project being set

Register in `app/src/lib/api/queries/queries.ts`.

### 3.2 Board route and components
**New file:** `app/src/routes/_layout/board/route.tsx`

- Search params: `organization`, `project`, `iterationPath`, `team` (all optional, via zod schema)
- Loader prefetches `workItemProjects()`
- Top bar: project selector dropdown + sprint selector dropdown
- Main content: 3-column board

**New directory:** `app/src/routes/_layout/board/-components/`

| File | Purpose |
|------|---------|
| `board-view.tsx` | 3-column layout, groups items by `boardState` field from API response |
| `board-column.tsx` | Single column with header + item count + card list |
| `board-card.tsx` | Card showing type badge, ID, title, assignee avatar, priority |
| `project-selector.tsx` | Dropdown for org/project selection |
| `sprint-selector.tsx` | Dropdown for iteration, highlights current sprint (`isCurrent` field) |
| `copy-work-item.tsx` | Copy button + clipboard formatting logic |

### 3.3 Board state grouping (frontend-side)

No state mapping needed — the backend adapter maps ADO states to `boardState` values. Frontend simply groups by `boardState` field:

```typescript
const columns = {
  todo: items.filter(i => i.boardState === "todo"),
  inProgress: items.filter(i => i.boardState === "inProgress"),
  done: items.filter(i => i.boardState === "done"),
};
```

### 3.4 Copy-to-Claude format

```markdown
## User Story #1234: Implement user authentication

**State:** Active | **Priority:** 2 | **Assigned To:** John Doe
**Iteration:** Project\Sprint 5 | **Area:** Project\Backend
**Tags:** auth, security

### Description
<plain text from API — already stripped by backend>

### Acceptance Criteria
<plain text from API — already stripped by backend>

### Parent Work Item
#1230 - Authentication Feature

### Related Items
- Child: #1235 - Add login form
- Related: #1236 - Password reset
```

### 3.5 Navigation
**File:** `app/src/components/side-nav.tsx`

Add to `MENU_ITEMS`:
```tsx
{ title: "Board", icon: KanbanSquare, variant: "ghost", to: "/board" }
```

Place between "Pull requests" and "Time Tracking".

---

## Verification

1. **Backend compiles:** `SQLX_OFFLINE=true just check`
2. **Frontend type-checks:** `just tsc` and `just lint`
3. **Manual test:** `just dev`, navigate to Board view
   - Select a project → iterations load
   - Current sprint items appear in 3 columns
   - Switch sprint → board updates
   - Click copy button on a card → paste in editor to verify markdown format

---

## Deliverables

- All Phase 1–3 code changes
- Verification passes (`just check-all`)
- LEARNINGS.md updated with any discoveries

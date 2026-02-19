# Work Items Board View — Task Checklist

## Phase 1: az-devops Crate

### 1.1 Extend WorkItem model
- [x] Add `description: Option<String>` field to `WorkItem` struct (`az-devops/src/models/work_item.rs`)
- [x] Add `acceptance_criteria: Option<String>` field to `WorkItem` struct
- [x] Add `iteration_path: Option<String>` field to `WorkItem` struct
- [x] Add `area_path: Option<String>` field to `WorkItem` struct
- [x] Add `tags: Option<String>` field to `WorkItem` struct
- [x] Extract `System.Description` in `From<AzureWorkItem>` (same `.get().and_then(as_str)` pattern as title)
- [x] Extract `Microsoft.VSTS.Common.AcceptanceCriteria` in `From<AzureWorkItem>`
- [x] Extract `System.IterationPath` in `From<AzureWorkItem>`
- [x] Extract `System.AreaPath` in `From<AzureWorkItem>`
- [x] Extract `System.Tags` in `From<AzureWorkItem>`

### 1.2 Add Iteration model
- [x] Create `az-devops/src/models/iteration.rs` with `Iteration` struct (id, name, path, start_date, finish_date)
  - Use `serde::{Serialize, Deserialize}`, `#[serde(rename_all = "camelCase")]`
  - Use `time::OffsetDateTime` with `#[serde(with = "time::serde::rfc3339::option")]` for date fields
- [x] Register `mod iteration;` and `pub use iteration::*;` in `az-devops/src/models/mod.rs`
  - No change needed in `az-devops/src/lib.rs` since it already does `pub use models::*`

### 1.3 Add RepoClient methods
- [x] Add `pub fn organization(&self) -> &str` getter to `RepoClient`
- [x] Add `pub fn project(&self) -> &str` getter to `RepoClient`
- [x] Add `query_work_item_ids_wiql(&self, query: &str, team: &str) -> Result<Vec<i32>, RepoClientError>`
  - Use `self.work_item_client.wiql_client().query_by_wiql()`
  - Construct `Wiql { query: Some(query.to_string()) }` body
  - Pass `team` parameter to the API call
  - Extract `work_items` from `WorkItemQueryResult`, filter_map on `id`
- [x] Add `get_iterations(&self, depth: Option<i32>) -> Result<Vec<Iteration>, RepoClientError>`
  - Use `self.work_item_client.classification_nodes_client().get(org, project, "iterations", "")`
  - Use `.depth(depth.unwrap_or(10))` builder method
  - Recursively flatten the `WorkItemClassificationNode` tree
  - Extract `attributes.startDate`/`finishDate` from node attributes (JSON)
  - Convert to `Vec<Iteration>`
- [x] Add chunking to `get_work_items()` — split ids into chunks of 200, run batch requests, concat results

### 1.4 Verification
- [x] `cargo check -p az-devops` compiles without errors

---

## Phase 2: Backend — Hexagonal Architecture (toki-api)

### 2.1 Domain models
- [x] Create `toki-api/src/domain/models/work_item.rs`
- [x] Define `WorkItem` struct with string-based `id` field (not `i32`)
- [x] Define `BoardState` enum: `Todo`, `InProgress`, `Done` (with `Serialize`, `Ord`/`PartialOrd`)
- [x] Define `WorkItemCategory` enum: `UserStory`, `Bug`, `Task`, `Feature`, `Epic`, `Other(String)`
- [x] Define `WorkItemPerson` struct: `display_name`, `unique_name`, `image_url`
- [x] Define `WorkItemRef` struct: `id: String`, `title: Option<String>`
- [x] Define `Iteration` struct (domain version): string `id`, `name`, `path`, dates, `is_current: bool`
- [x] Define `WorkItemProject` struct: `organization`, `project`
- [x] Register `mod work_item; pub use work_item::*;` in `toki-api/src/domain/models/mod.rs`

### 2.2 Domain error
- [x] Create `toki-api/src/domain/work_item_error.rs`
- [x] Define `WorkItemError` enum with `thiserror::Error`: `ProjectNotFound(String, String)`, `WorkItemNotFound(String)`, `ProviderError(String)`, `Unknown(String)`
- [x] Register in `toki-api/src/domain/mod.rs`

### 2.3 Inbound port — WorkItemService trait
- [x] Create `toki-api/src/domain/ports/inbound/work_items.rs`
- [x] Define `WorkItemService` trait with `#[async_trait]`, `Send + Sync + 'static` bounds
  - `get_iterations() -> Result<Vec<Iteration>, WorkItemError>`
  - `get_board_items(iteration_path: Option<&str>, team: Option<&str>) -> Result<Vec<WorkItem>, WorkItemError>`
  - `get_work_item(id: &str) -> Result<WorkItem, WorkItemError>`
- [x] Register in `toki-api/src/domain/ports/inbound/mod.rs`

### 2.4 Outbound port — WorkItemProvider trait
- [x] Create `toki-api/src/domain/ports/outbound/work_item_provider.rs`
- [x] Define `WorkItemProvider` trait with `#[async_trait]`, `Send + Sync + 'static` bounds
  - `get_iterations() -> Result<Vec<Iteration>, WorkItemError>`
  - `query_work_item_ids(iteration_path: Option<&str>, team: Option<&str>) -> Result<Vec<String>, WorkItemError>`
  - `get_work_items(ids: &[String]) -> Result<Vec<WorkItem>, WorkItemError>`
  - `get_work_item(id: &str) -> Result<WorkItem, WorkItemError>`
- [x] Register in `toki-api/src/domain/ports/outbound/mod.rs`

### 2.5 Service implementation
- [x] Create `toki-api/src/domain/services/work_items.rs`
- [x] Define `WorkItemServiceImpl<P: WorkItemProvider>` with `provider: Arc<P>`
- [x] Implement `WorkItemService` for `WorkItemServiceImpl<P>`
  - `get_iterations` — delegate to provider
  - `get_board_items` — query IDs, fetch items, sort by board_state then priority
  - `get_work_item` — delegate to provider
- [x] Register in `toki-api/src/domain/services/mod.rs`

### 2.6 Azure DevOps adapter + conversions
- [x] Create `toki-api/src/adapters/outbound/azure_devops/` directory
- [x] Create `toki-api/src/adapters/outbound/azure_devops/mod.rs`
- [x] Define `AzureDevOpsWorkItemAdapter` struct wrapping `RepoClient`
- [x] Implement `WorkItemProvider` for `AzureDevOpsWorkItemAdapter`
  - `get_iterations` — call `client.get_iterations()`, map via `to_domain_iteration()`
  - `query_work_item_ids` — build WIQL query, call `client.query_work_item_ids_wiql()`, convert ids to strings
  - `get_work_items` — parse string IDs to i32, call `client.get_work_items()`, map via `to_domain_work_item()`
  - `get_work_item` — call `client.get_work_items(vec![id])`, return first
- [x] Create `toki-api/src/adapters/outbound/azure_devops/conversions.rs`
- [x] Implement `to_domain_work_item(ado: az_devops::WorkItem) -> domain::WorkItem`
  - Map `state` → `BoardState` via `map_state()`
  - Map `work_item_type` → `WorkItemCategory` via `map_category()`
  - Strip HTML from `description` and `acceptance_criteria` via `strip_html()`
  - Split `tags` string on semicolons into `Vec<String>`
  - Map `assigned_to` → `WorkItemPerson`
- [x] Implement `to_domain_iteration(ado: az_devops::Iteration) -> domain::Iteration`
  - String-ify the `id` field
  - Calculate `is_current` from start/finish dates vs now
- [x] Implement `map_state(state: &str) -> BoardState` — New/Proposed/To Do/Approved → Todo, Active/Committed/In Progress/Doing/Resolved → InProgress, Done/Closed/Completed/Removed → Done, default → Todo
- [x] Implement `map_category(work_item_type: &str) -> WorkItemCategory`
- [x] Implement `strip_html(html: &str) -> String` — basic HTML tag removal
- [x] Register `pub mod azure_devops;` in `toki-api/src/adapters/outbound/mod.rs`

### 2.7 Factory trait (inbound HTTP adapter)
- [x] Create `toki-api/src/adapters/inbound/http/work_items.rs`
- [x] Define `WorkItemServiceFactory` trait with `#[async_trait]`, `Send + Sync + 'static` bounds
  - `create_service(organization: &str, project: &str) -> Result<Box<dyn WorkItemService>, WorkItemServiceError>`
  - `get_available_projects(user_id: i32) -> Result<Vec<WorkItemProject>, WorkItemServiceError>`
- [x] Define `WorkItemServiceError` struct: `status: StatusCode`, `message: String`
- [x] Register in `toki-api/src/adapters/inbound/http/mod.rs`

### 2.8 HTTP response types
- [x] Add response types (in `toki-api/src/adapters/inbound/http/responses.rs` or separate file)
- [x] `WorkItemResponse` with `#[serde(rename_all = "camelCase")]` and `From<domain::WorkItem>`
- [x] `WorkItemPersonResponse` with `From<domain::WorkItemPerson>`
- [x] `WorkItemRefResponse` with `From<domain::WorkItemRef>`
- [x] `IterationResponse` with `From<domain::Iteration>`
- [x] `WorkItemProjectResponse` with `From<domain::WorkItemProject>`

### 2.9 Factory implementation (composition root)
- [x] Add `AzureDevOpsWorkItemServiceFactory` struct in `toki-api/src/factory.rs`
  - Fields: `repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>`, `user_repo: Arc<UserRepositoryImpl>`
- [x] Implement `WorkItemServiceFactory` for `AzureDevOpsWorkItemServiceFactory`
  - `create_service` — find RepoClient matching org+project, wrap in `AzureDevOpsWorkItemAdapter`, create `WorkItemServiceImpl`, return boxed
  - `get_available_projects` — call `user_repo.followed_repositories()`, deduplicate into `(org, project)` pairs

### 2.10 AppState changes
- [x] Add `pub work_item_factory: Arc<dyn WorkItemServiceFactory>` field to `AppState` struct
- [x] Create `AzureDevOpsWorkItemServiceFactory` in `AppState::new()` with clones of `repo_clients` and `user_repo` Arcs
- [x] Update `AppState::new()` to store factory

### 2.11 Error integration
- [x] Add `From<WorkItemError> for ApiError` in `toki-api/src/routes/error.rs`
  - `ProjectNotFound` → `not_found`, `WorkItemNotFound` → `not_found`, `ProviderError` → `internal`, `Unknown` → `internal`
- [x] Add `From<WorkItemServiceError> for ApiError` in `toki-api/src/routes/error.rs`
  - Map `status` + `message` to `ApiError::new()`

### 2.12 Route handlers
- [x] Create `toki-api/src/routes/work_items/mod.rs`
- [x] Define `ProjectQuery` struct: `{ organization: String, project: String }` with camelCase serde
- [x] Define `BoardQuery` struct: `{ organization, project, iteration_path: Option<String>, team: Option<String> }` with camelCase serde
- [x] Implement `GET /projects` handler — uses `app_state.work_item_factory.get_available_projects(user.id)`
- [x] Implement `GET /iterations` handler — creates service via factory, calls `service.get_iterations()`
- [x] Implement `GET /board` handler — creates service via factory, calls `service.get_board_items()`
- [x] Define `pub fn router() -> Router<AppState>` with the three routes

### 2.13 Route registration
- [x] Add `pub(crate) mod work_items;` to `toki-api/src/routes/mod.rs`
- [x] Add `.nest("/work-items", routes::work_items::router())` to `toki-api/src/router.rs`

### 2.14 Verification
- [x] `SQLX_OFFLINE=true just check` compiles without errors

---

## Phase 3: Frontend

### 3.1 Query factory
- [x] Create `app/src/lib/api/queries/workItems.ts`
  - Define `WorkItemProject` type: `{ organization: string; project: string }`
  - Define `BoardWorkItem` type with string `id`, `boardState`, `category`, `assignedTo`, `tags: string[]`, etc.
  - Define `Iteration` type with string `id`, `isCurrent: boolean`, dates
  - Define `workItemsQueries` object with:
    - `baseKey: ["workItems"]`
    - `projects()` — `queryOptions` for `GET work-items/projects`, returns `WorkItemProject[]`
    - `iterations(org, project)` — `queryOptions` for `GET work-items/iterations?organization=...&project=...`
    - `board(params)` — `queryOptions` for `GET work-items/board?...`, with `enabled` flag based on org+project being set
- [x] Register in `app/src/lib/api/queries/queries.ts`:
  - Import `workItemsQueries`
  - Spread into `queries` object

### 3.2 Board route
- [x] Create `app/src/routes/_layout/board/route.tsx`
  - Define zod search schema: `organization`, `project`, `iterationPath`, `team` (all optional strings)
  - `createFileRoute("/_layout/board")` with `validateSearch`, `loader` (prefetch projects), `component`
  - Main component: fetch projects, render `TopBar` + `BoardView`
  - Handle loading/empty states

### 3.3 Board components
- [x] Create `app/src/routes/_layout/board/-components/` directory
- [x] `project-selector.tsx` — Dropdown listing projects, updates search params on select
- [x] `sprint-selector.tsx` — Dropdown listing iterations, highlights current sprint via `isCurrent` field, updates `iterationPath` search param
- [x] `board-view.tsx` — Groups work items by `boardState` field (no STATE_MAP needed — backend provides mapping), renders 3 `BoardColumn`s
- [x] `board-column.tsx` — Column container: header (title + count badge), scrollable list of `BoardCard`s
- [x] `board-card.tsx` — Card with: type badge (color-coded by `category`), `#id`, title, assignee avatar+name, priority indicator, copy button
- [x] `copy-work-item.tsx` — Button that formats a work item as markdown for Claude and copies to clipboard
  - Description/acceptance criteria already plain text from backend (no HTML stripping needed)
  - Format: type + id + title header, state/priority/assignee/iteration/area/tags metadata, description, acceptance criteria, parent, related items

### 3.4 Navigation
- [x] Add `KanbanSquare` import from `lucide-react` in `app/src/components/side-nav.tsx`
- [x] Add `{ title: "Board", icon: KanbanSquare, variant: "ghost", to: "/board" }` to `MENU_ITEMS` array
  - Place between "Pull requests" and "Time Tracking"

### 3.5 Route generation
- [x] Run `just app` or dev server to auto-generate route tree (TanStack Router auto-generates `routeTree.gen.ts`)

### 3.6 Verification
- [x] `just tsc` passes
- [x] `just lint` passes

---

## Final Verification
- [x] `just check-all` passes (backend + frontend)
- [x] Update `task/LEARNINGS.md` with any discoveries made during implementation

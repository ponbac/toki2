# Work Items Board View — Task Checklist

## Phase 1: az-devops Crate

### 1.1 Extend WorkItem model
- [ ] Add `description: Option<String>` field to `WorkItem` struct (`az-devops/src/models/work_item.rs`)
- [ ] Add `acceptance_criteria: Option<String>` field to `WorkItem` struct
- [ ] Add `iteration_path: Option<String>` field to `WorkItem` struct
- [ ] Add `area_path: Option<String>` field to `WorkItem` struct
- [ ] Add `tags: Option<String>` field to `WorkItem` struct
- [ ] Extract `System.Description` in `From<AzureWorkItem>` (same `.get().and_then(as_str)` pattern as title)
- [ ] Extract `Microsoft.VSTS.Common.AcceptanceCriteria` in `From<AzureWorkItem>`
- [ ] Extract `System.IterationPath` in `From<AzureWorkItem>`
- [ ] Extract `System.AreaPath` in `From<AzureWorkItem>`
- [ ] Extract `System.Tags` in `From<AzureWorkItem>`

### 1.2 Add Iteration model
- [ ] Create `az-devops/src/models/iteration.rs` with `Iteration` struct (id, name, path, start_date, finish_date)
  - Use `serde::{Serialize, Deserialize}`, `#[serde(rename_all = "camelCase")]`
  - Use `time::OffsetDateTime` with `#[serde(with = "time::serde::rfc3339::option")]` for date fields
- [ ] Register `mod iteration;` and `pub use iteration::*;` in `az-devops/src/models/mod.rs`
  - No change needed in `az-devops/src/lib.rs` since it already does `pub use models::*`

### 1.3 Add RepoClient methods
- [ ] Add `pub fn organization(&self) -> &str` getter to `RepoClient`
- [ ] Add `pub fn project(&self) -> &str` getter to `RepoClient`
- [ ] Add `query_work_item_ids_wiql(&self, query: &str, team: &str) -> Result<Vec<i32>, RepoClientError>`
  - Use `self.work_item_client.wiql_client().query_by_wiql()`
  - Construct `Wiql { query: Some(query.to_string()) }` body
  - Pass `team` parameter to the API call
  - Extract `work_items` from `WorkItemQueryResult`, filter_map on `id`
- [ ] Add `get_iterations(&self, depth: Option<i32>) -> Result<Vec<Iteration>, RepoClientError>`
  - Use `self.work_item_client.classification_nodes_client().get(org, project, "iterations", "")`
  - Use `.depth(depth.unwrap_or(10))` builder method
  - Recursively flatten the `WorkItemClassificationNode` tree
  - Extract `attributes.startDate`/`finishDate` from node attributes (JSON)
  - Convert to `Vec<Iteration>`
- [ ] Add chunking to `get_work_items()` — split ids into chunks of 200, run batch requests, concat results

### 1.4 Verification
- [ ] `cargo check -p az-devops` compiles without errors

---

## Phase 2: Backend (toki-api)

### 2.1 Add get_project_client to AppState
- [ ] Add method `get_project_client(&self, organization: &str, project: &str) -> Result<RepoClient, AppStateError>` to `AppState`
  - Iterate `repo_clients` map, find first key matching org+project (ignore repo_name since WIT API is project-scoped)
  - Return cloned client or `AppStateError::RepoClientNotFound`

### 2.2 Create work_items route module
- [ ] Create `toki-api/src/routes/work_items.rs`
- [ ] Define `ProjectKey` query struct: `{ organization: String, project: String }` with camelCase serde
- [ ] Define `BoardQuery` struct: `{ organization, project, iteration_path: Option<String>, team: Option<String> }`
- [ ] Implement `GET /projects` handler:
  - Takes `AuthSession` and `State(app_state)`
  - Gets user ID from session, calls `user_repo.followed_repositories()`
  - Deduplicates into unique `(organization, project)` pairs
  - Returns `Json<Vec<ProjectKey>>`
- [ ] Implement `GET /iterations` handler:
  - Takes `Query(ProjectKey)` and `State(app_state)`
  - Calls `app_state.get_project_client(org, project)`
  - Calls `client.get_iterations(None)`
  - Returns `Json<Vec<az_devops::Iteration>>`
- [ ] Implement `GET /board` handler:
  - Takes `Query(BoardQuery)` and `State(app_state)`
  - Gets project client
  - Builds WIQL query: if `iteration_path` present use `[System.IterationPath] = '<path>'`, else use `@currentIteration` macro
  - Calls `client.query_work_item_ids_wiql()`
  - If empty, return empty vec
  - Calls `client.get_work_items(ids)` (chunked)
  - Returns `Json<Vec<az_devops::WorkItem>>`
- [ ] Define `pub fn router() -> Router<AppState>` with the three routes

### 2.3 Register routes
- [ ] Add `pub(crate) mod work_items;` to `toki-api/src/routes/mod.rs`
- [ ] Add `.nest("/work-items", routes::work_items::router())` to `toki-api/src/router.rs` (in `base_app` chain)

### 2.4 Verification
- [ ] `SQLX_OFFLINE=true just check` compiles without errors

---

## Phase 3: Frontend

### 3.1 Query factory
- [ ] Create `app/src/lib/api/queries/workItems.ts`
  - Define `BoardWorkItem` type extending existing `WorkItem` with: `description`, `acceptanceCriteria`, `iterationPath`, `areaPath`, `tags`
  - Define `ProjectKey` type: `{ organization: string; project: string }`
  - Define `Iteration` type: `{ id: number; name: string; path: string; startDate: string | null; finishDate: string | null }`
  - Define `workItemsQueries` object with:
    - `baseKey: ["workItems"]`
    - `projects()` — `queryOptions` for `GET work-items/projects`, returns `ProjectKey[]`
    - `iterations(org, project)` — `queryOptions` for `GET work-items/iterations?organization=...&project=...`
    - `board(params)` — `queryOptions` for `GET work-items/board?...`, with `enabled` flag based on org+project being set
- [ ] Register in `app/src/lib/api/queries/queries.ts`:
  - Import `workItemsQueries`
  - Spread into `queries` object

### 3.2 Board route
- [ ] Create `app/src/routes/_layout/board/route.tsx`
  - Define zod search schema: `organization`, `project`, `iterationPath`, `team` (all optional strings)
  - `createFileRoute("/_layout/board")` with `validateSearch`, `loader` (prefetch projects), `component`
  - Main component: fetch projects, render `TopBar` + `BoardView`
  - Handle loading/empty states

### 3.3 Board components
- [ ] Create `app/src/routes/_layout/board/-components/` directory
- [ ] `project-selector.tsx` — Dropdown (shadcn Select or Popover+Command) listing projects, updates search params on select
- [ ] `sprint-selector.tsx` — Dropdown listing iterations for selected project, highlights current sprint (where now() is between start/finish dates), updates `iterationPath` search param
- [ ] `board-view.tsx` — Takes work items array, applies STATE_MAP to bucket into 3 columns, renders 3 `BoardColumn`s in flex row
  - STATE_MAP: New/Proposed/To Do/Approved → todo, Active/Committed/In Progress/Doing/Resolved → inProgress, Done/Closed/Completed/Removed → done
  - Default unmapped → todo
- [ ] `board-column.tsx` — Column container: header (title + count badge), scrollable list of `BoardCard`s
- [ ] `board-card.tsx` — Card with: work item type badge (color-coded), `#id`, title, assignee avatar+name, priority indicator, copy button
- [ ] `copy-work-item.tsx` — Button that formats a work item as markdown for Claude and copies to clipboard
  - Strip HTML via `new DOMParser().parseFromString(html, "text/html").body.textContent`
  - Format: type + id + title header, state/priority/assignee/iteration/area/tags metadata, description, acceptance criteria, parent, related items

### 3.4 Navigation
- [ ] Add `KanbanSquare` import from `lucide-react` in `app/src/components/side-nav.tsx`
- [ ] Add `{ title: "Board", icon: KanbanSquare, variant: "ghost", to: "/board" }` to `MENU_ITEMS` array
  - Place as first item (before "Pull requests") or between "Pull requests" and "Milltime"

### 3.5 Route generation
- [ ] Run `just app` or dev server to auto-generate route tree (TanStack Router auto-generates `routeTree.gen.ts`)

### 3.6 Verification
- [ ] `just tsc` passes
- [ ] `just lint` passes

---

## Final Verification
- [ ] `just check-all` passes (backend + frontend)
- [ ] Update `task/LEARNINGS.md` with any discoveries made during implementation

# Work Items Board View

## Context

There's currently no way to view Azure DevOps work items directly in Toki2. Work items are only shown as linked references in the PR view. This plan adds a per-project sprint board with simplified columns (To Do / In Progress / Done) and a copy-to-Claude feature that formats full work item context for pasting into Claude Code.

## Implementation Overview

Three layers: (1) extend `az-devops` crate with WIQL + iteration APIs, (2) add backend endpoints, (3) build frontend board UI.

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

## Phase 2: Backend (toki-api)

### 2.1 Add `get_project_client` to AppState
**File:** `toki-api/src/app_state.rs`

Find any `RepoClient` in the `repo_clients` map matching org+project (WIT API is project-scoped, doesn't use `repo_id`).

### 2.2 New route module
**New file:** `toki-api/src/routes/work_items.rs`

Three endpoints:

**`GET /work-items/projects`** (auth required)
- Returns unique `{ organization, project }` pairs derived from user's followed repos
- Reuses `user_repo.followed_repositories()`

**`GET /work-items/iterations?organization=X&project=Y`**
- Calls `client.get_iterations()`
- Returns `Vec<Iteration>`

**`GET /work-items/board?organization=X&project=Y[&iterationPath=Z][&team=T]`**
- If `iterationPath` provided: WIQL with `[System.IterationPath] = '<path>'`
- If omitted: WIQL with `@currentIteration('<project>\<team>')` macro
- `team` defaults to project name
- Fetches IDs via WIQL, then batch-fetches full work items

### 2.3 Register route
- `toki-api/src/routes/mod.rs` — add `pub(crate) mod work_items;`
- `toki-api/src/router.rs` — add `.nest("/work-items", routes::work_items::router())`

---

## Phase 3: Frontend

### 3.1 Query factory
**New file:** `app/src/lib/api/queries/workItems.ts`

- `workItemProjects()` — `GET /work-items/projects`
- `iterations(org, project)` — `GET /work-items/iterations`
- `board({ org, project, iterationPath?, team? })` — `GET /work-items/board`

Extend `BoardWorkItem` type with new fields: `description`, `acceptanceCriteria`, `iterationPath`, `areaPath`, `tags`.

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
| `board-view.tsx` | 3-column layout, groups work items by mapped state |
| `board-column.tsx` | Single column with header + item count + card list |
| `board-card.tsx` | Card showing type badge, ID, title, assignee avatar, priority |
| `project-selector.tsx` | Dropdown for org/project selection |
| `sprint-selector.tsx` | Dropdown for iteration, highlights current sprint |
| `copy-work-item.tsx` | Copy button + clipboard formatting logic |

### 3.3 State mapping (frontend-side)

```typescript
const STATE_MAP: Record<string, "todo" | "inProgress" | "done"> = {
  "New": "todo", "Proposed": "todo", "To Do": "todo", "Approved": "todo",
  "Active": "inProgress", "Committed": "inProgress", "In Progress": "inProgress",
  "Doing": "inProgress", "Resolved": "inProgress",
  "Done": "done", "Closed": "done", "Completed": "done", "Removed": "done",
};
// Unmapped states default to "todo"
```

### 3.4 Copy-to-Claude format

```markdown
## User Story #1234: Implement user authentication

**State:** Active | **Priority:** 2 | **Assigned To:** John Doe
**Iteration:** Project\Sprint 5 | **Area:** Project\Backend
**Tags:** auth, security

### Description
<stripped HTML content>

### Acceptance Criteria
<stripped HTML content>

### Parent Work Item
#1230 - Authentication Feature

### Related Items
- Child: #1235 - Add login form
- Related: #1236 - Password reset
```

HTML stripping via `DOMParser`: `new DOMParser().parseFromString(html, "text/html").body.textContent`

### 3.5 Navigation
**File:** `app/src/components/side-nav.tsx`

Add to `MENU_ITEMS`:
```tsx
{ title: "Board", icon: KanbanSquare, variant: "ghost", to: "/board" }
```

Place between "Pull requests" and "Milltime".

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

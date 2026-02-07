# Agent Prompt — Work Items Board View

This is an idempotent prompt. Any agent can pick this up and continue from wherever the work left off.

---

## Goal

Implement a Work Items Board View for Toki2. The feature adds a per-project sprint board showing Azure DevOps work items in 3 columns (To Do / In Progress / Done) with a copy-to-Claude feature.

## How to Start

1. **Read the plan**: `task/PLAN.md` — full implementation plan with all details
2. **Check progress**: `task/TASKS.md` — granular checklist; check off completed items as you go
3. **Read learnings**: `task/LEARNINGS.md` — codebase patterns, API research, and prior discoveries
4. **Read project instructions**: `CLAUDE.md` — project conventions, tech stack, verification commands

## Implementation Order

Work through the phases in order. Each phase builds on the previous one.

### Phase 1: az-devops Crate (`az-devops/`)
1. Extend `WorkItem` model in `az-devops/src/models/work_item.rs` — add 5 new fields + extract them in `From<AzureWorkItem>`
2. Create `Iteration` model in `az-devops/src/models/iteration.rs`, register in `mod.rs`
3. Add methods to `RepoClient` in `az-devops/src/repo_client.rs`:
   - `organization()` / `project()` getters
   - `query_work_item_ids_wiql()` — WIQL query
   - `get_iterations()` — classification nodes API
   - Chunk `get_work_items()` to max 200 per batch
4. Verify: `cargo check -p az-devops`

### Phase 2: Backend (`toki-api/`)
1. Add `get_project_client()` to `AppState` in `toki-api/src/app_state.rs`
2. Create `toki-api/src/routes/work_items.rs` with 3 endpoints (projects, iterations, board)
3. Register in `toki-api/src/routes/mod.rs` and `toki-api/src/router.rs`
4. Verify: `SQLX_OFFLINE=true just check`

### Phase 3: Frontend (`app/`)
1. Create query factory `app/src/lib/api/queries/workItems.ts`, register in `queries.ts`
2. Create route `app/src/routes/_layout/board/route.tsx`
3. Create components in `app/src/routes/_layout/board/-components/`:
   - `project-selector.tsx`, `sprint-selector.tsx`
   - `board-view.tsx`, `board-column.tsx`, `board-card.tsx`
   - `copy-work-item.tsx`
4. Add nav item in `app/src/components/side-nav.tsx`
5. Verify: `just tsc && just lint`

### Final
- Run `just check-all` to verify everything
- Update `task/LEARNINGS.md` with anything new discovered
- Update `task/TASKS.md` — check off all completed items

## Key Patterns to Follow

### Rust (Backend)
- Serde: `#[serde(rename_all = "camelCase")]` on all structs
- Route handlers: `#[instrument(name = "GET /...", skip(app_state))]`
- Extractors: `State(app_state): State<AppState>`, `Query(query): Query<T>`, `AuthSession`
- Return types: `Result<Json<T>, (StatusCode, String)>` or `Result<Json<T>, AppStateError>`
- Router: `pub fn router() -> Router<AppState>`

### TypeScript (Frontend)
- Query factories: object with `baseKey` + methods returning `queryOptions()`
- API calls: `api.get("path").searchParams({...}).json<Type>()`
- Routes: `createFileRoute("/_layout/board")` with `validateSearch`, `loader`, `component`
- Data: `useSuspenseQuery(queries.methodName())`
- Props: inline in function signatures, use `type` not `interface`
- Components: co-located in `-components/` directory

## Useful Commands
```bash
just check-all          # Verify everything
SQLX_OFFLINE=true just check  # Backend without DB
just tsc                # Frontend type check
just lint               # Frontend lint
just dev                # Run dev servers (backend + frontend)
```

## File Map (what to create/edit)

### New Files
- `az-devops/src/models/iteration.rs`
- `toki-api/src/routes/work_items.rs`
- `app/src/lib/api/queries/workItems.ts`
- `app/src/routes/_layout/board/route.tsx`
- `app/src/routes/_layout/board/-components/project-selector.tsx`
- `app/src/routes/_layout/board/-components/sprint-selector.tsx`
- `app/src/routes/_layout/board/-components/board-view.tsx`
- `app/src/routes/_layout/board/-components/board-column.tsx`
- `app/src/routes/_layout/board/-components/board-card.tsx`
- `app/src/routes/_layout/board/-components/copy-work-item.tsx`

### Files to Edit
- `az-devops/src/models/work_item.rs` — add fields + extraction
- `az-devops/src/models/mod.rs` — register iteration module
- `az-devops/src/repo_client.rs` — add methods + batch chunking
- `toki-api/src/app_state.rs` — add `get_project_client()`
- `toki-api/src/routes/mod.rs` — add `work_items` module
- `toki-api/src/router.rs` — nest work-items routes
- `app/src/lib/api/queries/queries.ts` — register workItems queries
- `app/src/components/side-nav.tsx` — add Board nav item

# Agent Prompt — Work Items Board View

This is an idempotent prompt. Any agent can pick this up and continue from wherever the work left off.

---

## Goal

Implement a Work Items Board View for Toki2. The feature adds a per-project sprint board showing Azure DevOps work items in 3 columns (To Do / In Progress / Done) with a copy-to-Claude feature.

The backend uses **hexagonal architecture** (like time tracking) — domain models, ports, service, adapters, and a factory — to keep the domain provider-agnostic for future GitHub Issues support.

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

### Phase 2: Backend — Hexagonal Architecture (`toki-api/`)
1. **Domain models** — Create `toki-api/src/domain/models/work_item.rs` with provider-agnostic types (`WorkItem`, `BoardState`, `WorkItemCategory`, `WorkItemPerson`, `Iteration`, `WorkItemProject`). String-based IDs.
2. **Domain error** — Create `toki-api/src/domain/work_item_error.rs` with `WorkItemError` enum
3. **Inbound port** — Create `toki-api/src/domain/ports/inbound/work_items.rs` with `WorkItemService` trait (use cases)
4. **Outbound port** — Create `toki-api/src/domain/ports/outbound/work_item_provider.rs` with `WorkItemProvider` trait (provider abstraction)
5. **Service impl** — Create `toki-api/src/domain/services/work_items.rs` with `WorkItemServiceImpl<P: WorkItemProvider>` (business logic, sorting)
6. **ADO adapter** — Create `toki-api/src/adapters/outbound/azure_devops/` with `AzureDevOpsWorkItemAdapter` impl `WorkItemProvider` + `conversions.rs` (state→BoardState, type→Category, HTML stripping)
7. **Factory trait** — Create `toki-api/src/adapters/inbound/http/work_items.rs` with `WorkItemServiceFactory` trait (`create_service(org, project)`, `get_available_projects(user_id)`)
8. **HTTP responses** — Add response types (`WorkItemResponse`, `IterationResponse`, etc.) with serde + `From` domain impls
9. **Factory impl** — Add `AzureDevOpsWorkItemServiceFactory` in `toki-api/src/factory.rs` (finds RepoClient, creates adapter+service)
10. **AppState** — Add `work_item_factory: Arc<dyn WorkItemServiceFactory>` to `toki-api/src/app_state.rs`
11. **Error integration** — Add `From<WorkItemError>` and `From<WorkItemServiceError>` for `ApiError` in `toki-api/src/routes/error.rs`
12. **Route handlers** — Create `toki-api/src/routes/work_items/mod.rs` (handlers use factory only)
13. **Route registration** — Register in `routes/mod.rs` and `router.rs`
14. Verify: `SQLX_OFFLINE=true just check`

### Phase 3: Frontend (`app/`)
1. Create query factory `app/src/lib/api/queries/workItems.ts` with types (string IDs, `boardState`/`category` from backend), register in `queries.ts`
2. Create route `app/src/routes/_layout/board/route.tsx`
3. Create components in `app/src/routes/_layout/board/-components/`:
   - `project-selector.tsx`, `sprint-selector.tsx`
   - `board-view.tsx` (group by `boardState` field — no STATE_MAP needed), `board-column.tsx`, `board-card.tsx`
   - `copy-work-item.tsx`
4. Add nav item in `app/src/components/side-nav.tsx` (between "Pull requests" and "Time Tracking")
5. Verify: `just tsc && just lint`

### Final
- Run `just check-all` to verify everything
- Update `task/LEARNINGS.md` with anything new discovered
- Update `task/TASKS.md` — check off all completed items

## Key Patterns to Follow

### Rust (Backend) — Hexagonal Architecture

**Reference files** (read these to understand the patterns):

| Pattern | Reference File |
|---------|---------------|
| Inbound port trait | `toki-api/src/domain/ports/inbound/time_tracking.rs` |
| Outbound port trait | `toki-api/src/domain/ports/outbound/time_tracking.rs` |
| Service implementation | `toki-api/src/domain/services/time_tracking.rs` |
| Domain error | `toki-api/src/domain/error.rs` |
| Domain models | `toki-api/src/domain/models/timer.rs` |
| Factory trait | `toki-api/src/adapters/inbound/http/time_tracking.rs` |
| Factory impl | `toki-api/src/factory.rs` |
| Outbound adapter | `toki-api/src/adapters/outbound/milltime/mod.rs` |
| HTTP responses | `toki-api/src/adapters/inbound/http/responses.rs` |
| Error integration | `toki-api/src/routes/error.rs` |
| Route handlers | `toki-api/src/routes/time_tracking/` |

**Key principles:**
- **Service traits** use `#[async_trait]` with `Send + Sync + 'static` bounds
- **Service impl** is generic over trait bounds (`WorkItemServiceImpl<P: WorkItemProvider>`), NOT `dyn`
- **Factory** creates per-request service, returns `Box<dyn WorkItemService>` (type erasure at factory boundary)
- **Factory takes `org + project`** (not `CookieJar`) — ADO uses PAT auth from repo_clients
- **`get_available_projects()`** lives on the factory (cross-project), not the service (project-scoped)
- **Adapters** convert provider types → domain types. State mapping (ADO state → BoardState) happens in the adapter, NOT frontend
- **Error flow:** Domain `WorkItemError` → `?` in handler → `From<WorkItemError> for ApiError` → HTTP response
- **`repo_clients`** is `Arc<RwLock<>>` — factory holds a clone of the same Arc
- Serde: `#[serde(rename_all = "camelCase")]` on all response structs
- Route handlers: `#[instrument(name = "GET /...", skip(app_state))]`
- Extractors: `State(app_state): State<AppState>`, `Query(query): Query<T>`, `AuthSession`
- Router: `pub fn router() -> Router<AppState>`

### TypeScript (Frontend)
- Query factories: object with `baseKey` + methods returning `queryOptions()`
- API calls: `api.get("path").searchParams({...}).json<Type>()`
- Routes: `createFileRoute("/_layout/board")` with `validateSearch`, `loader`, `component`
- Data: `useSuspenseQuery(queries.methodName())`
- Props: inline in function signatures, use `type` not `interface`
- Components: co-located in `-components/` directory
- **No state mapping needed** — backend provides `boardState` and `category` fields directly
- **String IDs** — work item IDs are strings (not numbers)
- **`isCurrent`** — iteration has `isCurrent` boolean from backend

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
**Phase 1 (az-devops):**
- `az-devops/src/models/iteration.rs`

**Phase 2 (toki-api — hexagonal layers):**
- `toki-api/src/domain/models/work_item.rs` — domain models
- `toki-api/src/domain/work_item_error.rs` — domain error
- `toki-api/src/domain/ports/inbound/work_items.rs` — inbound port
- `toki-api/src/domain/ports/outbound/work_item_provider.rs` — outbound port
- `toki-api/src/domain/services/work_items.rs` — service impl
- `toki-api/src/adapters/outbound/azure_devops/mod.rs` — ADO adapter
- `toki-api/src/adapters/outbound/azure_devops/conversions.rs` — ADO→domain conversions
- `toki-api/src/adapters/inbound/http/work_items.rs` — factory trait + service error
- `toki-api/src/routes/work_items/mod.rs` — route handlers

**Phase 3 (frontend):**
- `app/src/lib/api/queries/workItems.ts`
- `app/src/routes/_layout/board/route.tsx`
- `app/src/routes/_layout/board/-components/project-selector.tsx`
- `app/src/routes/_layout/board/-components/sprint-selector.tsx`
- `app/src/routes/_layout/board/-components/board-view.tsx`
- `app/src/routes/_layout/board/-components/board-column.tsx`
- `app/src/routes/_layout/board/-components/board-card.tsx`
- `app/src/routes/_layout/board/-components/copy-work-item.tsx`

### Files to Edit
**Phase 1:**
- `az-devops/src/models/work_item.rs` — add fields + extraction
- `az-devops/src/models/mod.rs` — register iteration module
- `az-devops/src/repo_client.rs` — add methods + batch chunking

**Phase 2:**
- `toki-api/src/domain/models/mod.rs` — register work_item module
- `toki-api/src/domain/mod.rs` — register work_item_error module
- `toki-api/src/domain/ports/inbound/mod.rs` — register work_items port
- `toki-api/src/domain/ports/outbound/mod.rs` — register work_item_provider port
- `toki-api/src/domain/services/mod.rs` — register work_items service
- `toki-api/src/adapters/outbound/mod.rs` — register azure_devops module
- `toki-api/src/adapters/inbound/http/mod.rs` — register work_items module
- `toki-api/src/factory.rs` — add AzureDevOpsWorkItemServiceFactory
- `toki-api/src/app_state.rs` — add work_item_factory field + wiring
- `toki-api/src/routes/error.rs` — add From impls for WorkItemError + WorkItemServiceError
- `toki-api/src/routes/mod.rs` — add work_items module
- `toki-api/src/router.rs` — nest work-items routes

**Phase 3:**
- `app/src/lib/api/queries/queries.ts` — register workItems queries
- `app/src/components/side-nav.tsx` — add Board nav item

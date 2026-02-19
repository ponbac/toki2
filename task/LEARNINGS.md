# Learnings — Work Items Board View

Living document for recording discoveries, gotchas, and decisions during implementation.

---

## Codebase Patterns Discovered

### az-devops crate
- **WorkItem field extraction**: Uses `work_item.fields.get("System.FieldName").and_then(|v| v.as_str()).unwrap_or_default().to_owned()` pattern for required string fields, and `.and_then(|v| v.as_i64().map(|n| n as i32))` for optional ints.
- **RepoClient** is `Clone` (all inner clients are cloneable). Fields: `git_client`, `work_item_client`, `graph_client`, `organization`, `project`, `repo_id`.
- **No public getters** exist for `organization` or `project` on `RepoClient` — need to add them.
- **get_work_items** currently takes a raw `Vec<i32>` with no batching — Azure DevOps batch API has a 200-item limit per request.
- **Dependencies**: Uses `azure_devops_rust_api` v0.28.0 with features `git`, `pipelines`, `wit`, `graph`. Error type from `typespec::error::Error`.
- **Serde**: All models use `#[serde(rename_all = "camelCase")]` for JSON serialization.

### toki-api backend — Hexagonal Architecture

The codebase has **two route patterns**:

1. **Simple pattern** (PRs, repos, notifications): Route handlers directly use `AppState` fields (`repo_clients`, repositories). Good for features that don't need provider abstraction.

2. **Factory-based hexagonal pattern** (time tracking, work items): Full DDD layers — domain models → ports (inbound/outbound) → service → adapters → factory → routes. Used when provider swappability matters (Milltime→future, ADO→GitHub).

**Key hexagonal principles:**
- **Inbound port** (`domain/ports/inbound/`): Trait defining use cases (e.g. `TimeTrackingService`, `WorkItemService`). Marked `#[async_trait]` with `Send + Sync + 'static` bounds.
- **Outbound port** (`domain/ports/outbound/`): Trait abstracting external providers (e.g. `TimeTrackingClient`, `WorkItemProvider`). Provider-specific details hidden behind this.
- **Service impl** (`domain/services/`): Generic over outbound port bounds (`ServiceImpl<P: Provider>`). Contains business logic (sorting, validation). NOT `dyn` at the struct level — generics used for compile-time monomorphization.
- **Adapter** (`adapters/outbound/`): Implements outbound port. Thin wrapper around external client. Converts provider types → domain types. Error mapping happens here.
- **Factory trait** (`adapters/inbound/http/`): Knows about HTTP types. Creates per-request services, returns `Box<dyn Service>` (type erasure at factory boundary).
- **Factory impl** (`factory.rs`): Composition root. Knows about concrete types. Finds the right client, creates adapter + service, boxes it.
- **AppState**: Holds `Arc<dyn Factory>` — NOT services. Services are created per-request.
- **Error flow**: Domain error → `?` operator in handler → `From<DomainError> for ApiError` impl → HTTP response. Factory errors also have `From` impls.

**Work items factory vs time tracking factory:**
- Time tracking: `create_service(CookieJar, cookie_domain)` → extracts Milltime credentials from cookies. Per-user auth.
- Work items: `create_service(organization, project)` → finds `RepoClient` matching org+project from `repo_clients`. PAT-based auth (already in `RepoClient`).
- Work items: `get_available_projects(user_id)` lives on factory (cross-project concern). Service is project-scoped.
- Time tracking: Has two outbound ports (`TimeTrackingClient` + `TimerHistoryRepository`). Service generic over both: `<C, R>`.
- Work items: Single outbound port (`WorkItemProvider`). Service generic over one: `<P>`. No local DB storage needed.

**State mapping in adapter, not frontend:**
- ADO states like "New", "Active", "Done" are mapped to `BoardState` enum (`Todo`/`InProgress`/`Done`) in the `AzureDevOpsWorkItemAdapter` conversions.
- Frontend receives `boardState: "todo" | "inProgress" | "done"` — just groups by the field.
- When GitHub support is added, its adapter will do its own state mapping.

**Domain models are provider-agnostic:**
- IDs are `String` (not `i32`) to support both ADO numeric IDs and GitHub `owner/repo#42` style.
- `WorkItemCategory` enum covers common types across providers.
- HTML stripping done in adapter — domain models store plain text.

**`repo_clients` access pattern:**
- `Arc<RwLock<HashMap<RepoKey, RepoClient>>>` — multiple repos in same project have separate clients.
- Factory holds clone of same `Arc` — doesn't copy data, shares the lock.
- For project-scoped operations (WIT API), find _any_ client matching org+project.

### Frontend
- **Query factories**: Objects with `baseKey` array and methods returning `queryOptions()`. Registered by spreading into `queries` object in `queries.ts`. Existing factories: `userQueries`, `differsQueries`, `pullRequestsQueries`, `commitsQueries`, `timeTrackingQueries`.
- **API client**: `ky` instance at `app/src/lib/api/api.ts`, uses `api.get("path").json<Type>()`. Includes credentials for cookies.
- **Route pattern**: `createFileRoute("/_layout/path")` with `validateSearch` (zod), `loader` (prefetch), `component`. Uses `useSuspenseQuery` for data.
- **Co-located components**: In `-components/` directory next to `route.tsx`.
- **Side nav**: `MENU_ITEMS` array: Pull requests → Time Tracking → Repositories. `{ title, icon, variant, to }` shape.
- **Work item types** use string IDs, `boardState`/`category` come from backend, `isCurrent` boolean on iterations from backend. No frontend state mapping needed.

---

## API Research

### WIQL API
- Endpoint: `POST /{organization}/{project}/{team}/_apis/wit/wiql`
- Body: `{ "query": "SELECT [System.Id] FROM WorkItems WHERE ..." }`
- Response: `WorkItemQueryResult { work_items: Vec<WorkItemReference> }`
- The `@currentIteration` macro requires the `{team}` URL segment — defaults to project name if not specified.

### Classification Nodes API (Iterations)
- Endpoint: `GET /{organization}/{project}/_apis/wit/classificationnodes/iterations`
- Returns tree: `WorkItemClassificationNode { id, name, path, attributes, children }`
- `attributes` is a generic JSON object with optional `startDate`, `finishDate` strings
- Need to recursively flatten the tree to get all iterations

### Azure DevOps Batch Work Items
- Endpoint: `POST /{organization}/{project}/_apis/wit/workitemsbatch`
- Max 200 IDs per request — must chunk

---

## Decisions Made

_(Record implementation decisions and their rationale here as work progresses)_

---

## Issues Encountered

_(Record any problems, workarounds, or unexpected behavior here)_

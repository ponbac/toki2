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

### toki-api backend
- **AppState.repo_clients**: `HashMap<RepoKey, RepoClient>` — keyed by `{organization, project, repo_name}`. Multiple repos in the same project will have separate clients. For project-scoped WIT operations, we just need to find _any_ client matching org+project.
- **RepoKey**: `{ organization, project, repo_name }` with `camelCase` serde rename. Has `new()`, `Display`, `Hash`, `Eq`.
- **Route pattern**: Each module exports `pub fn router() -> Router<AppState>`, handlers use `State(app_state)`, `Query(params)`, `AuthSession` extractors. Return `Result<Json<T>, (StatusCode, String)>` or `Result<Json<T>, AppStateError>`.
- **Auth**: `AuthSession` extractor provides `auth_session.user` (Option<User>). User has `id: i32`, `email: String`.
- **followed_repositories()**: Returns `Vec<RepoKey>` from the `UserRepository` trait. Already used in `pull_requests.rs` `get_followed_pull_requests()`.

### Frontend
- **Query factories**: Objects with `baseKey` array and methods returning `queryOptions()`. Registered by spreading into `queries` object in `queries.ts`.
- **API client**: `ky` instance at `app/src/lib/api/api.ts`, uses `api.get("path").json<Type>()`.
- **Route pattern**: `createFileRoute("/_layout/path")` with `validateSearch` (zod), `loader` (prefetch), `component`. Uses `useSuspenseQuery` for data.
- **Co-located components**: In `-components/` directory next to `route.tsx`.
- **Side nav**: `MENU_ITEMS` array with `{ title, icon, variant, to }`. Type is `satisfies readonly { ... }[]`.
- **Existing WorkItem type** in `pullRequests.ts` doesn't have description/acceptanceCriteria/iterationPath/areaPath/tags — need new extended type.

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

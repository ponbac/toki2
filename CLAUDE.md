## Project Overview

Toki2 is a **provider-agnostic** time tracking and development workflow platform:

- Track work time via pluggable time tracking providers (currently: Milltime)
- Monitor pull requests across pluggable SCM providers (currently: Azure DevOps)
- Real-time notifications for PR activity (comments, closures, mentions)
- Generate time entry notes from work items linked to PRs

**Design goal:** The system is designed to support multiple time tracking backends (not just Milltime) and multiple PR/issue providers (not just Azure DevOps). All new backend features should be built with this provider-agnosticism in mind.

## Tech Stack

**Backend (Rust)**: Axum, SQLx (PostgreSQL), Tokio, azure_devops_rust_api, web-push
**Frontend (React/TS)**: Vite, TanStack Router + Query, Zustand, shadcn/ui, Tailwind
**Package manager**: Bun — never use npm or npx, always use `bun` / `bunx`

## Project Structure

```
toki-api/       # Main Axum backend
az-devops/      # Azure DevOps API wrapper crate
milltime/       # Milltime API client crate (reverse-engineered, no official docs)
app/            # React frontend
```

## Version Control

**jj (Jujutsu) is preferred over git.** This repo uses jj for version control. Use `jj` commands instead of `git` when possible.

## Architecture Principles

**Hexagonal architecture (ports & adapters)** is the target pattern for all backend domain logic. When adding or modifying backend features:

1. **Domain logic must not depend on specific providers.** Business logic lives in `domain/services/` and depends only on traits defined in `domain/ports/`. Never import `az_devops::*` or `milltime::*` from domain code.
2. **Define ports (traits) for external interactions.** Inbound ports in `domain/ports/inbound/` define use cases. Outbound ports in `domain/ports/outbound/` define what the domain needs from external systems.
3. **Adapters implement ports.** Provider-specific code lives in `adapters/outbound/{provider}/`. HTTP handlers live in `adapters/inbound/http/`.
4. **Prefer extending existing traits** over creating provider-specific shortcuts. If a new capability is needed, add it to the relevant port trait and implement it in the adapter.

### Abstraction Status

| Domain | Status | Notes |
|--------|--------|-------|
| **Time tracking** | Fully hexagonal | `TimeTrackingClient` + `TimeTrackingService` ports, `MilltimeAdapter` |
| **Work items** | Partial | `WorkItemProvider` outbound port exists, missing inbound service trait |
| **Pull requests** | Not abstracted | `RepoDiffer` directly uses `az_devops::RepoClient` — needs refactoring |
| **Notifications** | Not abstracted | Coupled to PR change events |

When working on partially or non-abstracted domains, prefer moving toward the hexagonal pattern rather than adding more provider coupling.

## Key Patterns

### Backend

- **Repository pattern**: Database access via traits (`UserRepository`, etc.) with `*Impl` implementations
- **AppState**: Shared state container passed via Axum extractors
- **RepoDiffer workers**: Background tasks polling ADO for PR changes, communicating via mpsc channels (note: tightly coupled to Azure DevOps, future refactoring target)
- **SQLx offline mode**: `.sqlx/` caches query metadata. Set `SQLX_OFFLINE=true` to compile without a live DB. Run `cargo sqlx prepare` after changing SQL queries

### Time Tracking Architecture (Hexagonal)

The time tracking system is the **reference implementation** of the hexagonal pattern. New domains should follow this structure:

```
toki-api/src/
├── domain/
│   ├── models/           # Domain types (ActiveTimer, TimeEntry, Project, etc.)
│   ├── ports/
│   │   ├── inbound/      # TimeTrackingService trait (use cases)
│   │   └── outbound/     # TimeTrackingClient, TimerHistoryRepository traits
│   ├── services/         # TimeTrackingServiceImpl (business logic)
│   └── error.rs          # TimeTrackingError
└── adapters/
    ├── inbound/http/     # TimeTrackingServiceExt, HTTP response types
    └── outbound/
        ├── milltime/     # MilltimeAdapter (implements TimeTrackingClient)
        └── postgres/     # PostgresTimerHistoryAdapter (implements TimerHistoryRepository)
```

**Key traits:**
- `TimeTrackingClient` (outbound): Interface for time tracking providers (timer, projects, calendar)
- `TimerHistoryRepository` (outbound): Interface for local timer history storage
- `TimeTrackingService` (inbound): Use cases for HTTP handlers
- `TimeTrackingServiceExt`: Creates service instances from HTTP cookies

**Per-request service creation:** The service is created per-request from cookies (not stored in AppState) because credentials are user-specific. The service merges provider data with local timer history for accurate start/end times.

### Frontend

- **File-based routing**: TanStack Router (`_layout/` for layouts, `$param` for dynamic routes)
- **Query factories**: Queries in factory objects returning `queryOptions()`
- **Co-located components**: Route-specific components in `-components/` directories
- **Props inline**: Inline props in function signatures, prefer `type` over `interface`

## Development

Use `just` to run common commands (run `just` to see all available recipes):

```bash
just dev        # Run both backend and frontend

# Or individually:
just init-db    # Initialize database (first time setup)
just run        # Run backend
just app        # Run frontend dev server
```

## Verifying Changes

```bash
just check-all  # Verify everything (backend + frontend)

# Or individually:
just check      # Backend - verify Rust compiles
just tsc        # Frontend - TypeScript check
just lint       # Frontend - ESLint

# Without a running database, use SQLX_OFFLINE=true:
SQLX_OFFLINE=true just check
```

## Important Notes

1. **Provider-agnostic by default** - New backend features should use ports/adapters, not couple directly to Milltime or Azure DevOps
2. **Milltime API is unofficial** - reverse-engineered, no documentation
3. **Minimal test coverage** - be careful with changes
4. **Coordinated changes** - Backend API changes typically require frontend updates
5. **Route generation** - Don't edit `routeTree.gen.ts` manually
6. **shadcn/ui** - Components in `app/src/components/ui/` are from shadcn/ui

## Configuration

Backend config: `toki-api/config/{base,local,production}.yaml` + `TOKI_*` env vars  
Required secrets: Azure AD OAuth credentials, `MT_CRYPTO_KEY` for Milltime password encryption

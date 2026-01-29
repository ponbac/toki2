## Project Overview

Toki2 is a time tracking and Azure DevOps integration platform:

- Track work time and sync with Milltime (a time tracking system)
- Monitor pull requests across Azure DevOps repositories
- Real-time notifications for PR activity (comments, closures, mentions)
- Generate time entry notes from work items linked to PRs

## Tech Stack

**Backend (Rust)**: Axum, SQLx (PostgreSQL), Tokio, azure_devops_rust_api, web-push  
**Frontend (React/TS)**: Vite, TanStack Router + Query, Zustand, shadcn/ui, Tailwind

## Project Structure

```
toki-api/       # Main Axum backend
az-devops/      # Azure DevOps API wrapper crate
milltime/       # Milltime API client crate (reverse-engineered, no official docs)
app/            # React frontend
```

## Version Control

**jj (Jujutsu) is preferred over git.** This repo uses jj for version control. Use `jj` commands instead of `git` when possible.

## Key Patterns

### Backend

- **Repository pattern**: Database access via traits (`UserRepository`, etc.) with `*Impl` implementations
- **AppState**: Shared state container passed via Axum extractors
- **RepoDiffer workers**: Background tasks polling ADO for PR changes, communicating via mpsc channels
- **SQLx offline mode**: `.sqlx/` caches query metadata. Set `SQLX_OFFLINE=true` to compile without a live DB. Run `cargo sqlx prepare` after changing SQL queries
- **Hexagonal architecture for time tracking**: See below

### Time Tracking Architecture (Hexagonal)

The time tracking system uses hexagonal architecture to decouple from Milltime:

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

1. **Milltime API is unofficial** - reverse-engineered, no documentation
2. **Minimal test coverage** - be careful with changes
3. **Coordinated changes** - Backend API changes typically require frontend updates
4. **Route generation** - Don't edit `routeTree.gen.ts` manually
5. **shadcn/ui** - Components in `app/src/components/ui/` are from shadcn/ui

## Configuration

Backend config: `toki-api/config/{base,local,production}.yaml` + `TOKI_*` env vars  
Required secrets: Azure AD OAuth credentials, `MT_CRYPTO_KEY` for Milltime password encryption

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
- **SQLx offline mode**: `.sqlx/` caches query metadata. Run `cargo sqlx prepare` after changing SQL queries
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

**Adding a new time tracking provider:**
1. Create adapter in `adapters/outbound/new_provider/`
2. Implement `TimeTrackingClient` trait for your adapter
3. Update `TimeTrackingServiceExt` in `adapters/inbound/http/time_tracking.rs` to use your adapter

**Current state:** Calendar routes (`routes/milltime/calendar.rs`) use the hexagonal architecture via `TimeTrackingService`. Timer routes are not yet migrated. HTTP responses use dedicated response types that convert from domain models.

### Frontend

- **File-based routing**: TanStack Router (`_layout/` for layouts, `$param` for dynamic routes)
- **Query factories**: Queries in factory objects returning `queryOptions()`
- **Co-located components**: Route-specific components in `-components/` directories
- **Props inline**: Inline props in function signatures, prefer `type` over `interface`

## Development

```bash
# Backend (fresh DB setup if needed)
cd toki-api && ./scripts/init_db.sh
bacon run

# Frontend
cd app && bun dev
```

## Verifying Changes

```bash
# Backend - verify Rust changes compile
cargo check

# Frontend - verify TypeScript and linting
cd app && bun tsc && bun lint
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

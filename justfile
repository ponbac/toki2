# Toki2 justfile - common development commands
# Run `just` to see available recipes

# Default recipe shows help
default:
    @just --list

# === Backend (Rust) ===

# Run the backend
run:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/dev-env.sh
    cd toki-api && cargo run

# Check backend compiles
check:
    cargo check

# Run cargo clippy
clippy:
    cargo clippy

# Build backend in release mode
build:
    cargo build --release

# Initialize the database (requires Docker)
init-db:
    cd toki-api && ./scripts/init_db.sh

# Pull production Dokploy Postgres DB over Tailscale SSH and restore into local DB
db-prod-pull *args:
    ./toki-api/scripts/db_prod_pull.sh {{ args }}

# Prepare SQLx offline query data (run after changing SQL queries)
sqlx-prepare:
    cargo sqlx prepare --workspace

# === Frontend (React/TS) ===

# Run frontend dev server
app:
    cd app && bun dev

# TypeScript check
tsc:
    cd app && bun tsc

# Lint frontend
lint:
    cd app && bun lint

# Build frontend for production
build-app:
    cd app && bun run build

# Preview production frontend build
preview-app:
    cd app && VITE_API_URL=http://localhost:8180 bun run build && bun run preview -- --port 5173 --strictPort

# === Combined ===

# Run both backend and frontend
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT
    (
        source scripts/dev-env.sh
        cd toki-api && cargo run
    ) &
    (cd app && bun dev) &
    wait

# Build the frontend, then run backend with production frontend preview
preview:
    #!/usr/bin/env bash
    set -euo pipefail
    cd app && VITE_API_URL=http://localhost:8180 bun run build
    cd ..
    trap 'kill 0' EXIT
    (
        source scripts/dev-env.sh
        cd toki-api && cargo run
    ) &
    (cd app && bun run preview -- --port 5173 --strictPort) &
    wait

# Run both backend and frontend against the Kleer test environment
dev-sandbox:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f toki-api/.env.local ]]; then
        set -a
        source toki-api/.env.local
        set +a
    fi
    : "${TOKI_KLEER__SANDBOX_TOKEN:?Set TOKI_KLEER__SANDBOX_TOKEN in toki-api/.env.local}"
    export TOKI_KLEER__TOKEN="$TOKI_KLEER__SANDBOX_TOKEN"
    export TOKI_KLEER__COMPANY_ID="${TOKI_KLEER__COMPANY_ID:-4875}"
    export TOKI_KLEER__BASE_URL="${TOKI_KLEER__BASE_URL:-https://test-api.kleer.se/v1}"
    trap 'kill 0' EXIT
    (
        set -a
        [[ -f .env ]] && source .env
        set +a
        cd toki-api && cargo run
    ) &
    (cd app && bun dev) &
    wait

# Verify all code compiles/passes checks
check-all: check clippy tsc lint

# Format frontend code with prettier
fmt:
    cd app && bunx prettier --write src/

# === TUI ===

# Run the TUI (requires login — run `just tui-login` first if needed)
tui:
    cd toki-tui && cargo run -- run

# Run the TUI in dev mode (no login required, mock data)
tui-dev:
    cd toki-tui && cargo run -- dev

# Authenticate the TUI via browser OAuth
tui-login:
    cd toki-tui && cargo run -- login

# Log out (clear saved session)
tui-logout:
    cd toki-tui && cargo run -- logout

# Print TUI config path and create default config if missing
tui-config:
    cd toki-tui && cargo run -- config-path

# Print toki-tui version
tui-version:
    cd toki-tui && cargo run -- version

# Show toki-tui session status
tui-status:
    cd toki-tui && cargo run -- status

# Print the log notes directory path
tui-logs:
    cd toki-tui && cargo run -- logs-path

# Toki2 justfile - common development commands
# Run `just` to see available recipes

# Default recipe shows help
default:
    @just --list

# === Backend (Rust) ===

# Run the backend
run:
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

# Pull production Fly Postgres DB and restore into local DB
db-prod-pull *args:
    cd toki-api && ./scripts/db_prod_pull.sh {{args}}

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

# Preview production build
preview:
    cd app && bun preview

# === Combined ===

# Run both backend and frontend
dev:
    #!/usr/bin/env bash
    trap 'kill 0' EXIT
    (cd toki-api && cargo run) &
    (cd app && bun dev) &
    wait

# Verify all code compiles/passes checks
check-all: check clippy tsc lint

# Format frontend code with prettier
fmt:
    cd app && bunx prettier --write src/

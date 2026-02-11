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

# === TUI Development ===

# Initialize TUI dev database (creates isolated database)
init-tui-db:
    @echo "🔧 Creating isolated dev database for TUI..."
    @echo "⚠️  This will NOT touch your production database!"
    PGPASSWORD=password createdb -U postgres -h localhost toki_tui_dev || echo "Database already exists (that's OK)"
    @echo "📦 Running migrations on toki_tui_dev..."
    cd toki-api && DATABASE_URL=postgres://postgres:password@localhost:5432/toki_tui_dev sqlx migrate run
    @echo "👤 Creating test user..."
    PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c "INSERT INTO users (email, full_name, picture, access_token, roles) VALUES ('test@example.com', 'Test User', 'https://example.com/avatar.jpg', 'test_token', ARRAY['User']::text[]) ON CONFLICT (email) DO NOTHING;" > /dev/null
    @echo "✅ TUI dev database ready!"

# Run the TUI (with dev database)
tui:
    cd toki-tui && cargo run

# Reset TUI dev database (clean slate for testing)
reset-tui-db:
    @echo "🗑️  Dropping TUI dev database..."
    PGPASSWORD=password dropdb -U postgres -h localhost toki_tui_dev --if-exists
    @echo "🔧 Recreating..."
    just init-tui-db

# Check which databases exist (verify isolation)
check-dbs:
    @echo "📊 Existing databases:"
    PGPASSWORD=password psql -U postgres -h localhost -c "SELECT datname FROM pg_database WHERE datname LIKE 'toki%';"

# Check timer entries in TUI dev database
check-timers:
    @echo "📋 Timer entries in dev database:"
    @echo ""
    @echo "Active timers:"
    @PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c "SELECT id, start_time, project_name, activity_name, note FROM timer_history WHERE end_time IS NULL;" || echo "No active timers"
    @echo ""
    @echo "Recent timer history (last 10):"
    @PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c "SELECT id, start_time, end_time, project_name, activity_name, note FROM timer_history ORDER BY start_time DESC LIMIT 10;"

# Open interactive database shell for TUI dev database
db-shell:
    @echo "Opening database shell for toki_tui_dev..."
    @echo "Useful commands:"
    @echo "  \dt              - List tables"
    @echo "  \d timer_history - Describe timer_history table"
    @echo "  \q               - Quit"
    @echo ""
    PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev

# Create test timer history entries for demo
create-test-timers:
    @echo "📝 Creating test timer history entries..."
    @PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
        "INSERT INTO timer_history (user_id, start_time, end_time, project_name, activity_name, note) VALUES \
        (1, NOW() - INTERVAL '5 hours', NOW() - INTERVAL '3 hours', 'Toki2 Development', 'Backend Development', 'Fixed timer persistence bug'), \
        (1, NOW() - INTERVAL '8 hours', NOW() - INTERVAL '6 hours', 'TUI Development', 'Feature Implementation', 'Added project selection UI'), \
        (1, NOW() - INTERVAL '2 days', NOW() - INTERVAL '2 days' + INTERVAL '90 minutes', 'Azure DevOps Integration', 'API Integration', 'Implemented PR webhooks'), \
        (1, NOW() - INTERVAL '3 days', NOW() - INTERVAL '3 days' + INTERVAL '45 minutes', 'Internal Tools', 'Documentation', 'Updated setup guide'), \
        (1, NOW() - INTERVAL '1 day', NOW() - INTERVAL '1 day' + INTERVAL '2 hours', 'TUI Development', 'UI Design', 'Designed history view layout') \
        ON CONFLICT DO NOTHING;"
    @echo "✅ Created 5 test timer entries"
    @just check-timers

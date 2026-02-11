#!/usr/bin/env bash
# Setup script for Toki TUI development environment
# This creates an ISOLATED database that won't touch your production data

set -e

echo "🚀 Toki TUI Setup"
echo "================="
echo ""
echo "⚠️  This script will create an ISOLATED dev database."
echo "✅ Your production database will NOT be touched!"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Database configuration
DB_USER=${POSTGRES_USER:=postgres}
DB_PASSWORD="${POSTGRES_PASSWORD:=password}"
DB_NAME="toki_tui_dev"
DB_PORT="${POSTGRES_PORT:=5432}"
DB_HOST="${POSTGRES_HOST:=localhost}"

echo "📊 Database Configuration:"
echo "   Host: $DB_HOST:$DB_PORT"
echo "   User: $DB_USER"
echo "   Database: $DB_NAME (DEV ONLY)"
echo ""

# Check if psql is available
if ! command -v psql &> /dev/null; then
    echo -e "${RED}❌ Error: psql is not installed.${NC}"
    echo ""
    echo "Install it with:"
    echo "  Ubuntu/Debian: sudo apt install postgresql-client"
    echo "  macOS: brew install postgresql"
    exit 1
fi

# Check if sqlx is available
if ! command -v sqlx &> /dev/null; then
    echo -e "${YELLOW}⚠️  Warning: sqlx-cli is not installed.${NC}"
    echo ""
    echo "Install it with:"
    echo "  cargo install sqlx-cli --no-default-features --features rustls,postgres"
    echo ""
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check if production database exists (warn user)
export PGPASSWORD="${DB_PASSWORD}"
if psql -h "${DB_HOST}" -U "${DB_USER}" -p "${DB_PORT}" -lqt | cut -d \| -f 1 | grep -qw "toki"; then
    echo -e "${GREEN}✅ Found production database 'toki' (will not be modified)${NC}"
else
    echo -e "${YELLOW}⚠️  Production database 'toki' not found (that's OK for testing)${NC}"
fi

# Check if dev database already exists
if psql -h "${DB_HOST}" -U "${DB_USER}" -p "${DB_PORT}" -lqt | cut -d \| -f 1 | grep -qw "${DB_NAME}"; then
    echo -e "${YELLOW}⚠️  Dev database '${DB_NAME}' already exists${NC}"
    read -p "Drop and recreate it? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "🗑️  Dropping ${DB_NAME}..."
        dropdb -h "${DB_HOST}" -U "${DB_USER}" -p "${DB_PORT}" "${DB_NAME}"
    else
        echo "Keeping existing database."
        exit 0
    fi
fi

# Create dev database
echo "🔧 Creating dev database '${DB_NAME}'..."
createdb -h "${DB_HOST}" -U "${DB_USER}" -p "${DB_PORT}" "${DB_NAME}"

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✅ Database created successfully!${NC}"
else
    echo -e "${RED}❌ Failed to create database${NC}"
    exit 1
fi

# Run migrations
if command -v sqlx &> /dev/null; then
    echo "📦 Running migrations..."
    DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}"
    export DATABASE_URL
    
    cd "$(dirname "$0")/../toki-api"
    sqlx migrate run
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ Migrations completed!${NC}"
    else
        echo -e "${RED}❌ Migration failed${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠️  Skipping migrations (sqlx-cli not installed)${NC}"
fi

echo ""
echo -e "${GREEN}🎉 Setup complete!${NC}"
echo ""
echo "Next steps:"
echo "  1. cd toki-tui"
echo "  2. cargo run"
echo ""
echo "Or use justfile:"
echo "  just tui"
echo ""
echo "📊 Verify isolation:"
echo "  just check-dbs"
echo ""

#!/usr/bin/env bash
# Quick setup script that handles Docker group and PATH issues

set -e

# Add cargo to PATH for this session
export PATH="$HOME/.cargo/bin:$PATH"

echo "🔧 Toki TUI Quick Setup"
echo "======================="
echo ""

# Check if docker works
echo "Checking Docker access..."
if docker ps > /dev/null 2>&1; then
    echo "✅ Docker is accessible"
else
    echo "❌ Docker permission denied"
    echo ""
    echo "You're in the docker group, but it's not active in this session."
    echo ""
    echo "Please run ONE of these commands first:"
    echo ""
    echo "  Option 1 (Recommended - Permanent):"
    echo "    From Windows PowerShell: wsl --shutdown"
    echo "    Then restart WSL"
    echo ""
    echo "  Option 2 (Temporary - This session only):"
    echo "    newgrp docker"
    echo "    Then run this script again"
    echo ""
    exit 1
fi

echo ""
echo "✅ All tools available:"
echo "   - Docker: $(docker --version | cut -d',' -f1)"
echo "   - PostgreSQL client: $(psql --version)"
echo "   - sqlx-cli: $(sqlx --version)"
echo "   - just: $(just --version)"
echo ""

# Check if PostgreSQL container is running
echo "Checking for PostgreSQL container..."
if docker ps --format '{{.Names}}' | grep -q 'toki2'; then
    echo "✅ PostgreSQL container 'toki2' is already running"
else
    echo "📦 PostgreSQL container not found. Starting it..."
    cd /home/alx/Code/other/toki2/toki-api
    ./scripts/init_db.sh
    cd /home/alx/Code/other/toki2
fi

# Check if toki_tui_dev database exists
echo ""
echo "Checking for TUI dev database..."
if psql -U postgres -h localhost -lqt 2>/dev/null | cut -d \| -f 1 | grep -qw toki_tui_dev; then
    echo "✅ Dev database 'toki_tui_dev' exists"
else
    echo "📦 Creating isolated dev database 'toki_tui_dev'..."
    createdb -U postgres -h localhost toki_tui_dev
    
    echo "📦 Running migrations..."
    export DATABASE_URL=postgres://postgres:password@localhost:5432/toki_tui_dev
    cd /home/alx/Code/other/toki2/toki-api
    sqlx migrate run
    cd /home/alx/Code/other/toki2
    echo "✅ Dev database created and migrated"
fi

echo ""
echo "🎉 Setup complete!"
echo ""
echo "You can now run:"
echo "  just tui"
echo ""
echo "Or manually:"
echo "  cd toki-tui && cargo run"
echo ""

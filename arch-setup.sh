#!/usr/bin/env bash
# Arch Linux (WSL) setup script for Toki TUI development
# Supports Docker Desktop for Windows with WSL2 integration

set -e

echo "🏗️  Toki TUI - Arch Linux (WSL) Setup"
echo "======================================"
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if running as root
if [ "$EUID" -eq 0 ]; then 
    echo -e "${RED}⚠️  Don't run this as root/sudo${NC}"
    echo "Run as your normal user. It will ask for sudo when needed."
    exit 1
fi

echo "📦 Installing required packages..."
echo ""

# Update package database
echo "Updating package database..."
sudo pacman -Sy

# Install PostgreSQL client tools
if ! command -v psql &> /dev/null; then
    echo -e "${YELLOW}Installing PostgreSQL client...${NC}"
    sudo pacman -S --noconfirm postgresql-libs
    echo -e "${GREEN}✅ PostgreSQL client installed${NC}"
else
    echo -e "${GREEN}✅ PostgreSQL client already installed${NC}"
fi

# Check Rust/Cargo
if ! command -v cargo &> /dev/null; then
    echo -e "${YELLOW}Installing Rust...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    echo -e "${GREEN}✅ Rust installed${NC}"
else
    echo -e "${GREEN}✅ Rust already installed ($(rustc --version))${NC}"
fi

# Install just (command runner)
if ! command -v just &> /dev/null; then
    echo -e "${YELLOW}Installing just...${NC}"
    echo "This will take a couple minutes..."
    cargo install just
    echo -e "${GREEN}✅ just installed${NC}"
else
    echo -e "${GREEN}✅ just already installed${NC}"
fi

# Install sqlx-cli
if ! command -v sqlx &> /dev/null; then
    echo -e "${YELLOW}Installing sqlx-cli...${NC}"
    echo "This will take a few minutes..."
    cargo install sqlx-cli --no-default-features --features rustls,postgres
    echo -e "${GREEN}✅ sqlx-cli installed${NC}"
else
    echo -e "${GREEN}✅ sqlx-cli already installed${NC}"
fi

# Check Docker (Docker Desktop for Windows with WSL2 integration)
echo ""
echo -e "${BLUE}🐋 Checking Docker setup...${NC}"

if ! command -v docker &> /dev/null; then
    echo -e "${RED}❌ Docker not found${NC}"
    echo ""
    echo "It looks like Docker Desktop for Windows with WSL2 integration is not set up."
    echo "Please:"
    echo "  1. Make sure Docker Desktop is running on Windows"
    echo "  2. Go to Docker Desktop Settings → Resources → WSL Integration"
    echo "  3. Enable integration for your Arch distro"
    exit 1
fi

# Check if docker group exists (created by Docker Desktop integration)
if ! getent group docker > /dev/null 2>&1; then
    echo -e "${YELLOW}⚠️  Docker group doesn't exist yet${NC}"
    echo "Creating docker group..."
    sudo groupadd docker
fi

# Check if user is in docker group
if ! groups | grep -q docker; then
    echo -e "${YELLOW}⚠️  Adding user to docker group...${NC}"
    sudo usermod -aG docker $USER
    
    echo ""
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${YELLOW}⚠️  IMPORTANT: You need to restart your WSL session${NC}"
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "The docker group change won't take effect until you restart."
    echo ""
    echo "Run these commands:"
    echo "  1. exit                    (close this WSL session)"
    echo "  2. wsl --shutdown          (from Windows PowerShell)"
    echo "  3. Start WSL again"
    echo ""
    echo "Or run this in the current shell:"
    echo "  newgrp docker"
    echo ""
    
    NEEDS_RESTART=true
else
    echo -e "${GREEN}✅ User already in docker group${NC}"
    NEEDS_RESTART=false
fi

# Test Docker connection
echo ""
if docker ps > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Docker is working!${NC}"
else
    echo -e "${YELLOW}⚠️  Docker command found but can't connect${NC}"
    echo ""
    echo "Possible issues:"
    echo "  1. Docker Desktop is not running on Windows"
    echo "  2. WSL2 integration is not enabled for this distro"
    echo "  3. You need to restart WSL (if just added to docker group)"
    echo ""
    echo "To fix:"
    echo "  - Make sure Docker Desktop is running on Windows"
    echo "  - Check Docker Desktop → Settings → Resources → WSL Integration"
    echo "  - Enable integration for your distro"
    if [ "$NEEDS_RESTART" = true ]; then
        echo "  - Restart WSL: 'wsl --shutdown' in PowerShell, then restart"
    fi
fi

echo ""
echo -e "${GREEN}🎉 Package installation complete!${NC}"
echo ""

if [ "$NEEDS_RESTART" = true ]; then
    echo "⚠️  Before continuing, restart WSL to activate docker group:"
    echo ""
    echo "  From Windows PowerShell:"
    echo "    wsl --shutdown"
    echo ""
    echo "  Then restart your WSL distro"
    echo ""
else
    echo "Next steps:"
    echo "  1. Initialize the database: just init-tui-db"
    echo "  2. Run the TUI: just tui"
    echo ""
fi

#!/usr/bin/env bash
# Quick fix for Docker Desktop + WSL2 permission issue

echo "🔧 Fixing Docker permissions..."
echo ""

# Check if docker group exists
if ! getent group docker > /dev/null 2>&1; then
    echo "Creating docker group..."
    sudo groupadd docker
fi

# Check if user is in docker group (in /etc/group)
if getent group docker | grep -q "$USER"; then
    echo "✅ You're in the docker group (in /etc/group)"
else
    echo "Adding you to docker group..."
    sudo usermod -aG docker $USER
fi

# Check if docker group is active in current session
if groups | grep -q docker; then
    echo "✅ Docker group is active in current session"
else
    echo "⚠️  Docker group is NOT active in current session"
    echo ""
    echo "The group membership exists but hasn't taken effect yet."
    echo ""
    echo "To activate it, choose one option:"
    echo ""
    echo "Option 1: Restart WSL (recommended)"
    echo "  From Windows PowerShell, run:"
    echo "    wsl --shutdown"
    echo "  Then start WSL again"
    echo ""
    echo "Option 2: Use newgrp (temporary fix for current shell)"
    echo "  Run this command:"
    echo "    newgrp docker"
    echo "  Then continue with setup"
    echo ""
    exit 1
fi

# Test Docker
echo ""
echo "Testing Docker connection..."
if docker ps > /dev/null 2>&1; then
    echo "✅ Docker is working!"
else
    echo "❌ Docker test failed"
    echo ""
    echo "Possible issues:"
    echo "  1. Docker Desktop is not running on Windows"
    echo "  2. WSL2 integration not enabled for this distro"
    echo ""
    echo "To fix:"
    echo "  1. Start Docker Desktop on Windows"
    echo "  2. Go to: Settings → Resources → WSL Integration"
    echo "  3. Enable integration for 'Arch'"
    echo ""
fi

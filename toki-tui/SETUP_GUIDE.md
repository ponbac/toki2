# 🚀 Toki TUI Setup Guide

**Complete guide to setting up your isolated TUI development environment.**

## ⚠️ Important: Safety First!

This setup creates a **completely separate database** (`toki_tui_dev`) that:
- ✅ Is isolated from your production `toki` database
- ✅ Can be dropped/recreated safely
- ✅ Won't affect your real timers
- ✅ Perfect for experimentation

---

## Prerequisites Checklist

Before you start, make sure you have:

- [ ] **Rust toolchain** (check: `rustc --version`)
- [ ] **PostgreSQL running** (usually via Docker)
- [ ] **psql client** (check: `psql --version`)
  - Ubuntu/Debian: `sudo apt install postgresql-client`
  - macOS: `brew install postgresql`
- [ ] **sqlx-cli** (check: `sqlx --version`)
  - Install: `cargo install sqlx-cli --no-default-features --features rustls,postgres`

---

## Setup Methods

Choose one of these methods:

### Method 1: Automated Setup (Recommended)

```bash
# From the toki-tui directory
cd toki-tui
./setup.sh
```

This script will:
1. Check for the production database (to verify it won't be touched)
2. Create `toki_tui_dev` database
3. Run all migrations
4. Verify everything is ready

### Method 2: Using Justfile

```bash
# From the project root
just init-tui-db
```

### Method 3: Manual Setup

If you prefer to do it step-by-step:

#### Step 1: Create the Database

```bash
# Create the dev database
createdb -U postgres -h localhost toki_tui_dev

# Verify it was created
psql -U postgres -h localhost -c "\l" | grep toki
# Should show both 'toki' and 'toki_tui_dev'
```

#### Step 2: Run Migrations

```bash
# Set the database URL to the dev database
export DATABASE_URL=postgres://postgres:password@localhost:5432/toki_tui_dev

# Run migrations from the toki-api directory
cd toki-api
sqlx migrate run
```

#### Step 3: Verify Setup

```bash
# Connect to the dev database
psql postgres://postgres:password@localhost:5432/toki_tui_dev

# List tables (should see timer_history, users, etc.)
\dt

# Exit
\q
```

---

## Verify Isolation

**Critical step**: Make sure your production database is safe!

```bash
# Method 1: Using justfile
just check-dbs

# Method 2: Using psql directly
psql -U postgres -h localhost -c "SELECT datname FROM pg_database WHERE datname LIKE 'toki%';"

# You should see:
#   datname
# -------------
#   toki            <- Your production database (safe!)
#   toki_tui_dev    <- Your dev database (safe to break!)
```

---

## Configuration

### Environment Variables

The TUI uses `.env.tui` for configuration:

```bash
cd toki-tui
cat .env.tui
```

**Default configuration:**
```bash
# Dev database (isolated from production)
DATABASE_URL=postgres://postgres:password@localhost:5432/toki_tui_dev

# Milltime (optional - needed only if testing Milltime integration)
MILLTIME_URL=https://your.milltime.instance/cgi/mt.cgi
MT_CRYPTO_KEY=your_base64_encoded_crypto_key

# Disable auth for development
DISABLE_AUTH=true
```

### Custom Database Credentials

If your PostgreSQL uses different credentials, update `.env.tui`:

```bash
DATABASE_URL=postgres://YOUR_USER:YOUR_PASSWORD@localhost:5432/toki_tui_dev
```

---

## First Run

### Option 1: Using Justfile (Recommended)

```bash
just tui
```

### Option 2: Using Cargo Directly

```bash
cd toki-tui
cargo run
```

### What You Should See

```
🔍 Connecting to database: postgres://postgres:password@localhost:5432/toki_tui_dev
⚠️  Using ISOLATED dev database (toki_tui_dev)
✅ Your production database is safe!

✅ Connected to database successfully!

┌─────────────────────────────────┐
│        Toki Timer TUI           │
└─────────────────────────────────┘

┌─────────────────────────────────┐
│ Timer                           │
│   ⏱  00:00:00 (Stopped)         │
└─────────────────────────────────┘

...
```

### Controls

- **Space**: Start/Stop timer
- **R**: Refresh from database
- **Q**: Quit

---

## Troubleshooting

### Problem: "Failed to connect to database"

**Cause**: Database doesn't exist or PostgreSQL isn't running.

**Solution**:
```bash
# Check if PostgreSQL is running
docker ps | grep postgres

# If not running, start it
cd toki-api
./scripts/init_db.sh

# Then create the TUI database
just init-tui-db
```

### Problem: "sqlx-cli not found"

**Cause**: `sqlx-cli` not installed.

**Solution**:
```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
```

### Problem: "psql command not found"

**Cause**: PostgreSQL client tools not installed.

**Solution**:
```bash
# Ubuntu/Debian
sudo apt install postgresql-client

# macOS
brew install postgresql

# Windows (via WSL)
sudo apt install postgresql-client
```

### Problem: "Database toki_tui_dev does not exist"

**Cause**: Setup not completed.

**Solution**:
```bash
just init-tui-db
```

### Problem: "Permission denied creating database"

**Cause**: PostgreSQL user doesn't have CREATE DATABASE permission.

**Solution**:
```bash
# Connect as superuser
psql -U postgres -h localhost

# Grant permissions
ALTER USER postgres CREATEDB;
\q
```

### Problem: TUI crashes immediately

**Check these**:
1. Database connection works: `psql postgres://postgres:password@localhost:5432/toki_tui_dev`
2. Migrations ran: `psql postgres://postgres:password@localhost:5432/toki_tui_dev -c "\dt"`
3. `.env.tui` exists in `toki-tui/` directory
4. Terminal supports colors/Unicode (try Windows Terminal on Windows)

---

## Testing the Setup

### Test 1: Start a Timer

1. Run the TUI: `just tui`
2. Press **Space** to start timer
3. You should see "⏱ 00:00:01 (Running)"
4. Press **Q** to quit

### Test 2: Verify Database Isolation

```bash
# Check dev database has a timer
psql postgres://postgres:password@localhost:5432/toki_tui_dev \
  -c "SELECT * FROM timer_history WHERE end_time IS NULL;"

# Check production database is untouched
psql postgres://postgres:password@localhost:5432/toki \
  -c "SELECT COUNT(*) FROM timer_history;"
# (Count should match what you had before)
```

### Test 3: Refresh from Database

1. Run TUI: `just tui`
2. Start timer (Space)
3. Quit (Q)
4. Run TUI again: `just tui`
5. Press **R** to refresh
6. Timer should show with correct elapsed time

---

## Resetting the Environment

If you want to start completely fresh:

### Full Reset

```bash
# Drop and recreate the dev database
just reset-tui-db
```

### Partial Reset (Keep Database, Clear Data)

```bash
# Connect to dev database
psql postgres://postgres:password@localhost:5432/toki_tui_dev

# Delete all timers
DELETE FROM timer_history;

# Exit
\q
```

---

## Next Steps

Now that your environment is set up:

1. **Experiment freely** - Your production database is safe!
2. **Start/stop timers** - Test the basic functionality
3. **Check the code** - Look at `src/main.rs` to understand how it works
4. **Add features** - Follow the README.md for contribution guidelines

### Suggested Experiments

1. Start a timer, quit, restart - does it remember?
2. Start multiple timers (should only allow one active)
3. Edit the project name in the code
4. Add a new keyboard shortcut

---

## Production Database Safety

### How We Ensure Safety

1. **Different database name**: `toki_tui_dev` vs `toki`
2. **Explicit configuration**: `.env.tui` always points to dev database
3. **Visual warnings**: TUI shows which database it's using on startup
4. **No defaults**: Won't accidentally use production database

### Double-Check Anytime

```bash
# Show which database the TUI will use
cd toki-tui
grep DATABASE_URL .env.tui

# Should show:
# DATABASE_URL=postgres://postgres:password@localhost:5432/toki_tui_dev
```

---

## Getting Help

If you run into issues:

1. **Check this guide** - Most common issues are covered
2. **Check the README** - More detailed information
3. **Check the logs** - TUI shows connection info on startup
4. **Verify isolation** - Run `just check-dbs` to see databases

---

## Summary Checklist

Before you start developing:

- [ ] Dev database created (`toki_tui_dev`)
- [ ] Migrations ran successfully
- [ ] TUI runs and connects to database
- [ ] Verified production database is separate
- [ ] Can start/stop timers
- [ ] Timers persist across TUI restarts

If all checked ✅ - **You're ready to develop!** 🎉

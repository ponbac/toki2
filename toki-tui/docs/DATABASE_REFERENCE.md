# 🗄️ Database Cheat Sheet

Quick reference for checking timer entries and database contents.

## Quick Justfile Commands

### Check Timer Entries
```bash
just check-timers
```
Shows:
- Active timers (end_time IS NULL)
- Recent timer history (last 10 entries)

### Check Databases
```bash
just check-dbs
```
Lists all toki databases (production and dev)

### Open Database Shell
```bash
just db-shell
```
Opens interactive psql session to `toki_tui_dev`

---

## Manual SQL Queries

All commands use the dev database (`toki_tui_dev`).

### Active Timers

```bash
# Check if a timer is currently running
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT id, start_time, project_name, activity_name, note 
   FROM timer_history 
   WHERE end_time IS NULL;"
```

### Timer History

```bash
# Last 10 timers
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT id, start_time, end_time, project_name, note 
   FROM timer_history 
   ORDER BY start_time DESC 
   LIMIT 10;"
```

### Timer Statistics

```bash
# Count total timers
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT 
     COUNT(*) as total,
     COUNT(*) FILTER (WHERE end_time IS NULL) as active,
     COUNT(*) FILTER (WHERE end_time IS NOT NULL) as completed
   FROM timer_history;"
```

### Today's Timers

```bash
# Timers started today
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT id, start_time, end_time, project_name, note 
   FROM timer_history 
   WHERE start_time::date = CURRENT_DATE 
   ORDER BY start_time DESC;"
```

### Calculate Timer Duration

```bash
# Show timer duration in minutes
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT 
     id,
     start_time,
     end_time,
     EXTRACT(EPOCH FROM (end_time - start_time))/60 as minutes,
     project_name,
     note
   FROM timer_history 
   WHERE end_time IS NOT NULL
   ORDER BY start_time DESC 
   LIMIT 10;"
```

---

## Interactive Database Session

### Open Shell
```bash
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev
```

### Useful psql Commands

```sql
-- List all tables
\dt

-- Describe table structure
\d timer_history
\d users

-- See all timers (formatted)
\x  -- Expanded display (one column per line)
SELECT * FROM timer_history ORDER BY start_time DESC LIMIT 5;

-- Turn off expanded display
\x

-- Quit
\q
```

---

## Common Queries

### 1. Check What's Running Now

```sql
SELECT 
  id,
  start_time,
  NOW() - start_time as elapsed,
  project_name,
  activity_name,
  note
FROM timer_history 
WHERE end_time IS NULL;
```

### 2. Find Specific Timer by ID

```sql
SELECT * FROM timer_history WHERE id = 1;
```

### 3. Delete a Timer

```sql
-- Delete specific timer
DELETE FROM timer_history WHERE id = 1;

-- Delete all timers (careful!)
DELETE FROM timer_history;
```

### 4. Update a Timer Note

```sql
UPDATE timer_history 
SET note = 'Updated note' 
WHERE id = 1;
```

### 5. Stop an Active Timer Manually

```sql
UPDATE timer_history 
SET end_time = NOW() 
WHERE end_time IS NULL;
```

---

## Database Schema

### timer_history Table

```sql
Column         | Type                     | Description
---------------|--------------------------|---------------------------
id             | SERIAL PRIMARY KEY       | Auto-incrementing ID
user_id        | INT NOT NULL             | User who owns the timer
start_time     | TIMESTAMPTZ NOT NULL     | When timer started
end_time       | TIMESTAMPTZ              | When timer stopped (NULL = active)
registration_id| TEXT                     | Milltime entry ID (after save)
project_id     | TEXT                     | Project identifier
project_name   | TEXT                     | Project display name
activity_id    | TEXT                     | Activity identifier
activity_name  | TEXT                     | Activity display name
note           | TEXT                     | Timer note/description
created_at     | TIMESTAMPTZ NOT NULL     | Record creation time
```

### users Table

```sql
Column            | Type                  | Description
------------------|-----------------------|---------------------------
id                | SERIAL PRIMARY KEY    | User ID
email             | TEXT NOT NULL UNIQUE  | User email
full_name         | TEXT NOT NULL         | User display name
picture           | TEXT NOT NULL         | Avatar URL
access_token      | TEXT NOT NULL         | OAuth token
roles             | TEXT[] NOT NULL       | User roles (Admin, User)
session_auth_hash | TEXT NOT NULL         | Session validation hash
```

---

## Aliases for Convenience

Add these to your `~/.zshrc` or `~/.bashrc`:

```bash
# Database aliases
alias tui-db='PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev'
alias tui-timers='PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c "SELECT * FROM timer_history ORDER BY start_time DESC LIMIT 10;"'
alias tui-active='PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c "SELECT * FROM timer_history WHERE end_time IS NULL;"'
```

Then use:
```bash
tui-db           # Open database shell
tui-timers       # Show recent timers
tui-active       # Show active timer
```

---

## Production vs Development

### Check Production Database (READ ONLY!)

**⚠️ Be careful with production database!**

```bash
# Count production users (safe, read-only)
PGPASSWORD=password psql -U postgres -h localhost -d toki -c \
  "SELECT COUNT(*) FROM users;"

# Count production timers (safe, read-only)
PGPASSWORD=password psql -U postgres -h localhost -d toki -c \
  "SELECT COUNT(*) FROM timer_history;"
```

**DO NOT run DELETE or UPDATE on production database!**

### Compare Databases

```bash
# Show users in each database
echo "Production users:"
PGPASSWORD=password psql -U postgres -h localhost -d toki -c \
  "SELECT id, email FROM users LIMIT 5;"

echo ""
echo "Dev users:"
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT id, email FROM users;"
```

---

## Example Workflow

### Start Timer → Check → Stop

```bash
# 1. Start TUI and create a timer
just tui
# Press Space, then Q

# 2. Check the active timer
just check-timers

# 3. See the details
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT 
     id,
     start_time,
     NOW() - start_time as elapsed,
     project_name,
     note
   FROM timer_history 
   WHERE end_time IS NULL;"

# 4. Stop it in TUI
just tui
# Press Space to stop, then Q
```

---

## Troubleshooting

### "relation timer_history does not exist"

**Fix**: Run migrations
```bash
just init-tui-db
```

### "FATAL: database toki_tui_dev does not exist"

**Fix**: Create database
```bash
just init-tui-db
```

### "permission denied for table timer_history"

**Fix**: Check you're connecting to the right database with correct user
```bash
# Should connect as 'postgres' user
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev
```

---

## Quick Reference Card

```bash
# Check timers
just check-timers              # Show active and recent timers
just check-dbs                 # List databases
just db-shell                  # Open database shell

# Manual queries
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c "SELECT * FROM timer_history;"

# Common tasks
just reset-tui-db              # Reset database (clears all data)
just tui                       # Run TUI
```

---

## Tips

1. **Use `just check-timers`** - Easiest way to see what's in the database
2. **Use `just db-shell`** - For interactive exploration
3. **Use `\x` in psql** - Toggle expanded display for easier reading
4. **Always use dev database** - Never modify production directly
5. **Reset if confused** - `just reset-tui-db` gives you a clean slate

Happy querying! 🗄️

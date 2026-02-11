# Test User for Development

## Default Test User

The TUI dev database includes a test user that's automatically created:

```
Email: test@example.com
User ID: 1
Name: Test User
```

This user is used by the TUI when authentication is disabled (`DISABLE_AUTH=true` in `.env.tui`).

## How It Works

When you run `just init-tui-db`, it:
1. Creates the `toki_tui_dev` database
2. Runs all migrations
3. Creates the test user (if it doesn't exist)

The test user allows you to:
- ✅ Start/stop timers without authentication
- ✅ Test timer functionality immediately
- ✅ Develop without OAuth setup

## Production vs Development

| Environment | Database | Users |
|-------------|----------|-------|
| **Production** | `toki` | Your real users (from Azure AD OAuth) |
| **Development** | `toki_tui_dev` | Test user only |

## Creating Additional Test Users

If you need more test users for development:

```bash
export PGPASSWORD=password

psql -U postgres -h localhost -d toki_tui_dev -c "
INSERT INTO users (email, full_name, picture, access_token, roles) 
VALUES (
  'developer@example.com',
  'Developer Test',
  'https://example.com/dev.jpg',
  'dev_token',
  ARRAY['User', 'Admin']::text[]
);"
```

## Resetting Test Data

To start fresh with a clean database (including recreating the test user):

```bash
just reset-tui-db
```

This will:
1. Drop the `toki_tui_dev` database
2. Recreate it
3. Run migrations
4. Create the test user again

## Safety Note

⚠️ The test user is **only in the dev database** (`toki_tui_dev`).

Your production database (`toki`) is **not affected** and does not have this test user.

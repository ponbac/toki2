# Toki TUI - Terminal User Interface for Time Tracking

A cross-platform terminal UI for the Toki time tracking system, built with Ratatui.

## ⚠️ Safety First: Isolated Development Environment

**This TUI uses a completely separate database (`toki_tui_dev`) from your production setup.**

- ✅ Your production database (`toki`) is **never touched**
- ✅ All timer operations happen in the isolated dev database
- ✅ Safe to experiment without breaking your real timers

## Quick Start

### 1. Prerequisites

Make sure you have:
- ✅ Rust toolchain installed
- ✅ PostgreSQL running (usually via Docker)
- ✅ `psql` client installed (for database creation)
- ✅ `sqlx-cli` installed: `cargo install sqlx-cli --no-default-features --features rustls,postgres`

### 2. Initialize the Dev Database

```bash
# From the project root
just init-tui-db
```

This will:
1. Create a new database called `toki_tui_dev`
2. Run all migrations to set up the schema
3. Leave your production `toki` database untouched

### 3. Configure Environment

Edit `toki-tui/.env.tui` if needed:

```bash
# The dev database (already configured)
DATABASE_URL=postgres://postgres:password@localhost:5432/toki_tui_dev

# Milltime config (if you want to test Milltime integration later)
MILLTIME_URL=https://your.milltime.instance/cgi/mt.cgi
MT_CRYPTO_KEY=your_base64_encoded_crypto_key

# Disable auth for development
DISABLE_AUTH=true
```

### 4. Run the TUI

```bash
just tui
```

Or manually:

```bash
cd toki-tui
cargo run
```

## Features

### Current Features ✅

- ✅ View active timer with live elapsed time
- ✅ Start timer (Space key)
- ✅ Stop timer (Space key)
- ✅ Refresh from database (R key)
- ✅ Cross-platform (Linux, Windows PowerShell, macOS)
- ✅ Isolated dev database (safe experimentation)

### Planned Features 🚧

- [ ] Edit timer note
- [ ] Select project/activity from list
- [ ] View timer history
- [ ] Save timer to Milltime
- [ ] Authentication (session reuse or device code flow)
- [ ] Git integration (auto-fill notes from commits)
- [ ] Pomodoro mode
- [ ] Global hotkeys (via daemon)

## Controls

| Key | Action |
|-----|--------|
| `Space` | Start/Stop timer |
| `R` | Refresh timer from database |
| `Q` | Quit |
| `Ctrl+C` | Quit |

## Architecture

```
toki-tui/
├── src/
│   ├── main.rs          # Entry point, terminal setup
│   ├── app.rs           # Application state management
│   ├── ui/              # Ratatui UI components
│   │   └── mod.rs       # Rendering logic
│   ├── api/             # Database access
│   │   ├── mod.rs
│   │   └── database.rs  # Timer queries
│   └── auth/            # Authentication (placeholder)
│       └── mod.rs
├── Cargo.toml           # Dependencies
└── .env.tui            # Dev environment config
```

## Database Isolation

### How It Works

The TUI connects to `toki_tui_dev` instead of `toki`:

```
┌─────────────────────────────────────┐
│  Production (Web App)               │
│  Database: toki                     │
│  Your real timers are here          │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│  Development (TUI)                  │
│  Database: toki_tui_dev             │
│  Safe to experiment                 │
└─────────────────────────────────────┘
```

### Verify Isolation

```bash
# Check which databases exist
just check-dbs

# Should show both:
# - toki (production)
# - toki_tui_dev (development)
```

### Reset Dev Database

If you want to start fresh:

```bash
just reset-tui-db
```

This drops and recreates `toki_tui_dev` (production database is untouched).

## Cross-Platform Compatibility

### Linux / WSL ✅

Works perfectly out of the box.

### Windows ✅

- ✅ PowerShell 5.1+
- ✅ PowerShell 7+
- ✅ Windows Terminal (recommended)
- ✅ cmd.exe (basic colors)
- ✅ Git Bash

**Recommendation for colleagues**: Install [Windows Terminal](https://apps.microsoft.com/store/detail/windows-terminal/9N0DX20HK701) for the best experience.

### macOS ✅

Works in Terminal.app, iTerm2, and other terminals.

## Development

### Building

```bash
# Debug build
cd toki-tui
cargo build

# Release build (optimized)
cargo build --release
```

### Testing Without Database

If you want to test the UI without a database connection, you can modify `main.rs` to use mock data instead of connecting to PostgreSQL.

### Adding New Features

The codebase follows a simple structure:

1. **Database queries**: Add to `src/api/database.rs`
2. **UI components**: Add to `src/ui/mod.rs`
3. **Application state**: Modify `src/app.rs`
4. **Event handling**: Add to `src/main.rs` (in `run_app`)

## Troubleshooting

### "Failed to connect to database"

**Solution**: Make sure the dev database exists:

```bash
just init-tui-db
```

### "Database toki_tui_dev does not exist"

**Solution**: Run the init command:

```bash
createdb -U postgres -h localhost toki_tui_dev
just init-tui-db
```

### PostgreSQL not running

**Solution**: Check if your PostgreSQL Docker container is running:

```bash
docker ps | grep postgres
```

If not, start it:

```bash
cd toki-api
./scripts/init_db.sh
```

### "sqlx-cli not found"

**Solution**: Install it:

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
```

### TUI not rendering properly on Windows

**Solution**: 
1. Use Windows Terminal instead of old console
2. Install a Nerd Font (JetBrains Mono, Cascadia Code)
3. Make sure PowerShell is version 7+ for best results

## Next Steps

### Phase 1: Current (Basic Timer) ✅

You are here! Basic timer start/stop with isolated dev database.

### Phase 2: Enhanced Features

- [ ] Project/activity selection
- [ ] Note editing
- [ ] Timer history view
- [ ] Save to Milltime

### Phase 3: Advanced Features

- [ ] Authentication (session reuse)
- [ ] Git integration
- [ ] Pomodoro mode
- [ ] Multiple timer views

### Phase 4: Production Ready

- [ ] Error handling improvements
- [ ] Configuration file support
- [ ] Help screen
- [ ] Windows installer
- [ ] Documentation

## Contributing

Since this is an experimental TUI for the Toki project:

1. Always use the dev database (`toki_tui_dev`)
2. Test on multiple platforms if possible
3. Keep the UI simple and keyboard-driven
4. Follow the existing code structure

## License

Same as the parent Toki project.

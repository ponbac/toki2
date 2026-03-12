# toki-tui

Terminal UI for Toki time tracking, built with [Ratatui](https://ratatui.rs).

## Running

```bash
# Authenticate (first time or after session expires)
just tui-login

# Run against the real toki-api
just tui

# Run in dev mode (no server needed, fake in-memory data)
just tui-dev

# Clear saved session
just tui-logout
```

## CLI Commands

All commands are available via the binary directly (`toki-tui <command>`) or through `just`:

| Command | `just` recipe | Description |
| -------------- | -------------- | -------------------------------------------------- |
| `run` | `just tui` | Run against the real toki-api server |
| `dev` | `just tui-dev` | Run in dev mode with in-memory mock data |
| `login` | `just tui-login` | Authenticate via browser OAuth |
| `logout` | `just tui-logout` | Clear saved session and Milltime cookies |
| `status` | `just tui-status` | Show current login and Milltime session status |
| `config-path` | `just tui-config` | Print config path; create default file if missing |
| `logs-path` | `just tui-logs` | Print the log notes directory path |
| `version` | `just tui-version` | Print the current version |

## Configuration

Config file: `~/.config/toki-tui/config.toml`

Run `just tui-config` (or `toki-tui config-path`) to print the path and create the file with defaults if it does not exist.

All keys are optional. If the file is missing, built-in defaults are used.

```toml
# URL of the toki-api server. Defaults to the production instance.
api_url = "https://toki-api.spinit.se"

# Prefix used when converting a git branch name to a time entry note,
# when no conventional commit prefix (feat/fix/etc.) or ticket number is found.
# Example: branch "branding/redesign" → "Development: branding/redesign"
git_default_prefix = "Utveckling"

# Taskwarrior filter tokens prepended before `status:pending export`.
# Leave empty to show all pending tasks.
# Example: "+work project:Toki"
task_filter = ""

# Whether to automatically resize the timer widget when the timer starts/stops.
# When true (default), the timer grows large when running and shrinks when stopped.
# Set to false to keep the timer at a fixed (normal) size at all times.
auto_resize_timer = true

# Entry templates — pre-fill project, activity and note from a picker (press T).
# [[template]] sections can be repeated.
[[template]]
name = "My project"
project = "My Project"
activity = "Development"
note = "Working on stuff"
```

### Entry templates

Define reusable presets in `config.toml`. In the timer view, press `T` to open the template picker and select one to pre-fill the current entry.

### Environment variables

Environment variables override values from `config.toml`.

- Prefix: `TOKI_TUI_`
- Key format: uppercase snake case

```bash
TOKI_TUI_API_URL="http://localhost:8080"
TOKI_TUI_GIT_DEFAULT_PREFIX="Development"
TOKI_TUI_TASK_FILTER="+work project:Toki"
TOKI_TUI_AUTO_RESIZE_TIMER=true
```

### Example: local dev setup

```toml
api_url = "http://localhost:8080"
git_default_prefix = "Development"
task_filter = "+work"
```

## Log notes

Attach a freeform markdown log file to any time entry. Log files are stored in `~/.local/share/toki-tui/logs/` and linked to entries via a tag embedded in the note (`[log:XXXXXX]`). The tag is hidden in all display locations — only the clean summary is shown.

Run `just tui-logs` (or `toki-tui logs-path`) to print the log directory path.

## Key bindings

### Timer view

| Key | Action |
| -------------------- | ----------------------------- |
| `Space` | Start / stop timer |
| `Ctrl+S` | Save (with options) |
| `Ctrl+R` | Resume last entry |
| `Ctrl+X` | Clear current entry |
| `Enter` | Edit description |
| `P` | Edit project / activity |
| `N` | Edit note (description editor) |
| `T` | Open template picker |
| `H` | Switch to history view |
| `S` | Switch to statistics view |
| `X` | Toggle timer size |
| `Z` | Zen mode (hide UI chrome) |
| `Tab / ↑↓ / j/k` | Navigate |
| `Q` | Quit |

### Description editor (note / `N`)

| Key | Action |
| -------------------- | ----------------------------- |
| `Ctrl+L` | Add / edit log file |
| `Ctrl+R` | Remove linked log file |
| `Ctrl+D` | Change working directory |
| `Ctrl+G` | Git: copy/paste branch or commit |
| `Ctrl+T` | Taskwarrior: pick a task |
| `Ctrl+X` | Clear note |
| `Ctrl+←/→` | Word-boundary navigation |
| `Ctrl+Backspace` | Delete word back |
| `Enter` | Confirm |
| `Esc` | Cancel |

### History view

| Key | Action |
| -------------------- | ----------------------------- |
| `↑↓` | Navigate entries |
| `Enter` | Edit entry |
| `Ctrl+R` | Resume entry (copy to timer) |
| `Ctrl+L` | Open linked log file |
| `H / Esc` | Back to timer view |
| `Q` | Quit |

**While editing a history entry:**

| Key | Action |
| -------------------- | ----------------------------- |
| `Tab` | Next field |
| `P / A` | Change project / activity |
| `Esc` | Save and exit edit mode |

## Testing

```bash
SQLX_OFFLINE=true cargo test -p toki-tui
```

Tests cover app and state behavior, text input helpers, runtime action handling, and focused Ratatui render assertions.

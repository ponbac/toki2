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

## Key bindings

| Key | Action |
|-----|--------|
| `Space` | Start / stop timer |
| `S` | Statistics view |
| `H` | History view |
| `P` | Select project |
| `A` | Select activity |
| `D` | Edit description |
| `E` | Edit entry (in history) |
| `Esc` | Back / cancel |
| `Q` | Quit |

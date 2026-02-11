# TUI Quick Reference

## 🚀 Quick Start
```bash
just tui                    # Launch TUI
just create-test-timers     # Add sample history data
just check-timers           # Check database
just reset-tui-db           # Clean slate
```

## ⌨️ Keyboard Shortcuts

### Timer View (Main)
| Key | Action |
|-----|--------|
| `Space` | Start/stop timer |
| `P` | Select project |
| `A` | Select activity |
| `H` or `Tab` | View history |
| `R` | Refresh from DB |
| `Q` | Quit |

### Selection Views (Projects/Activities)
| Key | Action |
|-----|--------|
| `↑` `↓` | Navigate |
| `Enter` | Select |
| `Esc` | Cancel |
| `Q` | Quit |

### History View
| Key | Action |
|-----|--------|
| `↑` `↓` | Scroll |
| `Tab` | Back to timer |
| `Q` | Quit |

## 📋 Available Projects

1. **Toki2 Development** `[TOKI-2]`
   - Backend Development
   - Frontend Development  
   - Bug Fixes
   - Code Review

2. **Azure DevOps Integration** `[ADO-INT]`
   - API Integration
   - Webhook Setup
   - Testing

3. **TUI Development** `[TUI]`
   - UI Design
   - Feature Implementation
   - Testing & Debugging

4. **Internal Tools** `[TOOLS]`
   - Development
   - Maintenance
   - Documentation

5. **Research & Learning**
   - Learning
   - Experimentation
   - Proof of Concept

## 🎯 Common Workflows

### Start Timer with Custom Project
1. Press `P` → Select project → `Enter`
2. Press `A` → Select activity → `Enter`
3. Press `Space` → Timer starts

### Check What You've Been Working On
1. Press `H` → See history
2. Use `↑↓` to scroll
3. Press `Tab` → Back to timer

### Change Project Mid-Day
1. Press `P` → Select new project
2. Press `A` → Select new activity
3. Press `Space` → Start new timer

## 🐛 Troubleshooting

| Problem | Solution |
|---------|----------|
| No active timer | Press `R` to refresh |
| History empty | Run `just create-test-timers` |
| Can't see selection | Use arrow keys (`↑↓`) |
| Wrong project | Press `P` to change |
| TUI crashes | Check terminal size (min 80x24) |
| DB error | Run `just init-tui-db` |

## 📊 Database Commands

```bash
# Check what's in the database
just check-timers

# Open database shell
just db-shell

# Manual query (active timer)
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev \
  -c "SELECT * FROM timer_history WHERE end_time IS NULL;"

# Manual query (history)
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev \
  -c "SELECT * FROM timer_history ORDER BY start_time DESC LIMIT 10;"
```

## 🎨 Visual Indicators

| Display | Meaning |
|---------|---------|
| `⏱ 01:23:45 (Running)` 🟢 | Timer is active |
| `⏱ 00:00:00 (Stopped)` 🟡 | Timer is stopped |
| `(not running)` | Selected but not started |
| **Yellow highlight** | Currently selected item |
| `[CODE]` | Project code |

## 📁 File Locations

```
toki2/
├── toki-tui/              # TUI source code
│   ├── src/
│   │   ├── test_data.rs   # Projects & activities
│   │   ├── app.rs         # Application state
│   │   ├── ui/mod.rs      # UI rendering
│   │   └── main.rs        # Entry point
│   └── .env.tui           # DB config
├── TEST_TUI_FEATURES.md   # Detailed testing guide
├── TUI_FEATURE_SUMMARY.md # Complete feature overview
└── TUI_VISUAL_GUIDE.md    # Screen mockups
```

## 🔗 Related Commands

```bash
# Backend development
just run                # Run main API
just check              # Check Rust builds
just db                 # Start production DB

# Frontend development  
just app                # Run frontend
just tsc                # TypeScript check

# TUI-specific
just tui                # Run TUI
just init-tui-db        # Setup TUI database
just reset-tui-db       # Reset TUI database
```

## ⚠️ Important Notes

- **Isolated database**: TUI uses `toki_tui_dev`, not production `toki`
- **Stop = Delete**: Stopping timer deletes it (no Milltime save yet)
- **Test data only**: Projects are hardcoded, not from real API
- **Single user**: Always uses user_id=1 (test user)

## 🎯 Next Steps

After trying the TUI:
1. ✅ Test project selection
2. ✅ Test activity selection  
3. ✅ Start and stop timers
4. ✅ View history
5. ✅ Navigate between views

Future features:
- Note editing
- Save to Milltime
- Real authentication
- Git integration

---

**Happy time tracking!** 🎉

Run `just tui` to get started.

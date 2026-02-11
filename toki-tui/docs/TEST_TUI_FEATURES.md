# TUI New Features - Testing Guide

## What's New? ✨

### 1. **Project & Activity Selection**
You can now select from 5 pre-configured projects:
- **Toki2 Development** [TOKI-2] - Backend, Frontend, Bug Fixes, Code Review
- **Azure DevOps Integration** [ADO-INT] - API Integration, Webhook Setup, Testing
- **TUI Development** [TUI] - UI Design, Feature Implementation, Testing
- **Internal Tools** [TOOLS] - Development, Maintenance, Documentation
- **Research & Learning** - Learning, Experimentation, Proof of Concept

### 2. **Timer History View**
See all your past timers with:
- Start date and time
- Duration (HH:MM format)
- Project and activity
- Notes

---

## How to Use 🎮

### Main Timer View
- **Space** - Start/stop timer with currently selected project/activity
- **P** - Select a project
- **A** - Select an activity (for currently selected project)
- **H** or **Tab** - View timer history
- **R** - Refresh timer from database
- **Q** - Quit

### Project Selection View
- **↑/↓** - Navigate through projects
- **Enter** - Select project (automatically returns to timer view)
- **Esc** - Cancel and return to timer
- **Q** - Quit

### Activity Selection View
- **↑/↓** - Navigate through activities
- **Enter** - Select activity (automatically returns to timer view)
- **Esc** - Cancel and return to timer
- **Q** - Quit

### History View
- **↑/↓** - Scroll through history
- **Tab** - Return to timer view
- **Q** - Quit

---

## Testing Steps 🧪

1. **Run the TUI**:
   ```bash
   just tui
   ```

2. **Select a Project**:
   - Press **P**
   - Use arrow keys to browse projects
   - Press **Enter** to select (e.g., "TUI Development")

3. **Select an Activity**:
   - Press **A**
   - Use arrow keys to browse activities
   - Press **Enter** to select (e.g., "Feature Implementation")

4. **Start a Timer**:
   - Press **Space**
   - Watch the timer count up!
   - Notice the project/activity displayed

5. **Stop the Timer**:
   - Press **Space** again
   - Timer stops and is deleted (since we're not saving to Milltime yet)

6. **View History** (after creating some timers):
   - Press **H** or **Tab**
   - See all your timer entries
   - Press **Tab** to return to timer view

7. **Change Project/Activity Mid-Work**:
   - While timer is running, press **P** or **A**
   - Select different project/activity
   - Start a new timer with the new selection

---

## Expected Behavior ✅

### When You Select a Project:
- Activities list updates to show only that project's activities
- First activity is auto-selected
- Status shows: "Selected project: [Project Name]"
- Returns to timer view automatically

### When You Select an Activity:
- Status shows: "Selected activity: [Activity Name]"
- Returns to timer view automatically

### When Timer is Not Running:
- Project info shows: "[Project] / [Activity] (not running)"
- This is your **default selection** for the next timer

### When Timer is Running:
- Project info shows: "[Project] / [Activity]"
- Elapsed time updates every second
- Timer persists in database (survives TUI restart)

### When Viewing History:
- Shows all timers (active and completed)
- Active timers show "Active" instead of duration
- Most recent entries appear first

---

## Architecture Notes 🏗️

### Test Data (toki-tui/src/test_data.rs)
- Hardcoded projects and activities
- In production, these would come from Milltime API
- Each project has 3-4 activities

### New Database Query
- `get_timer_history()` - Fetches last N timer entries
- Used when entering history view

### App State (toki-tui/src/app.rs)
- **Views**: Timer, History, SelectProject, SelectActivity
- **Navigation**: `navigate_to()`, `select_next()`, `select_previous()`, `confirm_selection()`
- **History**: `update_history()` stores fetched entries

### UI Rendering (toki-tui/src/ui/mod.rs)
- Separate render function for each view
- Dynamic controls based on current view
- Highlighted selection in project/activity lists

---

## Known Limitations ⚠️

1. **Timer deletion instead of save**: When you stop a timer, it's deleted from the database (not saved to Milltime)
   - This is temporary until we integrate Milltime API
   - Use refresh (R) to reload if you stop by accident

2. **No note editing**: Timer note is hardcoded to "Working on task"
   - Future enhancement: Add input dialog for notes

3. **No scrolling in history**: If you have >50 entries, older ones won't show
   - Can be improved with pagination

4. **Activities load on project selection**: When you select a project, activities refresh
   - This means activity selection resets to first item

---

## Future Enhancements 🚀

### Short Term:
- [ ] Note editing (input dialog)
- [ ] Save timer to Milltime instead of deleting
- [ ] Pagination for history view
- [ ] Visual indicator for active timer in history

### Medium Term:
- [ ] Git integration (auto-fill notes from commits)
- [ ] Timer editing (change project/activity/note after starting)
- [ ] Statistics (total time per project/activity)
- [ ] Export history to CSV

### Long Term:
- [ ] Real Milltime integration (fetch projects/activities from API)
- [ ] Authentication (session cookies from web app)
- [ ] Daemon mode with global hotkeys
- [ ] Cross-compilation for Windows colleagues

---

## Troubleshooting 🔧

### "No active timer" when you expect one:
- Press **R** to refresh from database
- Check database: `just check-timers`

### Project/activity selection not working:
- Make sure you're in Timer view (press Tab if in History)
- Try selecting project first (P), then activity (A)

### History is empty:
- You need to create and stop some timers first
- History only shows stopped timers (end_time set)
- Currently, stopping deletes the timer, so history won't populate

### TUI crashes or doesn't render:
- Make sure terminal is large enough (at least 80x24)
- Try running in full-screen terminal
- Check Docker is running: `docker ps`
- Check database connection: `just check-dbs`

---

## Database Verification 🗄️

### Check active timer:
```bash
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT * FROM timer_history WHERE end_time IS NULL;"
```

### Check all timers:
```bash
just check-timers
```

### Manually create a test timer:
```bash
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "INSERT INTO timer_history (user_id, start_time, project_name, activity_name, note) 
   VALUES (1, NOW() - INTERVAL '2 hours', 'Test Project', 'Testing', 'Manual test entry');"
```

### View history with SQL:
```bash
PGPASSWORD=password psql -U postgres -h localhost -d toki_tui_dev -c \
  "SELECT start_time, end_time, project_name, activity_name, note FROM timer_history ORDER BY start_time DESC LIMIT 10;"
```

---

## Success Criteria ✓

After testing, you should be able to:
- [x] Browse and select from 5 different projects
- [x] Browse and select activities specific to each project
- [x] Start a timer with selected project/activity
- [x] See timer counting up in real-time
- [x] Stop a timer
- [x] View timer history (after creating entries)
- [x] Navigate between views using keyboard shortcuts
- [x] See selected project/activity even when timer is stopped

**All features implemented and ready to test!** 🎉

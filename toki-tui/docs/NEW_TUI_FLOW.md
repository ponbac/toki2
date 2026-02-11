# NEW TUI Flow - Testing Guide

## 🎯 The New Workflow

### User Flow (As Requested)

1. **Open TUI**
   ```bash
   just tui
   ```
   - Timer shows: `Unknown / Unknown (not running)`
   - Description shows: `Doing something important...`

2. **Press Space to Start Timer**
   - Timer starts immediately with "Unknown" project/activity
   - Timer display: `⏱ 00:00:01 (Running)`
   - Project shows: `Unknown / Unknown`

3. **Press TAB to Select Project**
   - Project selection view appears
   - 5 projects listed with codes
   - Use ↑↓ to navigate
   - Press Enter to select

4. **Activity Selection Appears Automatically**
   - After selecting project, activity list shows immediately
   - Activities filtered for selected project
   - Use ↑↓ to navigate
   - Press Enter to select

5. **Press A to Edit Description**
   - Description editor opens
   - Type your description (single line)
   - Press Enter to confirm
   - Press Esc to cancel

---

## ⌨️ New Keyboard Shortcuts

### Main Timer View
| Key | Action |
|-----|--------|
| `Space` | Start/stop timer (works with "Unknown") |
| `Tab` | Select project (then auto-shows activities) |
| `A` | Edit description |
| `H` | View history |
| `Q` | Quit |

### Project Selection
| Key | Action |
|-----|--------|
| `↑↓` | Navigate projects |
| `Enter` | Select (auto-shows activities) |
| `Esc` | Cancel |

### Activity Selection
| Key | Action |
|-----|--------|
| `↑↓` | Navigate activities |
| `Enter` | Select |
| `Esc` | Cancel |

### Description Editor
| Key | Action |
|-----|--------|
| Type | Edit description |
| `Backspace` | Delete characters |
| `Enter` | Confirm |
| `Esc` | Cancel |

### History View
| Key | Action |
|-----|--------|
| `↑↓` | Scroll |
| `H` | Back to timer |
| `Esc` | Back to timer |
| `Q` | Quit |

---

## 🧪 Test Scenarios

### Scenario 1: Quick Start (Unknown Project)
1. Run `just tui`
2. Press **Space**
3. ✅ Timer starts with "Unknown / Unknown"
4. Watch it count up
5. Press **Space** to stop

### Scenario 2: Start then Change Project
1. Press **Space** (start timer)
2. Wait a few seconds
3. Press **Tab** (select project)
4. Choose "TUI Development" (↓ then Enter)
5. Choose "Feature Implementation" (Enter)
6. ✅ Timer continues with new project/activity

### Scenario 3: Set Project Before Starting
1. Press **Tab** (select project)
2. Choose "Toki2 Development" (↓ then Enter)
3. Choose "Backend Development" (↓ then Enter)
4. Press **Space** (start timer)
5. ✅ Timer starts with selected project

### Scenario 4: Edit Description While Running
1. Start timer (Space)
2. Press **A** (edit description)
3. Type: "Fixing timer flow"
4. Press **Enter**
5. ✅ Description updates in timer view

### Scenario 5: View History
1. Create some timers (start/stop a few times)
2. Press **H** (view history)
3. ✅ See list of past timers
4. Press **H** or **Esc** to return

---

## 📋 Expected Behavior

### Initial State
```
╔════════════════════════ Timer ═════════════════════════╗
║              ⏱  00:00:00 (Stopped)                     ║
╚════════════════════════════════════════════════════════╝

╔═══════════════ Project / Activity ═════════════════════╗
║            Unknown / Unknown (not running)             ║
╚════════════════════════════════════════════════════════╝

╔═══════════════════ Description ════════════════════════╗
║           Doing something important...                 ║
╚════════════════════════════════════════════════════════╝

╔════════════════════ Controls ══════════════════════════╗
║  Space: Start/Stop  Tab: Project  A: Description       ║
║              H: History  Q: Quit                       ║
╚════════════════════════════════════════════════════════╝
```

### After Starting (Space)
```
╔════════════════════════ Timer ═════════════════════════╗
║              ⏱  00:00:05 (Running) 🟢                  ║
╚════════════════════════════════════════════════════════╝

╔═══════════════ Project / Activity ═════════════════════╗
║                 Unknown / Unknown                      ║
╚════════════════════════════════════════════════════════╝
```

### After Selecting Project (Tab → Enter)
```
╔═══════════════════ Select Project ═════════════════════╗
║                                                        ║
║  [TOKI-2] Toki2 Development                            ║
║  [ADO-INT] Azure DevOps Integration                    ║
║  [TUI] TUI Development  ← (highlighted)                ║
║  [TOOLS] Internal Tools                                ║
║  Research & Learning                                   ║
║                                                        ║
╚════════════════════════════════════════════════════════╝

Press Enter → Activity list appears automatically
```

### After Selecting Activity (Auto-shown, then Enter)
```
╔═════════ Select Activity for TUI Development ══════════╗
║                                                        ║
║  UI Design                                             ║
║  Feature Implementation  ← (highlighted)               ║
║  Testing & Debugging                                   ║
║                                                        ║
╚════════════════════════════════════════════════════════╝

Press Enter → Returns to timer with new selection
```

### After Editing Description (A → Type → Enter)
```
╔═══════════════════ Description ════════════════════════╗
║              Implementing new timer flow               ║
╚════════════════════════════════════════════════════════╝
```

---

## 🎨 Key Improvements

### 1. **Simplified Start**
- Can start timer immediately without selecting anything
- Default "Unknown" project is fine for quick starts
- Project can be changed later

### 2. **Streamlined Project Selection**
- TAB instead of P (more natural)
- Activity selection appears automatically
- No need to remember separate activity shortcut

### 3. **Better Description Editing**
- A for "Annotate" (easier to remember)
- Single-line editor (cleaner UI)
- Default text matches web app behavior

### 4. **Consistent Navigation**
- H toggles history view
- Esc cancels any selection
- Q quits from anywhere

---

## 🔍 What Changed From Before

| Feature | Old Behavior | New Behavior |
|---------|-------------|--------------|
| **Start timer** | Required project/activity | Can start with "Unknown" |
| **Project selection** | P key | Tab key |
| **Activity selection** | A key | Automatic after project |
| **Description** | Called "Note", hardcoded | Called "Description", editable with A key |
| **History** | H or Tab | H only (Tab is project) |
| **Default text** | "Working on task" | "Doing something important..." |
| **Initial state** | Had default project | Starts with "Unknown" |

---

## 🐛 Testing Checklist

- [ ] Can start timer with Space (shows "Unknown")
- [ ] Timer counts up correctly
- [ ] Tab opens project selection
- [ ] Selecting project auto-shows activities
- [ ] Selecting activity returns to timer
- [ ] A opens description editor
- [ ] Can type and edit description
- [ ] Enter confirms description
- [ ] Description shows in timer view
- [ ] Description persists with timer
- [ ] H opens history view
- [ ] H or Esc closes history view
- [ ] Esc cancels selections
- [ ] Q quits from any view

---

## 💡 Usage Tips

1. **Quick tracking**: Just press Space to start. Fix project later with Tab.

2. **Organized tracking**: Press Tab first, select project/activity, then Space.

3. **Describe your work**: Use A to add meaningful descriptions like:
   - "Fixing bug #123"
   - "Implementing user authentication"
   - "Code review for PR #45"

4. **Check your time**: Press H anytime to see what you've been working on.

5. **Change projects**: Even while timer is running, press Tab to switch.

---

## 🎯 Success Criteria

✅ Timer starts with "Unknown" when no project selected  
✅ Tab key opens project selection  
✅ Activity list appears automatically after selecting project  
✅ A key opens description editor  
✅ Description defaults to "Doing something important..."  
✅ Single-line description editing works  
✅ H key toggles history view  
✅ All navigation flows work smoothly  

---

**Ready to test!** Run `just tui` and try the new flow. 🚀

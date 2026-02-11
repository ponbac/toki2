# TUI Fuzzy Project Selection Design

**Date:** 2025-02-11  
**Status:** Approved  
**Context:** Improving project selection UX in toki-tui with fuzzy finding

## Overview

Add fuzzy matching to project selection to improve UX when working with extensive project lists. Activity selection remains unchanged (simple list navigation) since activities are typically 2-8 items per project.

## User Experience Flow

### Project Selection with Fuzzy Matching

1. User presses `P` → opens project selection view
2. Search input is focused at top (auto-ready for typing)
3. As user types, project list filters in real-time with fuzzy matching
4. Results ranked by relevance score (closest matches at top)
5. Arrow keys navigate filtered results, Enter selects, Esc cancels
6. Selected project shows immediately in main view's Project/Activity box

### Activity Selection (Unchanged)

1. After project selection, user presses `A` or Tab → opens activity selection
2. Shows all activities for selected project (2-8 items, no filter needed)
3. Arrow keys navigate, Enter selects, Esc cancels

### Visual Design

**Search Input:**
- Box at top (3 lines) with border and title "Search:"
- Typing appears in white text with cursor indicator
- Empty search = show all projects (unfiltered)

**Results List:**
- Shows matched projects with format: `[CODE] Project Name` or just `Project Name`
- Selected item: bold + yellow
- Other items: white
- Header shows filtered count: "Projects (2/5)"

**Edge Cases:**
- Empty project list → "No projects available"
- No matches → "No matches found. Press Ctrl+U to clear."

## Technical Implementation

### Dependencies

Add to `toki-tui/Cargo.toml`:
```toml
fuzzy-matcher = "0.3"
```

Use `SkimMatcherV2` (fastest, most accurate algorithm).

### State Changes

Add to `App` struct in `app.rs`:

```rust
pub struct App {
    // ... existing fields ...
    
    // New fields for fuzzy finding
    pub project_search_input: String,       // User's search query
    pub filtered_projects: Vec<TestProject>, // Filtered and ranked results
    pub filtered_project_index: usize,       // Selection in filtered list
}
```

Initialize in `App::new()`:
```rust
project_search_input: String::new(),
filtered_projects: projects.clone(),  // Start with all projects
filtered_project_index: 0,
```

### Fuzzy Matching Logic

**On every keystroke in search input:**
1. Use `matcher.fuzzy_match(&project.name, &search_query)` → returns `Option<i64>` score
2. Filter: keep only projects with `Some(score)`
3. Sort by score descending (highest matches first)
4. Store in `filtered_projects`
5. Reset `filtered_project_index` to 0

**Empty search:**
- Copy all projects to `filtered_projects` (no filtering)

### Input Handling

When in `View::SelectProject`:

| Key | Action |
|-----|--------|
| Char input | Append to `project_search_input`, re-filter |
| Backspace | Remove last char from input, re-filter |
| Ctrl+U | Clear entire search input, show all projects |
| Arrow keys / j/k | Navigate `filtered_project_index` |
| Enter | Select `filtered_projects[filtered_project_index]` |
| Esc | Return to Timer view without selection |

### UI Rendering

**Layout:**
```
┌─ Search ──────────────────┐
│ development_              │  ← User input with cursor
└───────────────────────────┘
┌─ Projects (2/5) ──────────┐  ← Shows filtered count / total
│ TUI Development           │  ← Bold + Yellow (selected)
│ Toki2 Development         │  ← White
└───────────────────────────┘
┌─ Controls ────────────────┐
│ Type: Filter  ↑↓: Navigate│
│ Enter: Select  Esc: Cancel│
└───────────────────────────┘
```

**Rendering Details:**
- Search box: 3 lines, borders, title "Search:"
- Projects list: dynamic height, title "Projects (X/Y)" where X=filtered, Y=total
- Controls: 3 lines, centered text with yellow highlights for keys
- Selected item: bold + yellow
- Other items: white text

**Character Highlighting (Optional v2 feature):**
- Highlight matched characters in results
- Example: searching "dev" → "**Dev**elopment"
- `fuzzy-matcher` returns match indices for rendering
- Can skip for v1 - still functional without it

## Performance Considerations

- Re-filtering on every keystroke is acceptable for <100 projects
- If lag occurs with large lists, add debouncing (unlikely needed)
- `SkimMatcherV2` is optimized for interactive use

## Implementation Steps

1. Add `fuzzy-matcher` dependency to `Cargo.toml`
2. Update `App` struct with new fields in `app.rs`
3. Add filtering logic function in `app.rs`: `filter_projects(&self) -> Vec<TestProject>`
4. Update `render_project_selection()` in `ui/mod.rs` to show search input and filtered results
5. Update input handling in `main.rs` for `View::SelectProject` state
6. Test with various search queries and edge cases
7. Update main view controls to document new search capability

## Success Criteria

- Typing in project selection instantly filters results
- Fuzzy matching handles typos and out-of-order chars ("tudev" matches "TUI Development")
- Results ranked by relevance (best matches first)
- Empty search shows all projects
- No performance lag with <100 projects
- Activity selection unchanged (simple list navigation)

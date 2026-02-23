use crate::api::database::TimerHistoryEntry;
use crate::test_data::{get_test_activities, get_test_projects, TestActivity, TestProject};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use time::{OffsetDateTime, UtcOffset};

fn to_local_time(dt: OffsetDateTime) -> OffsetDateTime {
    if let Ok(local_offset) = UtcOffset::current_local_offset() {
        dt.to_offset(local_offset)
    } else {
        dt
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerState {
    Stopped,
    Running,
}

/// Default scheduled hours per week (no server API available)
pub const SCHEDULED_HOURS_PER_WEEK: f64 = 40.0;

/// Per-project/activity breakdown for the statistics view
#[derive(Debug, Clone)]
pub struct ProjectStat {
    pub label: String, // "Project - Activity"
    pub hours: f64,
    pub percentage: f64, // 0.0–100.0 of total worked this week
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Timer,
    History,
    SelectProject,
    SelectActivity,
    EditDescription,
    SaveAction,
    Statistics,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaveAction {
    ContinueSameProject,
    ContinueNewProject,
    SaveAndStop,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedBox {
    Timer,
    ProjectActivity,
    Description,
    Today,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerSize {
    Normal,
    Large,
}

#[derive(Debug, Clone)]
pub struct GitContext {
    pub cwd: std::path::PathBuf,
    pub branch: Option<String>,
    pub last_commit: Option<String>,
}

impl GitContext {
    pub fn from_cwd(cwd: std::path::PathBuf) -> Self {
        let branch = Self::git_branch(&cwd);
        let last_commit = Self::git_last_commit(&cwd);
        Self {
            cwd,
            branch,
            last_commit,
        }
    }

    fn git_branch(cwd: &std::path::Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["-C", cwd.to_str()?, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8(output.stdout).ok()?.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        } else {
            None
        }
    }

    fn git_last_commit(cwd: &std::path::Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["-C", cwd.to_str()?, "log", "-1", "--format=%s"])
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8(output.stdout).ok()?.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        } else {
            None
        }
    }

    pub fn refresh(&mut self) {
        self.branch = Self::git_branch(&self.cwd);
        self.last_commit = Self::git_last_commit(&self.cwd);
    }
}

/// A single Taskwarrior task entry.
#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub id: u32,
    pub description: String,
}

/// State for the Taskwarrior task-picker overlay.
#[derive(Debug, Clone, Default)]
pub struct TaskwarriorOverlay {
    pub tasks: Vec<TaskEntry>,
    pub selected: Option<usize>,
    pub error: Option<String>,
}

/// A text input with mid-string cursor support.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextInput {
    pub value: String,
    pub cursor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_str(s: &str) -> Self {
        Self {
            value: s.to_string(),
            cursor: s.len(),
        }
    }

    /// Insert a character at the cursor position.
    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the character immediately before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Step back one char boundary
        let new_cursor = self.prev_boundary(self.cursor);
        self.value.drain(new_cursor..self.cursor);
        self.cursor = new_cursor;
    }

    /// Move cursor one char to the left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_boundary(self.cursor);
        }
    }

    /// Move cursor one char to the right.
    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor = self.next_boundary(self.cursor);
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }
    pub fn end(&mut self) {
        self.cursor = self.value.len();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Returns the string split at the cursor: (before, after).
    /// Use for rendering: `before + "█" + after`.
    pub fn split_at_cursor(&self) -> (&str, &str) {
        (&self.value[..self.cursor], &self.value[self.cursor..])
    }

    // --- helpers ---
    fn prev_boundary(&self, pos: usize) -> usize {
        debug_assert!(pos > 0, "prev_boundary called with pos == 0");
        let mut p = pos;
        loop {
            p -= 1;
            if self.value.is_char_boundary(p) {
                return p;
            }
        }
    }
    fn next_boundary(&self, pos: usize) -> usize {
        debug_assert!(
            pos < self.value.len(),
            "next_boundary called at end of string"
        );
        let mut p = pos + 1;
        while p <= self.value.len() && !self.value.is_char_boundary(p) {
            p += 1;
        }
        p
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntryEditField {
    StartTime,
    EndTime,
    Project,
    Activity,
    Note,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryEditState {
    pub entry_id: i32,
    pub start_time_input: String,
    pub end_time_input: String,
    pub original_start_time: String,
    pub original_end_time: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: TextInput,
    pub focused_field: EntryEditField,
    pub validation_error: Option<String>,
}

pub struct App {
    pub running: bool,
    pub timer_state: TimerState,
    pub absolute_start: Option<OffsetDateTime>, // UTC time when timer started
    pub local_start: Option<Instant>,           // For UI duration display
    pub user_id: i32,
    pub status_message: Option<String>,
    pub current_view: View,
    pub focused_box: FocusedBox,
    pub timer_size: TimerSize,

    // Timer history
    pub timer_history: Vec<TimerHistoryEntry>,
    pub history_scroll: usize,
    pub overlapping_entry_ids: HashSet<i32>, // Entry IDs that have overlapping times

    // Project/Activity selection
    pub projects: Vec<TestProject>,
    pub activities: Vec<TestActivity>,
    pub selected_project_index: usize,
    pub selected_activity_index: usize,
    pub selected_project: Option<TestProject>,
    pub selected_activity: Option<TestActivity>,

    // Fuzzy finding for projects
    pub project_search_input: TextInput,
    pub filtered_projects: Vec<TestProject>,
    pub filtered_project_index: usize,

    // Fuzzy finding for activities
    pub activity_search_input: TextInput,
    pub filtered_activities: Vec<TestActivity>,
    pub filtered_activity_index: usize,

    // Whether focus is on the result list (vs the search input) in selection views
    pub selection_list_focused: bool,

    // Save action selection
    pub selected_save_action: SaveAction,

    // Description editing
    pub description_input: TextInput,
    pub editing_description: bool,
    pub description_is_default: bool,
    pub saved_timer_note: Option<String>, // Saved when editing entry note to restore later

    // Today box navigation (This Week view)
    pub focused_this_week_index: Option<usize>,
    pub this_week_edit_state: Option<EntryEditState>,

    // History view navigation and editing
    pub focused_history_index: Option<usize>,
    pub history_edit_state: Option<EntryEditState>,
    pub history_list_entries: Vec<usize>, // Indices into timer_history for entries (excludes date separators)

    // Git context for note editor
    pub git_context: GitContext,
    pub git_mode: bool,
    pub cwd_input: Option<TextInput>, // Some(_) when changing directory
    pub cwd_completions: Vec<String>, // Tab completion candidates
    pub taskwarrior_overlay: Option<TaskwarriorOverlay>,
}

impl App {
    pub fn new(user_id: i32) -> Self {
        let projects = get_test_projects();

        Self {
            running: true,
            timer_state: TimerState::Stopped,
            absolute_start: None,
            local_start: None,
            user_id,
            status_message: None,
            current_view: View::Timer,
            focused_box: FocusedBox::Timer,
            timer_size: TimerSize::Normal,
            timer_history: Vec::new(),
            history_scroll: 0,
            overlapping_entry_ids: HashSet::new(),
            projects: projects.clone(),
            activities: Vec::new(),
            selected_project_index: 0,
            selected_activity_index: 0,
            selected_project: None,
            selected_activity: None,
            project_search_input: TextInput::new(),
            filtered_projects: projects.clone(),
            filtered_project_index: 0,
            activity_search_input: TextInput::new(),
            filtered_activities: Vec::new(),
            filtered_activity_index: 0,
            selection_list_focused: false,
            selected_save_action: SaveAction::ContinueSameProject,
            description_input: TextInput::new(),
            editing_description: false,
            description_is_default: true,
            saved_timer_note: None,
            focused_this_week_index: None,
            this_week_edit_state: None,
            focused_history_index: None,
            history_edit_state: None,
            history_list_entries: Vec::new(),
            git_context: GitContext::from_cwd(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            git_mode: false,
            cwd_input: None,
            cwd_completions: Vec::new(),
            taskwarrior_overlay: None,
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Toggle timer size between Normal and Large
    pub fn toggle_timer_size(&mut self) {
        self.timer_size = match self.timer_size {
            TimerSize::Normal => TimerSize::Large,
            TimerSize::Large => TimerSize::Normal,
        };
    }

    /// Clear timer and reset to default state
    pub fn clear_timer(&mut self) {
        self.timer_state = TimerState::Stopped;
        self.absolute_start = None;
        self.local_start = None;
        self.selected_project = None;
        self.selected_activity = None;
        self.description_input = TextInput::new();
        self.description_is_default = true;
        self.status_message = Some("Timer cleared".to_string());
    }

    /// Start a new timer
    pub fn start_timer(&mut self) {
        self.timer_state = TimerState::Running;
        self.absolute_start = Some(OffsetDateTime::now_utc());
        self.local_start = Some(Instant::now());
    }

    /// Stop the timer (without saving)
    pub fn stop_timer(&mut self) {
        self.timer_state = TimerState::Stopped;
        self.absolute_start = None;
        self.local_start = None;
    }

    pub fn set_status(&mut self, message: String) {
        self.status_message = Some(message);
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Get the elapsed time for the current timer
    pub fn elapsed_duration(&self) -> Duration {
        match self.timer_state {
            TimerState::Stopped => Duration::ZERO,
            TimerState::Running => self
                .local_start
                .map(|start| start.elapsed())
                .unwrap_or_default(),
        }
    }

    /// Format elapsed time as HH:MM:SS
    pub fn format_elapsed(&self) -> String {
        let duration = self.elapsed_duration();
        let hours = duration.as_secs() / 3600;
        let minutes = (duration.as_secs() % 3600) / 60;
        let seconds = duration.as_secs() % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }

    /// Update timer history
    pub fn update_history(&mut self, history: Vec<TimerHistoryEntry>) {
        self.timer_history = history;
        self.history_scroll = 0;
        self.compute_overlaps();
    }

    /// Compute overlapping time entries per day
    /// Two entries overlap if they share any time (excluding adjacent entries at minute granularity)
    fn compute_overlaps(&mut self) {
        self.overlapping_entry_ids.clear();

        use std::collections::HashMap;
        let mut entries_by_date: HashMap<time::Date, Vec<&TimerHistoryEntry>> = HashMap::new();

        // Group entries by date
        for entry in &self.timer_history {
            let date = entry.start_time.date();
            entries_by_date.entry(date).or_default().push(entry);
        }

        // Check overlaps within each day
        for (_, day_entries) in entries_by_date {
            if day_entries.len() < 2 {
                continue;
            }

            // Build intervals with start/end times in minutes since midnight
            let mut intervals: Vec<(i32, i64, i64)> = day_entries
                .iter()
                .filter_map(|entry| {
                    let end = entry.end_time?;
                    let start_mins = entry.start_time.time().hour() as i64 * 60
                        + entry.start_time.time().minute() as i64;
                    let end_mins = end.time().hour() as i64 * 60 + end.time().minute() as i64;
                    Some((entry.id, start_mins, end_mins))
                })
                .collect();

            // Sort by start time
            intervals.sort_by_key(|(_, start, _)| *start);

            // Find overlaps
            for (i, (_, _, curr_end)) in intervals.iter().enumerate() {
                for (_, next_start, _) in intervals.iter().skip(i + 1) {
                    if *next_start < *curr_end {
                        // Overlap found (entries that are adjacent at minute granularity don't count)
                        self.overlapping_entry_ids.insert(intervals[i].0);
                        // Also mark the overlapping entry
                        if let Some((id, _, _)) = intervals
                            .iter()
                            .skip(i + 1)
                            .find(|(_, s, _)| *s < *curr_end)
                        {
                            self.overlapping_entry_ids.insert(*id);
                        }
                    } else {
                        break; // No more overlaps possible for this interval
                    }
                }
            }
        }
    }

    /// Check if an entry has overlapping times
    pub fn is_entry_overlapping(&self, entry_id: i32) -> bool {
        self.overlapping_entry_ids.contains(&entry_id)
    }

    /// Navigate to a different view
    pub fn navigate_to(&mut self, view: View) {
        self.current_view = view;
        self.clear_status();

        // Reset selections when entering selection views
        match view {
            View::SelectProject => {
                self.selected_project_index = self
                    .projects
                    .iter()
                    .position(|p| self.selected_project.as_ref().map(|sp| &sp.id) == Some(&p.id))
                    .unwrap_or(0);
                // Reset search input and show all projects
                self.project_search_input.clear();
                self.filtered_projects = self.projects.clone();
                self.filtered_project_index = 0;
                self.selection_list_focused = false;
            }
            View::SelectActivity => {
                self.selected_activity_index = self
                    .activities
                    .iter()
                    .position(|a| self.selected_activity.as_ref().map(|sa| &sa.id) == Some(&a.id))
                    .unwrap_or(0);
                self.selection_list_focused = false;
            }
            View::EditDescription => {
                // If in edit mode, don't clear - the view handler will set it from edit_state
                if self.description_is_default
                    && self.this_week_edit_state.is_none()
                    && self.history_edit_state.is_none()
                {
                    self.description_input.clear();
                    self.description_is_default = false;
                }
                self.editing_description = true;
            }
            View::Timer => {
                self.editing_description = false;
                self.focused_box = FocusedBox::Timer; // Reset focus
            }
            _ => {}
        }
    }

    /// Move focus to next box (vim-style j or down)
    pub fn focus_next(&mut self) {
        self.focused_box = match self.focused_box {
            FocusedBox::Timer => FocusedBox::ProjectActivity,
            FocusedBox::ProjectActivity => FocusedBox::Description,
            FocusedBox::Description => FocusedBox::Today,
            FocusedBox::Today => FocusedBox::Timer,
        };
    }

    /// Move focus to previous box (vim-style k or up)
    pub fn focus_previous(&mut self) {
        self.focused_box = match self.focused_box {
            FocusedBox::Timer => FocusedBox::Today,
            FocusedBox::ProjectActivity => FocusedBox::Timer,
            FocusedBox::Description => FocusedBox::ProjectActivity,
            FocusedBox::Today => FocusedBox::Description,
        };
    }

    /// Handle Enter key on focused box
    pub fn activate_focused_box(&mut self) {
        match self.focused_box {
            FocusedBox::Timer => {
                // Toggle timer - handled in main.rs
            }
            FocusedBox::ProjectActivity => {
                self.navigate_to(View::SelectProject);
            }
            FocusedBox::Description => {
                self.navigate_to(View::EditDescription);
            }
            FocusedBox::Today => {
                self.enter_this_week_edit_mode();
            }
        }
    }

    /// Build the history list entries (indices into timer_history)
    /// This should be called when entering History view or after updates
    pub fn rebuild_history_list(&mut self) {
        let month_ago = OffsetDateTime::now_utc() - time::Duration::days(30);
        self.history_list_entries = self
            .timer_history
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.start_time >= month_ago)
            .map(|(idx, _)| idx)
            .collect();
    }

    /// Move focus up in History view
    pub fn history_focus_up(&mut self) {
        if self.history_list_entries.is_empty() {
            return;
        }

        if let Some(idx) = self.focused_history_index {
            if idx > 0 {
                self.focused_history_index = Some(idx - 1);
            }
        } else {
            self.focused_history_index = Some(self.history_list_entries.len() - 1);
        }
    }

    /// Move focus down in History view
    pub fn history_focus_down(&mut self) {
        if self.history_list_entries.is_empty() {
            return;
        }

        if let Some(idx) = self.focused_history_index {
            if idx < self.history_list_entries.len() - 1 {
                self.focused_history_index = Some(idx + 1);
            }
        } else {
            self.focused_history_index = Some(0);
        }
    }

    /// Move focus up in This Week box, wrap to Description if at top
    pub fn this_week_focus_up(&mut self) {
        let this_week_count = self.this_week_history().len();
        if this_week_count == 0 {
            self.focused_box = FocusedBox::Description;
            self.focused_this_week_index = None;
            return;
        }

        if let Some(idx) = self.focused_this_week_index {
            if idx == 0 {
                self.focused_box = FocusedBox::Description;
                self.focused_this_week_index = None;
            } else {
                self.focused_this_week_index = Some(idx - 1);
            }
        } else {
            self.focused_this_week_index = Some(this_week_count - 1);
        }
    }

    /// Move focus down in This Week box, wrap to Timer if at bottom
    pub fn this_week_focus_down(&mut self) {
        let this_week_count = self.this_week_history().len();
        if this_week_count == 0 {
            self.focused_box = FocusedBox::Timer;
            self.focused_this_week_index = None;
            return;
        }

        if let Some(idx) = self.focused_this_week_index {
            if idx >= this_week_count - 1 {
                self.focused_box = FocusedBox::Timer;
                self.focused_this_week_index = None;
            } else {
                self.focused_this_week_index = Some(idx + 1);
            }
        } else {
            self.focused_this_week_index = Some(0);
        }
    }

    /// Enter edit mode for the currently focused This Week entry
    pub fn enter_this_week_edit_mode(&mut self) {
        if let Some(idx) = self.focused_this_week_index {
            // Clone the entry data we need to avoid borrow issues
            let entry_data = self.this_week_history().get(idx).map(|e| {
                (
                    e.id,
                    e.start_time,
                    e.end_time,
                    e.project_id.clone(),
                    e.project_name.clone(),
                    e.activity_id.clone(),
                    e.activity_name.clone(),
                    e.note.clone(),
                )
            });

            if let Some((
                id,
                start_time,
                end_time,
                project_id,
                project_name,
                activity_id,
                activity_name,
                note,
            )) = entry_data
            {
                self.create_edit_state(
                    id,
                    start_time,
                    end_time,
                    project_id,
                    project_name,
                    activity_id,
                    activity_name,
                    note,
                );
            }
        }
    }

    /// Enter edit mode for the currently focused History entry
    pub fn enter_history_edit_mode(&mut self) {
        if let Some(list_idx) = self.focused_history_index {
            if let Some(&history_idx) = self.history_list_entries.get(list_idx) {
                // Clone the entry data we need
                let entry_data = self.timer_history.get(history_idx).map(|e| {
                    (
                        e.id,
                        e.start_time,
                        e.end_time,
                        e.project_id.clone(),
                        e.project_name.clone(),
                        e.activity_id.clone(),
                        e.activity_name.clone(),
                        e.note.clone(),
                    )
                });

                if let Some((
                    id,
                    start_time,
                    end_time,
                    project_id,
                    project_name,
                    activity_id,
                    activity_name,
                    note,
                )) = entry_data
                {
                    self.create_edit_state(
                        id,
                        start_time,
                        end_time,
                        project_id,
                        project_name,
                        activity_id,
                        activity_name,
                        note,
                    );
                }
            }
        }
    }

    /// Create edit state from entry data
    fn create_edit_state(
        &mut self,
        entry_id: i32,
        start_time: OffsetDateTime,
        end_time: Option<OffsetDateTime>,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) {
        let start_t = to_local_time(start_time).time();
        let start_str = format!("{:02}:{:02}", start_t.hour(), start_t.minute());

        let end_str = end_time
            .map(|et| {
                let t = to_local_time(et).time();
                format!("{:02}:{:02}", t.hour(), t.minute())
            })
            .unwrap_or_else(|| "00:00".to_string());

        let edit_state = EntryEditState {
            entry_id,
            start_time_input: start_str.clone(),
            end_time_input: end_str.clone(),
            original_start_time: start_str,
            original_end_time: end_str,
            project_id,
            project_name,
            activity_id,
            activity_name,
            note: TextInput::from_str(&note.unwrap_or_default()),
            focused_field: EntryEditField::StartTime,
            validation_error: None,
        };

        // Set the appropriate edit state based on current view
        if self.current_view == View::History {
            self.history_edit_state = Some(edit_state);
        } else {
            self.this_week_edit_state = Some(edit_state);
        }
    }

    /// Exit edit mode for This Week view
    pub fn exit_this_week_edit_mode(&mut self) {
        self.this_week_edit_state = None;
    }

    /// Exit edit mode for History view
    pub fn exit_history_edit_mode(&mut self) {
        self.history_edit_state = None;
    }

    /// Get mutable reference to current edit state (based on current view)
    pub fn current_edit_state(&mut self) -> Option<&mut EntryEditState> {
        if self.current_view == View::History {
            self.history_edit_state.as_mut()
        } else {
            self.this_week_edit_state.as_mut()
        }
    }

    /// Move to next field in edit mode
    pub fn entry_edit_next_field(&mut self) {
        // Update whichever edit state is active (this_week or history)
        if let Some(state) = &mut self.this_week_edit_state {
            state.focused_field = match state.focused_field {
                EntryEditField::StartTime => EntryEditField::EndTime,
                EntryEditField::EndTime => EntryEditField::Project,
                EntryEditField::Project => EntryEditField::Activity,
                EntryEditField::Activity => EntryEditField::Note,
                EntryEditField::Note => EntryEditField::StartTime,
            };
            state.validation_error = None;
        }
        if let Some(state) = &mut self.history_edit_state {
            state.focused_field = match state.focused_field {
                EntryEditField::StartTime => EntryEditField::EndTime,
                EntryEditField::EndTime => EntryEditField::Project,
                EntryEditField::Project => EntryEditField::Activity,
                EntryEditField::Activity => EntryEditField::Note,
                EntryEditField::Note => EntryEditField::StartTime,
            };
            state.validation_error = None;
        }
    }

    /// Move to previous field in edit mode
    pub fn entry_edit_prev_field(&mut self) {
        // Update whichever edit state is active (this_week or history)
        if let Some(state) = &mut self.this_week_edit_state {
            state.focused_field = match state.focused_field {
                EntryEditField::StartTime => EntryEditField::Note,
                EntryEditField::EndTime => EntryEditField::StartTime,
                EntryEditField::Project => EntryEditField::EndTime,
                EntryEditField::Activity => EntryEditField::Project,
                EntryEditField::Note => EntryEditField::Activity,
            };
            state.validation_error = None;
        }
        if let Some(state) = &mut self.history_edit_state {
            state.focused_field = match state.focused_field {
                EntryEditField::StartTime => EntryEditField::Note,
                EntryEditField::EndTime => EntryEditField::StartTime,
                EntryEditField::Project => EntryEditField::EndTime,
                EntryEditField::Activity => EntryEditField::Project,
                EntryEditField::Note => EntryEditField::Activity,
            };
            state.validation_error = None;
        }
    }

    /// Set the focused field in edit mode
    pub fn entry_edit_set_focused_field(&mut self, field: EntryEditField) {
        // Update whichever edit state is active (this_week or history)
        if let Some(state) = &mut self.this_week_edit_state {
            state.focused_field = field.clone();
            state.validation_error = None;
        }
        if let Some(state) = &mut self.history_edit_state {
            state.focused_field = field;
            state.validation_error = None;
        }
    }

    /// Handle character input in edit mode
    pub fn entry_edit_input_char(&mut self, c: char) {
        let apply_input = |state: &mut EntryEditState| match state.focused_field {
            EntryEditField::StartTime => {
                if state.start_time_input.len() >= 5 {
                    state.start_time_input.clear();
                }
                if c.is_ascii_digit() {
                    if state.start_time_input.is_empty() {
                        if c >= '3' && c <= '9' {
                            state.start_time_input.push('0');
                            state.start_time_input.push(c);
                            state.start_time_input.push(':');
                        } else {
                            state.start_time_input.push(c);
                        }
                    } else {
                        state.start_time_input.push(c);
                        if state.start_time_input.len() == 2 {
                            state.start_time_input.push(':');
                        }
                    }
                }
            }
            EntryEditField::EndTime => {
                if state.end_time_input.len() >= 5 {
                    state.end_time_input.clear();
                }
                if c.is_ascii_digit() {
                    if state.end_time_input.is_empty() {
                        if c >= '3' && c <= '9' {
                            state.end_time_input.push('0');
                            state.end_time_input.push(c);
                            state.end_time_input.push(':');
                        } else {
                            state.end_time_input.push(c);
                        }
                    } else {
                        state.end_time_input.push(c);
                        if state.end_time_input.len() == 2 {
                            state.end_time_input.push(':');
                        }
                    }
                }
            }
            EntryEditField::Note => {
                state.note.insert(c);
            }
            EntryEditField::Project | EntryEditField::Activity => {}
        };

        if let Some(state) = &mut self.this_week_edit_state {
            apply_input(state);
        }
        if let Some(state) = &mut self.history_edit_state {
            apply_input(state);
        }
    }

    /// Handle backspace in edit mode
    pub fn entry_edit_backspace(&mut self) {
        let apply_backspace = |state: &mut EntryEditState| match state.focused_field {
            EntryEditField::StartTime => {
                if state.start_time_input.ends_with(':') {
                    state.start_time_input.pop();
                }
                state.start_time_input.pop();
            }
            EntryEditField::EndTime => {
                if state.end_time_input.ends_with(':') {
                    state.end_time_input.pop();
                }
                state.end_time_input.pop();
            }
            EntryEditField::Note => {
                state.note.backspace();
            }
            EntryEditField::Project | EntryEditField::Activity => {}
        };

        if let Some(state) = &mut self.this_week_edit_state {
            apply_backspace(state);
        }
        if let Some(state) = &mut self.history_edit_state {
            apply_backspace(state);
        }
    }

    /// Move cursor left/right in a text field (Note only; time fields have no cursor).
    pub fn entry_edit_move_cursor(&mut self, left: bool) {
        let apply = |state: &mut EntryEditState| {
            if state.focused_field == EntryEditField::Note {
                if left {
                    state.note.move_left();
                } else {
                    state.note.move_right();
                }
            }
        };
        if let Some(s) = &mut self.this_week_edit_state {
            apply(s);
        }
        if let Some(s) = &mut self.history_edit_state {
            apply(s);
        }
    }

    pub fn entry_edit_cursor_home_end(&mut self, home: bool) {
        let apply = |state: &mut EntryEditState| {
            if state.focused_field == EntryEditField::Note {
                if home {
                    state.note.home();
                } else {
                    state.note.end();
                }
            }
        };
        if let Some(s) = &mut self.this_week_edit_state {
            apply(s);
        }
        if let Some(s) = &mut self.history_edit_state {
            apply(s);
        }
    }

    /// Clear the current time field for direct re-entry
    pub fn entry_edit_clear_time(&mut self) {
        if let Some(state) = &mut self.this_week_edit_state {
            match state.focused_field {
                EntryEditField::StartTime => {
                    state.start_time_input.clear();
                }
                EntryEditField::EndTime => {
                    state.end_time_input.clear();
                }
                _ => {}
            }
        }
        if let Some(state) = &mut self.history_edit_state {
            match state.focused_field {
                EntryEditField::StartTime => {
                    state.start_time_input.clear();
                }
                EntryEditField::EndTime => {
                    state.end_time_input.clear();
                }
                _ => {}
            }
        }
    }

    /// Check if a time string is valid HH:MM format
    fn is_valid_time_format(time_str: &str) -> bool {
        if time_str.len() != 5 || time_str.chars().nth(2) != Some(':') {
            return false;
        }
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 2 {
            return false;
        }
        if let (Ok(hours), Ok(mins)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            hours <= 23 && mins <= 59
        } else {
            false
        }
    }

    /// Revert invalid time inputs to original values
    pub fn entry_edit_revert_invalid_times(&mut self) {
        if let Some(state) = &mut self.this_week_edit_state {
            if !Self::is_valid_time_format(&state.start_time_input) {
                state.start_time_input = state.original_start_time.clone();
            }
            if !Self::is_valid_time_format(&state.end_time_input) {
                state.end_time_input = state.original_end_time.clone();
            }
        }
        if let Some(state) = &mut self.history_edit_state {
            if !Self::is_valid_time_format(&state.start_time_input) {
                state.start_time_input = state.original_start_time.clone();
            }
            if !Self::is_valid_time_format(&state.end_time_input) {
                state.end_time_input = state.original_end_time.clone();
            }
        }
    }

    /// Validate edit state and return error if invalid
    pub fn entry_edit_validate(&self) -> Option<String> {
        // Check whichever edit state is active
        let state = if let Some(s) = self.this_week_edit_state.as_ref() {
            s
        } else if let Some(s) = self.history_edit_state.as_ref() {
            s
        } else {
            return None;
        };

        // For empty times, just use default (00:00) - not an error
        let start_time = if state.start_time_input.is_empty() {
            "00:00"
        } else {
            &state.start_time_input
        };

        let end_time = if state.end_time_input.is_empty() {
            "00:00"
        } else {
            &state.end_time_input
        };

        // Validate start time format (HH:MM)
        if start_time.len() != 5 || start_time.chars().nth(2) != Some(':') {
            return Some("Invalid start time format (use HH:MM)".to_string());
        }

        // Validate end time format (HH:MM)
        if end_time.len() != 5 || end_time.chars().nth(2) != Some(':') {
            return Some("Invalid end time format (use HH:MM)".to_string());
        }

        // Parse times
        let start_parts: Vec<&str> = start_time.split(':').collect();
        let end_parts: Vec<&str> = end_time.split(':').collect();

        let start_hours: u32 = start_parts[0].parse().unwrap_or(0);
        let start_mins: u32 = start_parts[1].parse().unwrap_or(0);
        let end_hours: u32 = end_parts[0].parse().unwrap_or(0);
        let end_mins: u32 = end_parts[1].parse().unwrap_or(0);

        if start_hours > 23 || start_mins > 59 {
            return Some("Invalid start time (hours 0-23, mins 0-59)".to_string());
        }
        if end_hours > 23 || end_mins > 59 {
            return Some("Invalid end time (hours 0-23, mins 0-59)".to_string());
        }

        // Check end >= start
        let start_total_mins = start_hours * 60 + start_mins;
        let end_total_mins = end_hours * 60 + end_mins;

        if end_total_mins < start_total_mins {
            return Some("End time must be after start time".to_string());
        }

        None
    }

    /// Get the entry ID for the currently edited entry
    pub fn entry_edit_entry_id(&self) -> Option<i32> {
        if self.current_view == View::History {
            self.history_edit_state.as_ref().map(|s| s.entry_id)
        } else {
            self.this_week_edit_state.as_ref().map(|s| s.entry_id)
        }
    }

    /// Update the edit state with selected project (called after project selection)
    pub fn update_edit_state_project(&mut self, project_id: String, project_name: String) {
        // Update whichever edit state is active (this_week or history)
        if let Some(state) = &mut self.this_week_edit_state {
            state.project_id = Some(project_id.clone());
            state.project_name = Some(project_name.clone());
        }
        if let Some(state) = &mut self.history_edit_state {
            state.project_id = Some(project_id);
            state.project_name = Some(project_name);
        }
    }

    /// Update the edit state with selected activity (called after activity selection)
    pub fn update_edit_state_activity(&mut self, activity_id: String, activity_name: String) {
        // Update whichever edit state is active (this_week or history)
        if let Some(state) = &mut self.this_week_edit_state {
            state.activity_id = Some(activity_id.clone());
            state.activity_name = Some(activity_name.clone());
        }
        if let Some(state) = &mut self.history_edit_state {
            state.activity_id = Some(activity_id);
            state.activity_name = Some(activity_name);
        }
    }

    /// Update the edit state note (called after description edit)
    pub fn update_edit_state_note(&mut self, note: String) {
        // Update whichever edit state is active (this_week or history)
        if let Some(state) = &mut self.this_week_edit_state {
            state.note = TextInput::from_str(&note);
        }
        if let Some(state) = &mut self.history_edit_state {
            state.note = TextInput::from_str(&note);
        }
    }

    /// Check if we're in any edit mode (this_week or history)
    pub fn is_in_edit_mode(&self) -> bool {
        self.this_week_edit_state.is_some() || self.history_edit_state.is_some()
    }

    /// Get the return view after edit mode project/activity selection
    pub fn get_return_view_from_edit(&self) -> View {
        if self.history_edit_state.is_some() {
            View::History
        } else {
            View::Timer
        }
    }

    /// Select next item in current list
    pub fn select_next(&mut self) {
        match self.current_view {
            View::SelectProject => {
                if !self.filtered_projects.is_empty() {
                    self.filtered_project_index =
                        (self.filtered_project_index + 1) % self.filtered_projects.len();
                }
            }
            View::SelectActivity => {
                if !self.filtered_activities.is_empty() {
                    self.filtered_activity_index =
                        (self.filtered_activity_index + 1) % self.filtered_activities.len();
                }
            }
            View::History => {
                self.history_focus_down();
            }
            _ => {}
        }
    }

    /// Select previous item in current list
    pub fn select_previous(&mut self) {
        match self.current_view {
            View::SelectProject => {
                if !self.filtered_projects.is_empty() {
                    self.filtered_project_index = if self.filtered_project_index == 0 {
                        self.filtered_projects.len() - 1
                    } else {
                        self.filtered_project_index - 1
                    };
                }
            }
            View::SelectActivity => {
                if !self.filtered_activities.is_empty() {
                    self.filtered_activity_index = if self.filtered_activity_index == 0 {
                        self.filtered_activities.len() - 1
                    } else {
                        self.filtered_activity_index - 1
                    };
                }
            }
            View::History => {
                self.history_focus_up();
            }
            _ => {}
        }
    }

    /// Confirm selection in current view
    pub fn confirm_selection(&mut self) {
        match self.current_view {
            View::SelectProject => {
                if let Some(project) = self.filtered_projects.get(self.filtered_project_index) {
                    self.selected_project = Some(project.clone());
                    // Load activities for selected project
                    self.activities = get_test_activities(&project.id);
                    self.selected_activity_index = 0;
                    self.selected_activity = None;
                    // Initialize filtered activities and clear search
                    self.activity_search_input.clear();
                    self.filtered_activities = self.activities.clone();
                    self.filtered_activity_index = 0;
                    self.set_status(format!("Selected project: {}", project.name));
                    // Automatically show activity selection
                    self.navigate_to(View::SelectActivity);
                }
            }
            View::SelectActivity => {
                if let Some(activity) = self.filtered_activities.get(self.filtered_activity_index) {
                    self.selected_activity = Some(activity.clone());
                    self.set_status(format!("Selected activity: {}", activity.name));
                    // If annotation is default, auto-open editor
                    // Otherwise, return to timer view with annotation box highlighted
                    if self.description_is_default {
                        self.navigate_to(View::EditDescription);
                    } else {
                        self.navigate_to(View::Timer);
                        self.focused_box = FocusedBox::Description;
                    }
                }
            }
            _ => {}
        }
    }

    /// Cancel current selection and return to timer view
    pub fn cancel_selection(&mut self) {
        self.navigate_to(View::Timer);
    }

    /// Get current project name for display
    pub fn current_project_name(&self) -> String {
        self.selected_project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "[None]".to_string())
    }

    /// Get current activity name for display
    pub fn current_activity_name(&self) -> String {
        self.selected_activity
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "[None]".to_string())
    }

    /// Check if project/activity is selected
    pub fn has_project_activity(&self) -> bool {
        self.selected_project.is_some() && self.selected_activity.is_some()
    }

    /// Get contextual status message
    pub fn get_contextual_status(&self) -> String {
        match self.timer_state {
            TimerState::Stopped => {
                "No timer active (press Space/Ctrl+K to start a new timer)".to_string()
            }
            TimerState::Running => {
                if self.has_project_activity() {
                    "Timer active (press Ctrl+S to save)".to_string()
                } else {
                    "Timer active (press P to add Project / Activity)".to_string()
                }
            }
        }
    }

    /// Get current description for display
    pub fn current_description(&self) -> String {
        self.description_input.value.clone()
    }

    /// Get the start of the current week (Monday 00:00:00)
    fn week_start(dt: OffsetDateTime) -> OffsetDateTime {
        let weekday = dt.weekday();
        let days_since_monday = weekday.number_days_from_monday();
        let monday = dt - time::Duration::days(days_since_monday as i64);
        monday.replace_time(time::Time::MIDNIGHT)
    }

    /// Get the end of the current week (Sunday 23:59:59.999999999)
    fn week_end(dt: OffsetDateTime) -> OffsetDateTime {
        let weekday = dt.weekday();
        let days_until_sunday = 6 - weekday.number_days_from_monday();
        let sunday = dt + time::Duration::days(days_until_sunday as i64);
        sunday.replace_time(time::Time::MIDNIGHT) + time::Duration::nanoseconds(86_399_999_999_999)
    }

    /// Get this week's history entries (Monday to Sunday)
    pub fn this_week_history(&self) -> Vec<&TimerHistoryEntry> {
        let now = OffsetDateTime::now_utc();
        let week_start = Self::week_start(now);
        let week_end = Self::week_end(now);
        self.timer_history
            .iter()
            .filter(|entry| entry.start_time >= week_start && entry.start_time <= week_end)
            .collect()
    }

    /// Get history entries from the last month (for History view)
    pub fn last_month_history(&self) -> Vec<&TimerHistoryEntry> {
        let now = OffsetDateTime::now_utc();
        let month_ago = now - time::Duration::days(30);
        self.timer_history
            .iter()
            .filter(|entry| entry.start_time >= month_ago)
            .collect()
    }

    /// Total hours worked this week (completed entries only)
    pub fn worked_hours_this_week(&self) -> f64 {
        self.this_week_history()
            .iter()
            .filter_map(|e| {
                let end = e.end_time?;
                let secs = (end - e.start_time).whole_seconds();
                if secs > 0 {
                    Some(secs as f64 / 3600.0)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Flex time = worked hours - scheduled hours
    pub fn flex_hours_this_week(&self) -> f64 {
        self.worked_hours_this_week() - SCHEDULED_HOURS_PER_WEEK
    }

    /// Weekly hours as a percentage of scheduled hours (0–100, clamped)
    pub fn weekly_hours_percent(&self) -> f64 {
        (self.worked_hours_this_week() / SCHEDULED_HOURS_PER_WEEK * 100.0).clamp(0.0, 100.0)
    }

    /// Per-project/activity breakdown for this week (≥ 1% of total, sorted desc)
    pub fn weekly_project_stats(&self) -> Vec<ProjectStat> {
        use std::collections::HashMap;

        let entries = self.this_week_history();
        let mut map: HashMap<String, f64> = HashMap::new();

        for e in &entries {
            if let Some(end) = e.end_time {
                let secs = (end - e.start_time).whole_seconds();
                if secs > 0 {
                    let project = e
                        .project_name
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string());
                    let activity = e
                        .activity_name
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string());
                    let key = format!("{} - {}", project, activity);
                    *map.entry(key).or_insert(0.0) += secs as f64 / 3600.0;
                }
            }
        }

        let total: f64 = map.values().sum();
        if total == 0.0 {
            return Vec::new();
        }

        let mut stats: Vec<ProjectStat> = map
            .into_iter()
            .filter_map(|(label, hours)| {
                let percentage = hours / total * 100.0;
                if percentage >= 1.0 {
                    Some(ProjectStat {
                        label,
                        hours,
                        percentage,
                    })
                } else {
                    None
                }
            })
            .collect();

        stats.sort_by(|a, b| {
            b.hours
                .partial_cmp(&a.hours)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        stats
    }

    /// Handle character input for description editing
    pub fn input_char(&mut self, c: char) {
        if self.editing_description {
            self.description_input.insert(c);
        }
    }

    /// Handle backspace for description editing
    pub fn input_backspace(&mut self) {
        if self.editing_description {
            self.description_input.backspace();
        }
    }

    pub fn input_move_cursor(&mut self, left: bool) {
        if self.editing_description {
            if left {
                self.description_input.move_left();
            } else {
                self.description_input.move_right();
            }
        }
    }

    pub fn input_cursor_home_end(&mut self, home: bool) {
        if self.editing_description {
            if home {
                self.description_input.home();
            } else {
                self.description_input.end();
            }
        }
    }

    /// Confirm description edit
    pub fn confirm_description(&mut self) {
        self.editing_description = false;
        self.navigate_to(View::Timer);
        self.set_status("Description updated".to_string());
    }

    /// Filter projects based on search input using fuzzy matching
    pub fn filter_projects(&mut self) {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;

        if self.project_search_input.value.is_empty() {
            // Empty search - show all projects
            self.filtered_projects = self.projects.clone();
            self.filtered_project_index = 0;
            return;
        }

        let matcher = SkimMatcherV2::default();
        let mut scored_projects: Vec<(TestProject, i64)> = self
            .projects
            .iter()
            .filter_map(|project| {
                matcher
                    .fuzzy_match(&project.name, &self.project_search_input.value)
                    .map(|score| (project.clone(), score))
            })
            .collect();

        // Sort by score descending (best matches first)
        scored_projects.sort_by(|a, b| b.1.cmp(&a.1));

        self.filtered_projects = scored_projects.into_iter().map(|(p, _)| p).collect();
        self.filtered_project_index = 0;
    }

    /// Handle character input for project search
    pub fn search_input_char(&mut self, c: char) {
        self.project_search_input.insert(c);
        self.filter_projects();
    }

    /// Handle backspace for project search
    pub fn search_input_backspace(&mut self) {
        self.project_search_input.backspace();
        self.filter_projects();
    }

    /// Clear project search input
    pub fn search_input_clear(&mut self) {
        self.project_search_input.clear();
        self.filter_projects();
    }

    /// Filter activities based on search input using fuzzy matching (only for selected project)
    pub fn filter_activities(&mut self) {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;

        if self.activity_search_input.value.is_empty() {
            // Empty search - show all activities for selected project
            self.filtered_activities = self.activities.clone();
            self.filtered_activity_index = 0;
            return;
        }

        let matcher = SkimMatcherV2::default();
        let mut scored_activities: Vec<(TestActivity, i64)> = self
            .activities
            .iter()
            .filter_map(|activity| {
                matcher
                    .fuzzy_match(&activity.name, &self.activity_search_input.value)
                    .map(|score| (activity.clone(), score))
            })
            .collect();

        // Sort by score descending (best matches first)
        scored_activities.sort_by(|a, b| b.1.cmp(&a.1));

        self.filtered_activities = scored_activities.into_iter().map(|(a, _)| a).collect();
        self.filtered_activity_index = 0;
    }

    /// Handle character input for activity search
    pub fn activity_search_input_char(&mut self, c: char) {
        self.activity_search_input.insert(c);
        self.filter_activities();
    }

    /// Handle backspace for activity search
    pub fn activity_search_input_backspace(&mut self) {
        self.activity_search_input.backspace();
        self.filter_activities();
    }

    /// Clear activity search input
    pub fn activity_search_input_clear(&mut self) {
        self.activity_search_input.clear();
        self.filter_activities();
    }

    /// Move cursor left/right in project search input.
    pub fn search_move_cursor(&mut self, left: bool) {
        if left {
            self.project_search_input.move_left();
        } else {
            self.project_search_input.move_right();
        }
    }

    /// Move cursor to home/end in project search input.
    pub fn search_cursor_home_end(&mut self, home: bool) {
        if home {
            self.project_search_input.home();
        } else {
            self.project_search_input.end();
        }
    }

    /// Move cursor left/right in activity search input.
    pub fn activity_search_move_cursor(&mut self, left: bool) {
        if left {
            self.activity_search_input.move_left();
        } else {
            self.activity_search_input.move_right();
        }
    }

    /// Move cursor to home/end in activity search input.
    pub fn activity_search_cursor_home_end(&mut self, home: bool) {
        if home {
            self.activity_search_input.home();
        } else {
            self.activity_search_input.end();
        }
    }

    /// Navigate to next save action option
    pub fn select_next_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::ContinueSameProject => SaveAction::ContinueNewProject,
            SaveAction::ContinueNewProject => SaveAction::SaveAndStop,
            SaveAction::SaveAndStop => SaveAction::Cancel,
            SaveAction::Cancel => SaveAction::ContinueSameProject,
        };
    }

    /// Navigate to previous save action option
    pub fn select_previous_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::ContinueSameProject => SaveAction::Cancel,
            SaveAction::ContinueNewProject => SaveAction::ContinueSameProject,
            SaveAction::SaveAndStop => SaveAction::ContinueNewProject,
            SaveAction::Cancel => SaveAction::SaveAndStop,
        };
    }

    /// Select save action by number (1-4)
    pub fn select_save_action_by_number(&mut self, num: u32) {
        self.selected_save_action = match num {
            1 => SaveAction::ContinueSameProject,
            2 => SaveAction::ContinueNewProject,
            3 => SaveAction::SaveAndStop,
            4 => SaveAction::Cancel,
            _ => return,
        };
    }

    /// Enter git mode (waiting for second key after Ctrl+G).
    pub fn enter_git_mode(&mut self) {
        self.git_mode = true;
    }

    /// Exit git mode without action.
    pub fn exit_git_mode(&mut self) {
        self.git_mode = false;
    }

    /// Paste raw branch name into description_input (inserts at cursor position).
    pub fn paste_git_branch_raw(&mut self) {
        self.git_mode = false;
        if let Some(branch) = &self.git_context.branch.clone() {
            for c in branch.chars() {
                self.description_input.insert(c);
            }
        }
    }

    /// Paste parsed branch name into description_input (inserts at cursor position).
    pub fn paste_git_branch_parsed(&mut self) {
        self.git_mode = false;
        if let Some(branch) = &self.git_context.branch.clone() {
            for c in crate::git::parse_branch(branch).chars() {
                self.description_input.insert(c);
            }
        }
    }

    /// Paste last commit message into description_input (inserts at cursor position).
    pub fn paste_git_last_commit(&mut self) {
        self.git_mode = false;
        if let Some(commit) = &self.git_context.last_commit.clone() {
            for c in commit.chars() {
                self.description_input.insert(c);
            }
        }
    }

    /// Begin CWD change mode. Pre-fill with current cwd string.
    pub fn begin_cwd_change(&mut self) {
        self.git_mode = false;
        self.cwd_input = Some(TextInput::from_str(&self.git_context.cwd.to_string_lossy()));
        self.cwd_completions = Vec::new();
    }

    /// Cancel CWD change mode.
    pub fn cancel_cwd_change(&mut self) {
        self.cwd_input = None;
        self.cwd_completions = Vec::new();
    }

    /// Confirm CWD change. Returns Err if path doesn't exist.
    pub fn confirm_cwd_change(&mut self) -> Result<(), String> {
        let input = self.cwd_input.take().unwrap_or_default();
        self.cwd_completions = Vec::new();
        let path = std::path::PathBuf::from(&input.value);
        if path.is_dir() {
            self.git_context = GitContext::from_cwd(path);
            Ok(())
        } else {
            self.cwd_input = Some(input);
            Err(format!("Not a directory: {}", path.display()))
        }
    }

    /// Tab-complete the current cwd_input. Fills cwd_completions with matches,
    /// and completes to longest common prefix if there are matches.
    pub fn cwd_tab_complete(&mut self) {
        let input = match &self.cwd_input {
            Some(s) => s.value.clone(),
            None => return,
        };

        let path = std::path::Path::new(&input);
        let (dir, prefix) = if input.ends_with('/') || input.ends_with(std::path::MAIN_SEPARATOR) {
            (path, "")
        } else {
            (
                path.parent().unwrap_or(std::path::Path::new(".")),
                path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            )
        };

        let mut matches: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with(prefix) {
                            let full =
                                format!("{}/{}", dir.to_string_lossy().trim_end_matches('/'), name);
                            matches.push(full);
                        }
                    }
                }
            }
        }
        matches.sort();

        if matches.len() == 1 {
            let new_val = format!("{}/", matches[0]);
            self.cwd_input = Some(TextInput::from_str(&new_val));
            self.cwd_completions = Vec::new();
        } else if matches.len() > 1 {
            // Find longest common prefix
            let lcp = longest_common_prefix(&matches);
            self.cwd_input = Some(TextInput::from_str(&lcp));
            self.cwd_completions = matches;
        }
        // no matches: do nothing
    }

    /// Append a char to cwd_input.
    pub fn cwd_input_char(&mut self, c: char) {
        if let Some(s) = &mut self.cwd_input {
            s.insert(c);
            self.cwd_completions.clear();
        }
    }

    /// Backspace in cwd_input.
    pub fn cwd_input_backspace(&mut self) {
        if let Some(s) = &mut self.cwd_input {
            s.backspace();
            self.cwd_completions.clear();
        }
    }

    pub fn cwd_move_cursor(&mut self, left: bool) {
        if let Some(s) = &mut self.cwd_input {
            if left {
                s.move_left();
            } else {
                s.move_right();
            }
        }
    }

    pub fn cwd_cursor_home_end(&mut self, home: bool) {
        if let Some(s) = &mut self.cwd_input {
            if home {
                s.home();
            } else {
                s.end();
            }
        }
    }

    /// Open the Taskwarrior task-picker overlay by running `task status:pending export`.
    /// If the env var `TOKI_TASK_FILTER` is set, its value is split on whitespace and
    /// prepended as extra filter args (e.g. `TOKI_TASK_FILTER="+spinit -paused"`).
    pub fn open_taskwarrior_overlay(&mut self) {
        // NOTE: This blocks the TUI thread for the duration of the `task export` call.
        // For typical task lists this is fast (<100ms), but could be noticeable with
        // Taskwarrior hooks or large databases. A background thread would be cleaner.
        // NOTE: argument order matters — filter first, subcommand second.
        let mut cmd = std::process::Command::new("task");
        // Suppress informational lines that task prints to stdout and would break JSON parsing.
        cmd.arg("rc.verbose=nothing");
        // Apply optional user-defined filter (e.g. "+spinit -paused")
        if let Ok(filter) = std::env::var("TOKI_TASK_FILTER") {
            for token in filter.split_whitespace() {
                cmd.arg(token);
            }
        }
        cmd.args(["status:pending", "export"]);
        let result = cmd.output();
        match result {
            Err(_) => {
                self.taskwarrior_overlay = Some(TaskwarriorOverlay {
                    tasks: vec![],
                    selected: None,
                    error: Some("taskwarrior not found (is `task` in PATH?)".to_string()),
                });
            }
            Ok(out) => match parse_task_export(&out.stdout) {
                Ok(tasks) => {
                    let selected = if tasks.is_empty() { None } else { Some(0) };
                    let error = if out.status.success() || !tasks.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8_lossy(&out.stderr).trim().to_string())
                    };
                    self.taskwarrior_overlay = Some(TaskwarriorOverlay {
                        tasks,
                        selected,
                        error,
                    });
                }
                Err(parse_err) => {
                    self.taskwarrior_overlay = Some(TaskwarriorOverlay {
                        tasks: vec![],
                        selected: None,
                        error: Some(parse_err),
                    });
                }
            },
        }
    }

    /// Close the Taskwarrior overlay without selecting anything.
    pub fn close_taskwarrior_overlay(&mut self) {
        self.taskwarrior_overlay = None;
    }

    /// Move the Taskwarrior overlay selection up (down=false) or down (down=true).
    pub fn taskwarrior_move(&mut self, down: bool) {
        if let Some(overlay) = &mut self.taskwarrior_overlay {
            let len = overlay.tasks.len();
            if len == 0 {
                return;
            }
            overlay.selected = Some(match overlay.selected {
                None => 0,
                Some(i) => {
                    if down {
                        (i + 1).min(len - 1)
                    } else {
                        i.saturating_sub(1)
                    }
                }
            });
        }
    }

    /// Confirm selection: close overlay and append the selected task description
    /// to `description_input`. A space is prepended if the note is non-empty.
    pub fn taskwarrior_confirm(&mut self) {
        let description = self
            .taskwarrior_overlay
            .as_ref()
            .and_then(|o| o.selected.and_then(|i| o.tasks.get(i)))
            .map(|t| t.description.clone());

        self.taskwarrior_overlay = None;

        if let Some(desc) = description {
            if !self.description_input.value.is_empty() {
                self.description_input.insert(' ');
            }
            for c in desc.chars() {
                self.description_input.insert(c);
            }
        }
    }
}

/// Parse the JSON output of `task status:pending export`.
/// Returns a list of TaskEntry values sorted by urgency (highest first),
/// matching the order `task next` would show them.
fn parse_task_export(output: &[u8]) -> Result<Vec<TaskEntry>, String> {
    let text = std::str::from_utf8(output)
        .map_err(|e| format!("task export output is not valid UTF-8: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("task export output is not valid JSON: {}", e))?;
    let arr = json
        .as_array()
        .ok_or_else(|| "task export output is not a JSON array".to_string())?;

    let mut entries: Vec<(TaskEntry, f64)> = arr
        .iter()
        .filter_map(|obj| {
            let id = obj.get("id")?.as_u64()? as u32;
            // id == 0 means the task is completed/deleted (no display ID assigned)
            if id == 0 {
                return None;
            }
            let description = obj.get("description")?.as_str()?.to_string();
            let urgency = obj.get("urgency").and_then(|u| u.as_f64()).unwrap_or(0.0);
            Some((TaskEntry { id, description }, urgency))
        })
        .collect();

    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(entries.into_iter().map(|(entry, _)| entry).collect())
}

fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.chars().zip(s.chars()).enumerate() {
            if a != b {
                len = len.min(i);
                break;
            }
        }
    }
    first[..len].to_string()
}

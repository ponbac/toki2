use crate::api::database::TimerHistoryEntry;
use crate::test_data::{get_test_activities, get_test_projects, TestActivity, TestProject};
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Timer,
    History,
    SelectProject,
    SelectActivity,
    EditDescription,
    SaveAction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaveAction {
    ContinueSameProject,
    ContinueNewProject,
    SaveAndPause,
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

#[derive(Debug, Clone, PartialEq)]
pub enum TodayEditField {
    StartTime,
    EndTime,
    Project,
    Activity,
    Annotation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TodayEditState {
    pub entry_id: i32,
    pub start_time_input: String,
    pub end_time_input: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: String,
    pub focused_field: TodayEditField,
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

    // Project/Activity selection
    pub projects: Vec<TestProject>,
    pub activities: Vec<TestActivity>,
    pub selected_project_index: usize,
    pub selected_activity_index: usize,
    pub selected_project: Option<TestProject>,
    pub selected_activity: Option<TestActivity>,

    // Fuzzy finding for projects
    pub project_search_input: String,
    pub filtered_projects: Vec<TestProject>,
    pub filtered_project_index: usize,

    // Fuzzy finding for activities
    pub activity_search_input: String,
    pub filtered_activities: Vec<TestActivity>,
    pub filtered_activity_index: usize,

    // Save action selection
    pub selected_save_action: SaveAction,

    // Description editing
    pub description_input: String,
    pub editing_description: bool,
    pub description_is_default: bool,
    pub saved_timer_note: Option<String>, // Saved when editing entry note to restore later

    // Today box navigation
    pub focused_today_index: Option<usize>,
    pub today_edit_state: Option<TodayEditState>,
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
            projects: projects.clone(),
            activities: Vec::new(),
            selected_project_index: 0,
            selected_activity_index: 0,
            selected_project: None,
            selected_activity: None,
            project_search_input: String::new(),
            filtered_projects: projects.clone(),
            filtered_project_index: 0,
            activity_search_input: String::new(),
            filtered_activities: Vec::new(),
            filtered_activity_index: 0,
            selected_save_action: SaveAction::ContinueSameProject,
            description_input: String::new(),
            editing_description: false,
            description_is_default: true,
            saved_timer_note: None,
            focused_today_index: None,
            today_edit_state: None,
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
        self.description_input = String::new();
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
            }
            View::SelectActivity => {
                self.selected_activity_index = self
                    .activities
                    .iter()
                    .position(|a| self.selected_activity.as_ref().map(|sa| &sa.id) == Some(&a.id))
                    .unwrap_or(0);
            }
            View::EditDescription => {
                // If in Today edit mode, don't clear - the view handler will set it from edit_state
                if self.description_is_default && self.today_edit_state.is_none() {
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
                self.enter_today_edit_mode();
            }
        }
    }

    /// Move focus up in Today box, wrap to Description if at top
    pub fn today_focus_up(&mut self) {
        let today_count = self.todays_history().len();
        if today_count == 0 {
            self.focused_box = FocusedBox::Description;
            self.focused_today_index = None;
            return;
        }

        if let Some(idx) = self.focused_today_index {
            if idx == 0 {
                self.focused_box = FocusedBox::Description;
                self.focused_today_index = None;
            } else {
                self.focused_today_index = Some(idx - 1);
            }
        } else {
            self.focused_today_index = Some(today_count - 1);
        }
    }

    /// Move focus down in Today box, wrap to Timer if at bottom
    pub fn today_focus_down(&mut self) {
        let today_count = self.todays_history().len();
        if today_count == 0 {
            self.focused_box = FocusedBox::Timer;
            self.focused_today_index = None;
            return;
        }

        if let Some(idx) = self.focused_today_index {
            if idx >= today_count - 1 {
                self.focused_box = FocusedBox::Timer;
                self.focused_today_index = None;
            } else {
                self.focused_today_index = Some(idx + 1);
            }
        } else {
            self.focused_today_index = Some(0);
        }
    }

    /// Enter edit mode for the currently focused Today entry
    pub fn enter_today_edit_mode(&mut self) {
        if let Some(idx) = self.focused_today_index {
            let todays = self.todays_history();
            if let Some(entry) = todays.get(idx) {
                let start_time = to_local_time(entry.start_time).time();
                let start_str = format!("{:02}:{:02}", start_time.hour(), start_time.minute());

                let end_str = entry
                    .end_time
                    .map(|et| {
                        let t = to_local_time(et).time();
                        format!("{:02}:{:02}", t.hour(), t.minute())
                    })
                    .unwrap_or_else(|| "00:00".to_string());

                self.today_edit_state = Some(TodayEditState {
                    entry_id: entry.id,
                    start_time_input: start_str,
                    end_time_input: end_str,
                    project_id: entry.project_id.clone(),
                    project_name: entry.project_name.clone(),
                    activity_id: entry.activity_id.clone(),
                    activity_name: entry.activity_name.clone(),
                    note: entry.note.clone().unwrap_or_default(),
                    focused_field: TodayEditField::StartTime,
                    validation_error: None,
                });
            }
        }
    }

    /// Exit edit mode and discard changes
    pub fn exit_today_edit_mode(&mut self) {
        self.today_edit_state = None;
    }

    /// Move to next field in edit mode
    pub fn today_edit_next_field(&mut self) {
        if let Some(state) = &mut self.today_edit_state {
            state.focused_field = match state.focused_field {
                TodayEditField::StartTime => TodayEditField::EndTime,
                TodayEditField::EndTime => TodayEditField::Project,
                TodayEditField::Project => TodayEditField::Activity,
                TodayEditField::Activity => TodayEditField::Annotation,
                TodayEditField::Annotation => TodayEditField::StartTime,
            };
            state.validation_error = None;
        }
    }

    /// Move to previous field in edit mode
    pub fn today_edit_prev_field(&mut self) {
        if let Some(state) = &mut self.today_edit_state {
            state.focused_field = match state.focused_field {
                TodayEditField::StartTime => TodayEditField::Annotation,
                TodayEditField::EndTime => TodayEditField::StartTime,
                TodayEditField::Project => TodayEditField::EndTime,
                TodayEditField::Activity => TodayEditField::Project,
                TodayEditField::Annotation => TodayEditField::Activity,
            };
            state.validation_error = None;
        }
    }

    /// Set the focused field in edit mode
    pub fn today_edit_set_focused_field(&mut self, field: TodayEditField) {
        if let Some(state) = &mut self.today_edit_state {
            state.focused_field = field;
            state.validation_error = None;
        }
    }

    /// Handle character input in edit mode
    pub fn today_edit_input_char(&mut self, c: char) {
        if let Some(state) = &mut self.today_edit_state {
            match state.focused_field {
                TodayEditField::StartTime => {
                    // Auto-clear if starting to type (field shows full time with brackets)
                    if state.start_time_input.len() >= 5 {
                        state.start_time_input.clear();
                    }
                    if c.is_ascii_digit() {
                        state.start_time_input.push(c);
                        // Auto-insert colon after 2 digits
                        if state.start_time_input.len() == 2 {
                            state.start_time_input.push(':');
                        }
                    }
                }
                TodayEditField::EndTime => {
                    // Auto-clear if starting to type
                    if state.end_time_input.len() >= 5 {
                        state.end_time_input.clear();
                    }
                    if c.is_ascii_digit() {
                        state.end_time_input.push(c);
                        // Auto-insert colon after 2 digits
                        if state.end_time_input.len() == 2 {
                            state.end_time_input.push(':');
                        }
                    }
                }
                TodayEditField::Annotation => {
                    state.note.push(c);
                }
                TodayEditField::Project | TodayEditField::Activity => {
                    // These are handled via modals
                }
            }
        }
    }

    /// Handle backspace in edit mode
    pub fn today_edit_backspace(&mut self) {
        if let Some(state) = &mut self.today_edit_state {
            match state.focused_field {
                TodayEditField::StartTime => {
                    // Remove colon if present when backspacing
                    if state.start_time_input.ends_with(':') {
                        state.start_time_input.pop();
                    }
                    state.start_time_input.pop();
                }
                TodayEditField::EndTime => {
                    if state.end_time_input.ends_with(':') {
                        state.end_time_input.pop();
                    }
                    state.end_time_input.pop();
                }
                TodayEditField::Annotation => {
                    state.note.pop();
                }
                TodayEditField::Project | TodayEditField::Activity => {
                    // These are handled via modals
                }
            }
        }
    }

    /// Clear the current time field for direct re-entry
    pub fn today_edit_clear_time(&mut self) {
        if let Some(state) = &mut self.today_edit_state {
            match state.focused_field {
                TodayEditField::StartTime => {
                    state.start_time_input.clear();
                }
                TodayEditField::EndTime => {
                    state.end_time_input.clear();
                }
                _ => {}
            }
        }
    }

    /// Validate edit state and return error if invalid
    pub fn today_edit_validate(&self) -> Option<String> {
        if let Some(state) = &self.today_edit_state {
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
        } else {
            None
        }
    }

    /// Get the entry ID for the currently edited entry
    pub fn today_edit_entry_id(&self) -> Option<i32> {
        self.today_edit_state.as_ref().map(|s| s.entry_id)
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
                if !self.timer_history.is_empty() {
                    self.history_scroll =
                        (self.history_scroll + 1).min(self.timer_history.len().saturating_sub(1));
                }
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
                if self.history_scroll > 0 {
                    self.history_scroll -= 1;
                }
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
        self.description_input.clone()
    }

    /// Get today's history entries
    pub fn todays_history(&self) -> Vec<&TimerHistoryEntry> {
        let today = OffsetDateTime::now_utc().date();
        self.timer_history
            .iter()
            .filter(|entry| entry.start_time.date() == today)
            .collect()
    }

    /// Handle character input for description editing
    pub fn input_char(&mut self, c: char) {
        if self.editing_description {
            self.description_input.push(c);
        }
    }

    /// Handle backspace for description editing
    pub fn input_backspace(&mut self) {
        if self.editing_description {
            self.description_input.pop();
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

        if self.project_search_input.is_empty() {
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
                    .fuzzy_match(&project.name, &self.project_search_input)
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
        self.project_search_input.push(c);
        self.filter_projects();
    }

    /// Handle backspace for project search
    pub fn search_input_backspace(&mut self) {
        self.project_search_input.pop();
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

        if self.activity_search_input.is_empty() {
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
                    .fuzzy_match(&activity.name, &self.activity_search_input)
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
        self.activity_search_input.push(c);
        self.filter_activities();
    }

    /// Handle backspace for activity search
    pub fn activity_search_input_backspace(&mut self) {
        self.activity_search_input.pop();
        self.filter_activities();
    }

    /// Clear activity search input
    pub fn activity_search_input_clear(&mut self) {
        self.activity_search_input.clear();
        self.filter_activities();
    }

    /// Navigate to next save action option
    pub fn select_next_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::ContinueSameProject => SaveAction::ContinueNewProject,
            SaveAction::ContinueNewProject => SaveAction::SaveAndPause,
            SaveAction::SaveAndPause => SaveAction::Cancel,
            SaveAction::Cancel => SaveAction::ContinueSameProject,
        };
    }

    /// Navigate to previous save action option
    pub fn select_previous_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::ContinueSameProject => SaveAction::Cancel,
            SaveAction::ContinueNewProject => SaveAction::ContinueSameProject,
            SaveAction::SaveAndPause => SaveAction::ContinueNewProject,
            SaveAction::Cancel => SaveAction::SaveAndPause,
        };
    }

    /// Select save action by number (1-4)
    pub fn select_save_action_by_number(&mut self, num: u32) {
        self.selected_save_action = match num {
            1 => SaveAction::ContinueSameProject,
            2 => SaveAction::ContinueNewProject,
            3 => SaveAction::SaveAndPause,
            4 => SaveAction::Cancel,
            _ => return,
        };
    }
}

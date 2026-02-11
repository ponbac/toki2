use crate::api::database::TimerHistoryEntry;
use crate::test_data::{get_test_activities, get_test_projects, TestActivity, TestProject};
use std::time::{Duration, Instant};
use time::OffsetDateTime;

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

    // Save action selection
    pub selected_save_action: SaveAction,

    // Description editing
    pub description_input: String,
    pub editing_description: bool,
    pub description_is_default: bool,
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
            selected_save_action: SaveAction::ContinueSameProject,
            description_input: String::new(),
            editing_description: false,
            description_is_default: true,
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
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
                // Clear default text on first edit, otherwise keep existing text
                if self.description_is_default {
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
            FocusedBox::Description => FocusedBox::Timer,
        };
    }

    /// Move focus to previous box (vim-style k or up)
    pub fn focus_previous(&mut self) {
        self.focused_box = match self.focused_box {
            FocusedBox::Timer => FocusedBox::Description,
            FocusedBox::ProjectActivity => FocusedBox::Timer,
            FocusedBox::Description => FocusedBox::ProjectActivity,
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
                if !self.activities.is_empty() {
                    self.selected_activity_index =
                        (self.selected_activity_index + 1) % self.activities.len();
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
                if !self.activities.is_empty() {
                    self.selected_activity_index = if self.selected_activity_index == 0 {
                        self.activities.len() - 1
                    } else {
                        self.selected_activity_index - 1
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
                    self.set_status(format!("Selected project: {}", project.name));
                    // Automatically show activity selection
                    self.navigate_to(View::SelectActivity);
                }
            }
            View::SelectActivity => {
                if let Some(activity) = self.activities.get(self.selected_activity_index) {
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

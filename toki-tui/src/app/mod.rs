use crate::config::TokiConfig;
use crate::time_utils::to_local_time;
use crate::types::{Activity, Project, TimeEntry};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use time::OffsetDateTime;

mod edit;
mod history;
mod navigation;
mod state;
pub use history::parse_date_str;
pub use state::{
    DailyProjectStat, DayStat, DeleteContext, DeleteOrigin, EntryEditField, EntryEditState,
    FocusedBox, GitContext, MilltimeReauthField, MilltimeReauthState, ProjectStat, SaveAction,
    TaskEntry, TaskwarriorOverlay, TextInput, TimerSize, TimerState, View,
};

pub struct App {
    pub running: bool,
    pub timer_state: TimerState,
    pub absolute_start: Option<OffsetDateTime>, // UTC time when timer started
    pub local_start: Option<Instant>,           // For UI duration display
    #[allow(dead_code)]
    pub user_id: i32,
    pub status_message: Option<String>,
    pub current_view: View,
    pub focused_box: FocusedBox,
    pub timer_size: TimerSize,

    // Timer history
    pub time_entries: Vec<TimeEntry>,
    pub history_scroll: usize,
    pub overlapping_entry_ids: HashSet<String>, // Registration IDs that have overlapping times

    // Project/Activity selection
    pub projects: Vec<Project>,
    pub activities: Vec<Activity>,
    pub selected_project_index: usize,
    pub selected_activity_index: usize,
    pub selected_project: Option<Project>,
    pub selected_activity: Option<Activity>,

    // Fuzzy finding for projects
    pub project_search_input: TextInput,
    pub filtered_projects: Vec<Project>,
    pub filtered_project_index: usize,

    // Fuzzy finding for activities
    pub activity_search_input: TextInput,
    pub filtered_activities: Vec<Activity>,
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
    pub pending_edit_selection_restore: Option<(Option<Project>, Option<Activity>)>,

    // Today box navigation (This Week view)
    pub focused_this_week_index: Option<usize>,
    pub this_week_edit_state: Option<EntryEditState>,
    pub this_week_scroll: usize, // Scroll offset (# logical rows skipped from top)
    pub this_week_view_height: usize, // Last-rendered inner height (updated by renderer each frame)

    // History view navigation and editing
    pub focused_history_index: Option<usize>,
    pub history_edit_state: Option<EntryEditState>,
    pub history_list_entries: Vec<usize>, // Indices into time_entries for entries (excludes date separators)
    pub history_view_height: usize, // Last-rendered inner height (updated by renderer each frame)

    // Delete confirmation
    pub delete_context: Option<DeleteContext>,

    // Git context for note editor
    pub git_context: GitContext,
    pub git_mode: bool,
    pub zen_mode: bool,
    pub cwd_input: Option<TextInput>, // Some(_) when changing directory
    pub cwd_completions: Vec<String>, // Tab completion candidates
    pub taskwarrior_overlay: Option<TaskwarriorOverlay>,

    // Loading indicator
    pub is_loading: bool,
    pub throbber_state: throbber_widgets_tui::ThrobberState,

    // Scheduled hours per week from Milltime (defaults to 40.0 until fetched)
    pub scheduled_hours_per_week: f64,

    /// Total accumulated flex time from Milltime (0.0 until fetched at startup)
    pub flex_time_current: f64,

    // Activity cache: project_id -> fetched activities
    pub activity_cache: HashMap<String, Vec<Activity>>,

    // Statistics cache — computed once per history update, used every render frame
    pub weekly_stats_cache: Vec<ProjectStat>,
    pub weekly_daily_stats_cache: Vec<DayStat>,

    // Milltime re-auth overlay — shown when Milltime cookies expire mid-session
    pub milltime_reauth: Option<MilltimeReauthState>,

    // Config values used at runtime
    pub task_filter: String,
    pub git_default_prefix: String,
}

impl App {
    pub fn new(user_id: i32, cfg: &TokiConfig) -> Self {
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
            time_entries: Vec::new(),
            history_scroll: 0,
            overlapping_entry_ids: HashSet::new(),
            projects: Vec::new(),
            activities: Vec::new(),
            selected_project_index: 0,
            selected_activity_index: 0,
            selected_project: None,
            selected_activity: None,
            project_search_input: TextInput::new(),
            filtered_projects: Vec::new(),
            filtered_project_index: 0,
            activity_search_input: TextInput::new(),
            filtered_activities: Vec::new(),
            filtered_activity_index: 0,
            selection_list_focused: false,
            selected_save_action: SaveAction::SaveAndStop,
            description_input: TextInput::new(),
            editing_description: false,
            description_is_default: true,
            saved_timer_note: None,
            pending_edit_selection_restore: None,
            focused_this_week_index: None,
            this_week_edit_state: None,
            this_week_scroll: 0,
            this_week_view_height: 0,
            focused_history_index: None,
            history_edit_state: None,
            history_list_entries: Vec::new(),
            history_view_height: 0,
            delete_context: None,
            git_context: GitContext::from_cwd(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            git_mode: false,
            zen_mode: false,
            cwd_input: None,
            cwd_completions: Vec::new(),
            taskwarrior_overlay: None,
            is_loading: false,
            throbber_state: throbber_widgets_tui::ThrobberState::default(),
            scheduled_hours_per_week: 40.0,
            flex_time_current: 0.0,
            activity_cache: HashMap::new(),
            weekly_stats_cache: Vec::new(),
            weekly_daily_stats_cache: Vec::new(),
            milltime_reauth: None,
            task_filter: cfg.task_filter.clone(),
            git_default_prefix: cfg.git_default_prefix.clone(),
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
    pub fn clear_project_activity(&mut self) {
        self.selected_project = None;
        self.selected_activity = None;
        self.status_message = Some("Project and activity cleared".to_string());
    }

    pub fn clear_note(&mut self) {
        self.description_input = TextInput::new();
        self.description_is_default = true;
        self.status_message = Some("Note cleared".to_string());
    }

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

    /// Populate delete_context from the currently selected entry and switch to ConfirmDelete view.
    /// Call only when not in edit mode and a row is selected.
    pub fn enter_delete_confirm(&mut self, origin: DeleteOrigin) {
        let ctx = match origin {
            DeleteOrigin::Timer => {
                let idx = match self.focused_this_week_index {
                    Some(i) => i,
                    None => return,
                };
                // Skip the running-timer row (index 0 when timer is running)
                let db_idx = if self.timer_state == TimerState::Running {
                    if idx == 0 {
                        return;
                    } // can't delete the live timer this way
                    idx.saturating_sub(1)
                } else {
                    idx
                };
                let entries = self.this_week_history();
                let e = match entries.get(db_idx) {
                    Some(e) => e,
                    None => return,
                };
                DeleteContext {
                    registration_id: e.registration_id.clone(),
                    display_label: format!("{} / {}", e.project_name, e.activity_name),
                    display_date: e.date.clone(),
                    display_hours: e.hours,
                    origin,
                }
            }
            DeleteOrigin::History => {
                let list_idx = match self.focused_history_index {
                    Some(i) => i,
                    None => return,
                };
                let entry_idx = match self.history_list_entries.get(list_idx) {
                    Some(&i) => i,
                    None => return,
                };
                let e = match self.time_entries.get(entry_idx) {
                    Some(e) => e,
                    None => return,
                };
                DeleteContext {
                    registration_id: e.registration_id.clone(),
                    display_label: format!("{} / {}", e.project_name, e.activity_name),
                    display_date: e.date.clone(),
                    display_hours: e.hours,
                    origin,
                }
            }
        };
        self.delete_context = Some(ctx);
        self.navigate_to(View::ConfirmDelete);
    }

    /// Start a new timer
    pub fn start_timer(&mut self) {
        self.timer_state = TimerState::Running;
        self.absolute_start = Some(OffsetDateTime::now_utc());
        self.local_start = Some(Instant::now());
        // Shift focus: running timer row is inserted at index 0, pushing DB entries up by 1
        if let Some(idx) = self.focused_this_week_index {
            self.focused_this_week_index = Some(idx + 1);
        }
    }

    /// Stop the timer (without saving)
    #[allow(dead_code)]
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

    /// Get the elapsed time for the current timer.
    ///
    /// Uses `absolute_start` (wall-clock UTC) when available so that elapsed
    /// time remains accurate after the system sleeps and wakes — `Instant` is
    /// monotonic and does not advance during sleep on most platforms.
    pub fn elapsed_duration(&self) -> Duration {
        match self.timer_state {
            TimerState::Stopped => Duration::ZERO,
            TimerState::Running => {
                if let Some(abs) = self.absolute_start {
                    let secs = (OffsetDateTime::now_utc() - abs).whole_seconds().max(0) as u64;
                    Duration::from_secs(secs)
                } else {
                    self.local_start
                        .map(|start| start.elapsed())
                        .unwrap_or_default()
                }
            }
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
    pub fn update_history(&mut self, mut entries: Vec<TimeEntry>) {
        // Sort: newest date first, then by end_time desc within each date (nulls last)
        entries.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| match (b.end_time, a.end_time) {
                    (Some(be), Some(ae)) => be.cmp(&ae),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
        });
        self.time_entries = entries;
        self.history_scroll = 0;
        self.compute_overlaps();
        // Recompute statistics caches — these are expensive (multiple passes over history)
        // and are called every render frame, so we compute once here and serve cached values.
        self.weekly_stats_cache = self.weekly_project_stats();
        self.weekly_daily_stats_cache = self.weekly_daily_stats();
    }

    /// Load projects and activities derived from timer history (via HTTP API).
    pub fn set_projects_activities(&mut self, projects: Vec<Project>, activities: Vec<Activity>) {
        self.filtered_projects = projects.clone();
        self.projects = projects;
        self.activities = activities;
        self.filtered_project_index = 0;
        self.filtered_activity_index = 0;
    }

    /// Navigate to a different view
    pub fn navigate_to(&mut self, view: View) {
        self.current_view = view;
        self.clear_status();

        match view {
            View::SelectProject => {
                self.selected_project_index = self
                    .projects
                    .iter()
                    .position(|p| self.selected_project.as_ref().map(|sp| &sp.id) == Some(&p.id))
                    .unwrap_or(0);
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
                self.activity_search_input.clear();
                self.filter_activities();
                self.selection_list_focused = false;
            }
            View::EditDescription => {
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
                self.focused_box = FocusedBox::Timer;
            }
            _ => {}
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
                    let project_id = project.id.clone();
                    self.selected_activity_index = 0;
                    self.selected_activity = None;
                    self.activity_search_input.clear();
                    self.filtered_activities = self
                        .activities
                        .iter()
                        .filter(|a| a.project_id == project_id)
                        .cloned()
                        .collect();
                    self.filtered_activity_index = 0;
                    self.set_status(format!("Selected project: {}", project.name));
                    self.navigate_to(View::SelectActivity);
                }
            }
            View::SelectActivity => {
                if let Some(activity) = self.filtered_activities.get(self.filtered_activity_index) {
                    self.selected_activity = Some(activity.clone());
                    self.set_status(format!("Selected activity: {}", activity.name));
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
        if let Some((restore_project, restore_activity)) =
            self.pending_edit_selection_restore.take()
        {
            self.selected_project = restore_project;
            self.selected_activity = restore_activity;
        }
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
                    "Timer active (press Space or Ctrl+S to save, Ctrl+X to clear)".to_string()
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
        if self.project_search_input.value.is_empty() {
            self.filtered_projects = self.projects.clone();
            self.filtered_project_index = 0;
            return;
        }

        let matcher = SkimMatcherV2::default();
        let mut scored_projects: Vec<(Project, i64)> = self
            .projects
            .iter()
            .filter_map(|project| {
                matcher
                    .fuzzy_match(&project.name, &self.project_search_input.value)
                    .map(|score| (project.clone(), score))
            })
            .collect();

        scored_projects.sort_by(|a, b| b.1.cmp(&a.1));
        self.filtered_projects = scored_projects.into_iter().map(|(p, _)| p).collect();
        self.filtered_project_index = 0;
    }

    pub fn search_input_char(&mut self, c: char) {
        self.project_search_input.insert(c);
        self.filter_projects();
    }

    pub fn search_input_backspace(&mut self) {
        self.project_search_input.backspace();
        self.filter_projects();
    }

    pub fn search_input_clear(&mut self) {
        self.project_search_input.clear();
        self.filter_projects();
    }

    /// Filter activities based on search input using fuzzy matching
    pub fn filter_activities(&mut self) {
        let selected_project_id = self
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str());
        let project_activities = self
            .activities
            .iter()
            .filter(|activity| {
                selected_project_id
                    .map(|project_id| activity.project_id == project_id)
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();

        if self.activity_search_input.value.is_empty() {
            self.filtered_activities = project_activities;
            self.filtered_activity_index = 0;
            return;
        }

        let matcher = SkimMatcherV2::default();
        let mut scored_activities: Vec<(Activity, i64)> = self
            .activities
            .iter()
            .filter(|activity| {
                selected_project_id
                    .map(|project_id| activity.project_id == project_id)
                    .unwrap_or(true)
            })
            .filter_map(|activity| {
                matcher
                    .fuzzy_match(&activity.name, &self.activity_search_input.value)
                    .map(|score| (activity.clone(), score))
            })
            .collect();

        scored_activities.sort_by(|a, b| b.1.cmp(&a.1));
        self.filtered_activities = scored_activities.into_iter().map(|(a, _)| a).collect();
        self.filtered_activity_index = 0;
    }

    pub fn activity_search_input_char(&mut self, c: char) {
        self.activity_search_input.insert(c);
        self.filter_activities();
    }

    pub fn activity_search_input_backspace(&mut self) {
        self.activity_search_input.backspace();
        self.filter_activities();
    }

    pub fn activity_search_input_clear(&mut self) {
        self.activity_search_input.clear();
        self.filter_activities();
    }

    pub fn search_move_cursor(&mut self, left: bool) {
        if left {
            self.project_search_input.move_left();
        } else {
            self.project_search_input.move_right();
        }
    }

    pub fn search_cursor_home_end(&mut self, home: bool) {
        if home {
            self.project_search_input.home();
        } else {
            self.project_search_input.end();
        }
    }

    pub fn activity_search_move_cursor(&mut self, left: bool) {
        if left {
            self.activity_search_input.move_left();
        } else {
            self.activity_search_input.move_right();
        }
    }

    pub fn activity_search_cursor_home_end(&mut self, home: bool) {
        if home {
            self.activity_search_input.home();
        } else {
            self.activity_search_input.end();
        }
    }

    pub fn select_next_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::SaveAndStop => SaveAction::ContinueNewProject,
            SaveAction::ContinueNewProject => SaveAction::ContinueSameProject,
            SaveAction::ContinueSameProject => SaveAction::Cancel,
            SaveAction::Cancel => SaveAction::SaveAndStop,
        };
    }

    pub fn select_previous_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::SaveAndStop => SaveAction::Cancel,
            SaveAction::ContinueNewProject => SaveAction::SaveAndStop,
            SaveAction::ContinueSameProject => SaveAction::ContinueNewProject,
            SaveAction::Cancel => SaveAction::ContinueSameProject,
        };
    }

    pub fn select_save_action_by_number(&mut self, num: u32) {
        self.selected_save_action = match num {
            1 => SaveAction::SaveAndStop,
            2 => SaveAction::ContinueNewProject,
            3 => SaveAction::ContinueSameProject,
            4 => SaveAction::Cancel,
            _ => return,
        };
    }

    pub fn enter_git_mode(&mut self) {
        self.git_mode = true;
    }

    pub fn exit_git_mode(&mut self) {
        self.git_mode = false;
    }

    pub fn toggle_zen_mode(&mut self) {
        self.zen_mode = !self.zen_mode;
    }

    pub fn exit_zen_mode(&mut self) {
        self.zen_mode = false;
    }

    pub fn paste_git_branch_raw(&mut self) {
        self.git_mode = false;
        if let Some(branch) = &self.git_context.branch.clone() {
            for c in branch.chars() {
                self.description_input.insert(c);
            }
        }
    }

    pub fn paste_git_branch_parsed(&mut self) {
        self.git_mode = false;
        if let Some(branch) = &self.git_context.branch.clone() {
            for c in crate::git::parse_branch(branch, &self.git_default_prefix).chars() {
                self.description_input.insert(c);
            }
        }
    }

    pub fn paste_git_last_commit(&mut self) {
        self.git_mode = false;
        if let Some(commit) = &self.git_context.last_commit.clone() {
            for c in commit.chars() {
                self.description_input.insert(c);
            }
        }
    }

    pub fn begin_cwd_change(&mut self) {
        self.git_mode = false;
        self.cwd_input = Some(TextInput::from_str(&self.git_context.cwd.to_string_lossy()));
        self.cwd_completions = Vec::new();
    }

    pub fn cancel_cwd_change(&mut self) {
        self.cwd_input = None;
        self.cwd_completions = Vec::new();
    }

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

    pub fn open_milltime_reauth(&mut self) {
        self.milltime_reauth = Some(MilltimeReauthState::default());
    }

    pub fn close_milltime_reauth(&mut self) {
        self.milltime_reauth = None;
    }

    pub fn milltime_reauth_input_char(&mut self, c: char) {
        if let Some(state) = &mut self.milltime_reauth {
            match state.focused_field {
                MilltimeReauthField::Username => state.username_input.insert(c),
                MilltimeReauthField::Password => state.password_input.insert(c),
            }
        }
    }

    pub fn milltime_reauth_backspace(&mut self) {
        if let Some(state) = &mut self.milltime_reauth {
            match state.focused_field {
                MilltimeReauthField::Username => state.username_input.backspace(),
                MilltimeReauthField::Password => state.password_input.backspace(),
            }
        }
    }

    pub fn milltime_reauth_next_field(&mut self) {
        if let Some(state) = &mut self.milltime_reauth {
            state.focused_field = match state.focused_field {
                MilltimeReauthField::Username => MilltimeReauthField::Password,
                MilltimeReauthField::Password => MilltimeReauthField::Username,
            };
        }
    }

    pub fn milltime_reauth_set_error(&mut self, err: String) {
        if let Some(state) = &mut self.milltime_reauth {
            state.error = Some(err);
        }
    }

    /// Returns (username, password) for the re-auth overlay, if present.
    pub fn milltime_reauth_credentials(&self) -> Option<(String, String)> {
        self.milltime_reauth.as_ref().map(|s| {
            (
                s.username_input.value.clone(),
                s.password_input.value.clone(),
            )
        })
    }

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
            let lcp = longest_common_prefix(&matches);
            self.cwd_input = Some(TextInput::from_str(&lcp));
            self.cwd_completions = matches;
        }
    }

    pub fn cwd_input_char(&mut self, c: char) {
        if let Some(s) = &mut self.cwd_input {
            s.insert(c);
            self.cwd_completions.clear();
        }
    }

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

    pub fn open_taskwarrior_overlay(&mut self) {
        let mut cmd = std::process::Command::new("task");
        cmd.arg("rc.verbose=nothing");
        for token in self.task_filter.split_whitespace() {
            cmd.arg(token);
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

    pub fn close_taskwarrior_overlay(&mut self) {
        self.taskwarrior_overlay = None;
    }

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

    let mut prefix: Vec<char> = strings[0].chars().collect();
    for s in &strings[1..] {
        let mut matched_chars = 0usize;
        for (a, b) in prefix.iter().zip(s.chars()) {
            if *a != b {
                break;
            }
            matched_chars += 1;
        }
        prefix.truncate(matched_chars);
        if prefix.is_empty() {
            break;
        }
    }

    prefix.into_iter().collect()
}

pub const SCHEDULED_HOURS_PER_WEEK: f64 = 40.0;

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

/// Per-project/activity breakdown for the statistics view
#[derive(Debug, Clone)]
pub struct ProjectStat {
    pub label: String, // "Project - Activity"
    pub hours: f64,
    pub percentage: f64, // 0.0–100.0 of total worked this week
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

    #[allow(dead_code)]
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
    pub fn split_at_cursor(&self) -> (&str, &str) {
        (&self.value[..self.cursor], &self.value[self.cursor..])
    }

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

// Keep Instant re-exported so App struct can use it without needing to import state internals

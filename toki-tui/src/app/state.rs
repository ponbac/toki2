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
    ConfirmDelete,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaveAction {
    ContinueSameProject,
    ContinueNewProject,
    SaveAndStop,
    Cancel,
}

/// Which view was active when delete was triggered — used to return after dismiss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeleteOrigin {
    Timer,
    History,
}

/// Context for the delete-confirmation modal.
#[derive(Debug, Clone)]
pub struct DeleteContext {
    pub registration_id: String,
    pub display_label: String, // "Project / Activity"
    pub display_date: String,  // "YYYY-MM-DD"
    pub display_hours: f64,
    pub origin: DeleteOrigin,
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
    pub label: String, // "Project: Activity"
    pub hours: f64,
    pub percentage: f64, // 0.0–100.0 of total worked this week
}

/// One project/activity's contribution to a single day
#[derive(Debug, Clone)]
pub struct DailyProjectStat {
    pub label: String, // "Project: Activity"
    pub hours: f64,
    pub color_index: usize, // index into the shared PALETTE
}

/// Hours breakdown for one weekday
#[derive(Debug, Clone)]
pub struct DayStat {
    pub day_name: String, // "Mon", "Tue", …
    pub total_hours: f64,
    pub projects: Vec<DailyProjectStat>, // sorted by color_index asc (same order as pie)
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

    /// Move cursor left by one whitespace-delimited word (bash/readline style).
    /// Skips whitespace leftward, then skips non-whitespace leftward.
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Step 1: skip whitespace to the left
        let mut p = self.cursor;
        while p > 0 {
            let prev = self.prev_boundary(p);
            if self.value[prev..p]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
            {
                p = prev;
            } else {
                break;
            }
        }
        // Step 2: skip non-whitespace to the left
        while p > 0 {
            let prev = self.prev_boundary(p);
            if self.value[prev..p]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
            {
                break;
            } else {
                p = prev;
            }
        }
        self.cursor = p;
    }

    /// Move cursor right by one whitespace-delimited word (bash/readline style).
    /// Skips non-whitespace rightward, then skips whitespace rightward.
    pub fn move_word_right(&mut self) {
        let len = self.value.len();
        if self.cursor >= len {
            return;
        }
        // Step 1: skip non-whitespace to the right
        let mut p = self.cursor;
        while p < len {
            let next = self.next_boundary(p);
            if self.value[p..next]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
            {
                break;
            } else {
                p = next;
            }
        }
        // Step 2: skip whitespace to the right
        while p < len {
            let next = self.next_boundary(p);
            if self.value[p..next]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
            {
                p = next;
            } else {
                break;
            }
        }
        self.cursor = p;
    }

    /// Delete the word immediately before the cursor (Alt+Backspace / readline kill-word-back).
    /// Equivalent to move_word_left then delete from new position to old cursor.
    pub fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let old_cursor = self.cursor;
        self.move_word_left();
        self.value.drain(self.cursor..old_cursor);
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
    pub registration_id: String, // "" = running timer sentinel
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

/// State for the Milltime re-authentication overlay.
/// Shown when Milltime cookies expire mid-session.
#[derive(Debug, Clone, Default)]
pub struct MilltimeReauthState {
    pub username_input: TextInput,
    pub password_input: TextInput,
    pub focused_field: MilltimeReauthField,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum MilltimeReauthField {
    #[default]
    Username,
    Password,
}

// Keep Instant re-exported so App struct can use it without needing to import state internals

#[cfg(test)]
mod tests {
    use super::TextInput;

    #[test]
    fn text_input_inserts_and_backspaces_at_utf8_boundaries() {
        let mut input = TextInput::from_str("a");
        input.insert('e');
        input.insert('\u{301}');

        assert_eq!(input.value, "ae\u{301}");

        input.backspace();
        assert_eq!(input.value, "ae");

        input.backspace();
        assert_eq!(input.value, "a");
    }

    #[test]
    fn text_input_moves_cursor_left_and_right_by_char() {
        let mut input = TextInput::from_str("a😀b");

        input.move_left();
        assert_eq!(input.cursor, "a😀".len());

        input.move_left();
        assert_eq!(input.cursor, "a".len());

        input.move_right();
        assert_eq!(input.cursor, "a😀".len());

        input.move_right();
        assert_eq!(input.cursor, "a😀b".len());
    }

    #[test]
    fn text_input_move_word_left_basic() {
        let mut ti = TextInput::from_str("hello world foo");
        // cursor at end (15)
        ti.move_word_left(); // skip 0 whitespace, skip "foo" → cursor at 12
        assert_eq!(ti.cursor, 12);
        ti.move_word_left(); // skip 1 space, skip "world" → cursor at 6
        assert_eq!(ti.cursor, 6);
        ti.move_word_left(); // skip 1 space, skip "hello" → cursor at 0
        assert_eq!(ti.cursor, 0);
        ti.move_word_left(); // at start, no-op
        assert_eq!(ti.cursor, 0);
    }

    #[test]
    fn text_input_move_word_left_from_middle_of_word() {
        let mut ti = TextInput::from_str("hello world");
        ti.cursor = 8; // inside "world" at byte 8 (w=6,o=7,r=8)
        ti.move_word_left(); // no leading whitespace, skip non-ws back to 6
        assert_eq!(ti.cursor, 6);
    }

    #[test]
    fn text_input_move_word_right_basic() {
        let mut ti = TextInput::from_str("hello world foo");
        ti.cursor = 0;
        ti.move_word_right(); // skip "hello" (5), skip " " (1) → cursor at 6
        assert_eq!(ti.cursor, 6);
        ti.move_word_right(); // skip "world" (5), skip " " (1) → cursor at 12
        assert_eq!(ti.cursor, 12);
        ti.move_word_right(); // skip "foo" (3), no trailing ws → cursor at 15
        assert_eq!(ti.cursor, 15);
        ti.move_word_right(); // at end, no-op
        assert_eq!(ti.cursor, 15);
    }

    #[test]
    fn text_input_delete_word_back_basic() {
        let mut ti = TextInput::from_str("hello world");
        // cursor at end (11): skip 0 ws, skip "world" (5) back to 6. drain [6..11].
        ti.delete_word_back();
        assert_eq!(ti.value, "hello ");
        assert_eq!(ti.cursor, 6);
        // now skip " " (1 ws), skip "hello" (5) → cursor 0. drain [0..6].
        ti.delete_word_back();
        assert_eq!(ti.value, "");
        assert_eq!(ti.cursor, 0);
    }

    #[test]
    fn text_input_delete_word_back_at_start() {
        let mut ti = TextInput::from_str("hello");
        ti.cursor = 0;
        ti.delete_word_back(); // no-op
        assert_eq!(ti.value, "hello");
        assert_eq!(ti.cursor, 0);
    }
}

use super::*;

impl App {
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
            FocusedBox::Timer => {}
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

    /// Move focus up in This Week box
    pub fn this_week_focus_up(&mut self) {
        let db_count = self.this_week_history().len();
        let running_offset = if self.timer_state == TimerState::Running {
            1
        } else {
            0
        };
        let visible_count = db_count + running_offset;
        if visible_count == 0 {
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
            self.focused_this_week_index = Some(visible_count - 1);
        }
    }

    /// Move focus down in This Week box
    pub fn this_week_focus_down(&mut self) {
        let db_count = self.this_week_history().len();
        let running_offset = if self.timer_state == TimerState::Running {
            1
        } else {
            0
        };
        let visible_count = db_count + running_offset;
        if visible_count == 0 {
            self.focused_box = FocusedBox::Timer;
            self.focused_this_week_index = None;
            return;
        }

        if let Some(idx) = self.focused_this_week_index {
            if idx >= visible_count - 1 {
                self.focused_box = FocusedBox::Timer;
                self.focused_this_week_index = None;
            } else {
                self.focused_this_week_index = Some(idx + 1);
            }
        } else {
            self.focused_this_week_index = Some(0);
        }
    }
}

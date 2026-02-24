use super::*;

impl App {
    /// Enter edit mode for the currently focused This Week entry
    pub fn enter_this_week_edit_mode(&mut self) {
        if let Some(idx) = self.focused_this_week_index {
            if self.timer_state == TimerState::Running && idx == 0 {
                let start_time = self.absolute_start.unwrap_or_else(OffsetDateTime::now_utc);
                let project_id = self.selected_project.as_ref().map(|p| p.id.clone());
                let project_name = self.selected_project.as_ref().map(|p| p.name.clone());
                let activity_id = self.selected_activity.as_ref().map(|a| a.id.clone());
                let activity_name = self.selected_activity.as_ref().map(|a| a.name.clone());
                let note = Some(self.description_input.value.clone());
                self.create_edit_state(
                    String::new(), // "" = running timer sentinel
                    Some(start_time),
                    None,
                    project_id,
                    project_name,
                    activity_id,
                    activity_name,
                    note,
                );
            } else {
                let db_idx = if self.timer_state == TimerState::Running {
                    idx.saturating_sub(1)
                } else {
                    idx
                };
                let entry_data = self.this_week_history().get(db_idx).map(|e| {
                    let (start_time, end_time) =
                        derive_start_end(e.start_time, e.end_time, &e.date, e.hours);
                    (
                        e.registration_id.clone(),
                        start_time,
                        end_time,
                        e.project_id.clone(),
                        e.project_name.clone(),
                        e.activity_id.clone(),
                        e.activity_name.clone(),
                        e.note.clone(),
                    )
                });

                if let Some((
                    registration_id,
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
                        registration_id,
                        start_time,
                        end_time,
                        Some(project_id),
                        Some(project_name),
                        Some(activity_id),
                        Some(activity_name),
                        note,
                    );
                }
            }
        }
    }

    /// Enter edit mode for the currently focused History entry
    pub fn enter_history_edit_mode(&mut self) {
        if let Some(list_idx) = self.focused_history_index {
            if let Some(&history_idx) = self.history_list_entries.get(list_idx) {
                let entry_data = self.time_entries.get(history_idx).map(|e| {
                    let (start_time, end_time) =
                        derive_start_end(e.start_time, e.end_time, &e.date, e.hours);
                    (
                        e.registration_id.clone(),
                        start_time,
                        end_time,
                        e.project_id.clone(),
                        e.project_name.clone(),
                        e.activity_id.clone(),
                        e.activity_name.clone(),
                        e.note.clone(),
                    )
                });

                if let Some((
                    registration_id,
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
                        registration_id,
                        start_time,
                        end_time,
                        Some(project_id),
                        Some(project_name),
                        Some(activity_id),
                        Some(activity_name),
                        note,
                    );
                }
            }
        }
    }

    /// Create edit state from entry data
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_edit_state(
        &mut self,
        registration_id: String,
        start_time: Option<OffsetDateTime>,
        end_time: Option<OffsetDateTime>,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) {
        let start_str = start_time
            .map(|st| {
                let t = to_local_time(st).time();
                format!("{:02}:{:02}", t.hour(), t.minute())
            })
            .unwrap_or_else(|| "00:00".to_string());

        let end_str = end_time
            .map(|et| {
                let t = to_local_time(et).time();
                format!("{:02}:{:02}", t.hour(), t.minute())
            })
            .unwrap_or_else(|| "00:00".to_string());

        let edit_state = EntryEditState {
            registration_id,
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
        if let Some(state) = &mut self.this_week_edit_state {
            state.focused_field = if state.registration_id.is_empty() {
                match state.focused_field {
                    EntryEditField::StartTime => EntryEditField::Project,
                    EntryEditField::Project => EntryEditField::Activity,
                    EntryEditField::Activity => EntryEditField::Note,
                    EntryEditField::Note => EntryEditField::StartTime,
                    EntryEditField::EndTime => EntryEditField::Project,
                }
            } else {
                match state.focused_field {
                    EntryEditField::StartTime => EntryEditField::EndTime,
                    EntryEditField::EndTime => EntryEditField::Project,
                    EntryEditField::Project => EntryEditField::Activity,
                    EntryEditField::Activity => EntryEditField::Note,
                    EntryEditField::Note => EntryEditField::StartTime,
                }
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
        if let Some(state) = &mut self.this_week_edit_state {
            state.focused_field = if state.registration_id.is_empty() {
                match state.focused_field {
                    EntryEditField::StartTime => EntryEditField::Note,
                    EntryEditField::Project => EntryEditField::StartTime,
                    EntryEditField::Activity => EntryEditField::Project,
                    EntryEditField::Note => EntryEditField::Activity,
                    EntryEditField::EndTime => EntryEditField::StartTime,
                }
            } else {
                match state.focused_field {
                    EntryEditField::StartTime => EntryEditField::Note,
                    EntryEditField::EndTime => EntryEditField::StartTime,
                    EntryEditField::Project => EntryEditField::EndTime,
                    EntryEditField::Activity => EntryEditField::Project,
                    EntryEditField::Note => EntryEditField::Activity,
                }
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
                        if ('3'..='9').contains(&c) {
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
                        if ('3'..='9').contains(&c) {
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

    /// Move cursor left/right in a text field (Note only).
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
        let state = if let Some(s) = self.this_week_edit_state.as_ref() {
            s
        } else if let Some(s) = self.history_edit_state.as_ref() {
            s
        } else {
            return None;
        };

        if state.registration_id.is_empty() {
            let start_time = if state.start_time_input.is_empty() {
                "00:00"
            } else {
                &state.start_time_input
            };
            if start_time.len() != 5 || start_time.chars().nth(2) != Some(':') {
                return Some("Invalid start time format (use HH:MM)".to_string());
            }
            let start_parts: Vec<&str> = start_time.split(':').collect();
            let start_hours: u32 = start_parts[0].parse().unwrap_or(0);
            let start_mins: u32 = start_parts[1].parse().unwrap_or(0);
            if start_hours > 23 || start_mins > 59 {
                return Some("Invalid start time (hours 0-23, mins 0-59)".to_string());
            }
            return None;
        }

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

        if start_time.len() != 5 || start_time.chars().nth(2) != Some(':') {
            return Some("Invalid start time format (use HH:MM)".to_string());
        }
        if end_time.len() != 5 || end_time.chars().nth(2) != Some(':') {
            return Some("Invalid end time format (use HH:MM)".to_string());
        }

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

        let start_total_mins = start_hours * 60 + start_mins;
        let end_total_mins = end_hours * 60 + end_mins;

        if end_total_mins < start_total_mins {
            return Some("End time must be after start time".to_string());
        }

        None
    }

    /// Get the registration_id for the currently edited entry
    #[allow(dead_code)]
    pub fn editing_registration_id(&self) -> Option<&str> {
        if self.current_view == View::History {
            self.history_edit_state
                .as_ref()
                .map(|s| s.registration_id.as_str())
        } else {
            self.this_week_edit_state
                .as_ref()
                .map(|s| s.registration_id.as_str())
        }
    }

    /// Update the edit state with selected project
    pub fn update_edit_state_project(&mut self, project_id: String, project_name: String) {
        if let Some(state) = &mut self.this_week_edit_state {
            state.project_id = Some(project_id.clone());
            state.project_name = Some(project_name.clone());
        }
        if let Some(state) = &mut self.history_edit_state {
            state.project_id = Some(project_id);
            state.project_name = Some(project_name);
        }
    }

    /// Update the edit state with selected activity
    pub fn update_edit_state_activity(&mut self, activity_id: String, activity_name: String) {
        if let Some(state) = &mut self.this_week_edit_state {
            state.activity_id = Some(activity_id.clone());
            state.activity_name = Some(activity_name.clone());
        }
        if let Some(state) = &mut self.history_edit_state {
            state.activity_id = Some(activity_id);
            state.activity_name = Some(activity_name);
        }
    }

    /// Update the edit state note
    pub fn update_edit_state_note(&mut self, note: String) {
        if let Some(state) = &mut self.this_week_edit_state {
            state.note = TextInput::from_str(&note);
        }
        if let Some(state) = &mut self.history_edit_state {
            state.note = TextInput::from_str(&note);
        }
    }

    /// Check if we're in any edit mode
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
}

/// Given an entry's optional start/end times, date string (YYYY-MM-DD), and hours,
/// return a concrete (start, end) pair for pre-populating the edit form.
///
/// When real times are absent (entry booked via Milltime web UI):
/// - start and end default to 00:00 (user must fill them in manually)
fn derive_start_end(
    start_time: Option<time::OffsetDateTime>,
    end_time: Option<time::OffsetDateTime>,
    _date_str: &str,
    _hours: f64,
) -> (Option<time::OffsetDateTime>, Option<time::OffsetDateTime>) {
    (start_time, end_time)
}

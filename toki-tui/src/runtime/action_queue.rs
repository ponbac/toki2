use crate::types::{Activity, Project};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
pub(super) enum Action {
    SubmitMilltimeReauth,
    ApplyProjectSelection {
        had_edit_state: bool,
        saved_selected_project: Option<Project>,
        saved_selected_activity: Option<Activity>,
    },
    ApplyActivitySelection {
        was_in_edit_mode: bool,
        saved_selected_project: Option<Project>,
        saved_selected_activity: Option<Activity>,
    },
    StartTimer,
    SaveTimer,
    SyncRunningTimerNote {
        note: String,
    },
    SaveHistoryEdit,
    SaveThisWeekEdit,
    LoadHistoryAndOpen,
    ConfirmDelete,
    StopServerTimerAndClear,
    RefreshHistoryBackground,
}

pub(super) type ActionTx = UnboundedSender<Action>;
pub(super) type ActionRx = UnboundedReceiver<Action>;

pub(super) fn channel() -> (ActionTx, ActionRx) {
    mpsc::unbounded_channel()
}

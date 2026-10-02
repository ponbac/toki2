/** The editable note and last server value for one active timer. */
export type TimerNoteDraft = Readonly<{
  timerStartTime: string | null;
  note: string;
  serverNote: string;
}>;

/** A timer note received from the active-timer query. */
export type ServerTimerNote = Readonly<{
  timerStartTime: string;
  note: string;
}>;

/** The timer-note draft before an active timer has been loaded. */
export const EMPTY_TIMER_NOTE_DRAFT: TimerNoteDraft = {
  timerStartTime: null,
  note: "",
  serverNote: "",
};

/** Returns whether the local note differs from the last accepted server note. */
export function isTimerNoteDraftDirty(draft: TimerNoteDraft) {
  return draft.note !== draft.serverNote;
}

/** Applies a local textarea edit to the current draft. */
export function editTimerNoteDraft(
  draft: TimerNoteDraft,
  note: string,
): TimerNoteDraft {
  return draft.note === note ? draft : { ...draft, note };
}

/** Synchronizes the local draft with an active timer returned by the server. */
export function syncTimerNoteDraft(
  draft: TimerNoteDraft,
  serverTimer: ServerTimerNote,
): TimerNoteDraft {
  if (
    draft.timerStartTime === serverTimer.timerStartTime &&
    isTimerNoteDraftDirty(draft)
  ) {
    return draft;
  }

  if (
    draft.timerStartTime === serverTimer.timerStartTime &&
    draft.note === serverTimer.note &&
    draft.serverNote === serverTimer.note
  ) {
    return draft;
  }

  return {
    timerStartTime: serverTimer.timerStartTime,
    note: serverTimer.note,
    serverNote: serverTimer.note,
  };
}

/** Marks a successfully persisted note as the server baseline. */
export function confirmTimerNoteDraftSaved(
  draft: TimerNoteDraft,
  savedTimer: ServerTimerNote,
): TimerNoteDraft {
  if (draft.timerStartTime !== savedTimer.timerStartTime) {
    return draft;
  }

  return {
    ...draft,
    serverNote: savedTimer.note,
  };
}

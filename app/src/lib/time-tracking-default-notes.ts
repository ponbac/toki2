export const FIRST_TIMER_OF_THE_WEEK_NOTE = "First timer of the week...";
export const TRY_CMD_K_NEXT_TIME_NOTE = "Try Ctrl+K to start a timer next time";
export const DOING_SOMETHING_IMPORTANT_NOTE = "Doing something important...";
export const CONTINUING_MY_WORK_NOTE = "Continuing my work...";

const DEFAULT_START_TIMER_NOTES = new Set([
  FIRST_TIMER_OF_THE_WEEK_NOTE,
  TRY_CMD_K_NEXT_TIME_NOTE,
  DOING_SOMETHING_IMPORTANT_NOTE,
  CONTINUING_MY_WORK_NOTE,
]);

export function isDefaultStartTimerNote(
  note: string | null | undefined,
): boolean {
  const normalizedNote = note?.trim() ?? "";

  if (!normalizedNote) {
    return false;
  }

  return DEFAULT_START_TIMER_NOTES.has(normalizedNote);
}

export type TimelineCardText = {
  projectLabel: string;
  activityLabel: string;
  note: string;
  hasNote: boolean;
  primaryDetail: string;
};

export function buildTimelineCardText({
  projectName,
  activityName,
  note,
}: {
  projectName: string | null;
  activityName: string | null;
  note: string | null;
}): TimelineCardText {
  const trimmedNote = note?.trim() ?? "";
  const hasNote = trimmedNote.length > 0;
  const activityLabel = activityName ?? "No activity selected";

  return {
    projectLabel: projectName ?? "No project selected",
    activityLabel,
    note: trimmedNote,
    hasNote,
    primaryDetail: hasNote ? trimmedNote : activityLabel,
  };
}

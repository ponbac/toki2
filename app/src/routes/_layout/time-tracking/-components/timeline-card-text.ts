import { formatNotePreview } from "@/lib/note-preview";

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
  const notePreview = formatNotePreview(trimmedNote);
  const hasNote = notePreview !== null;
  const activityLabel = activityName ?? "No activity selected";

  return {
    projectLabel: projectName ?? "No project selected",
    activityLabel,
    note: trimmedNote,
    hasNote,
    primaryDetail: notePreview?.previewText ?? activityLabel,
  };
}

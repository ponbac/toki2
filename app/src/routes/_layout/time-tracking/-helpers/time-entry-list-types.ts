import type {
  AbsenceEntry,
  TimeEntry,
  TimeEntryStatus,
} from "@/lib/api/queries/time-tracking";

export type MergedTimeEntry = Omit<TimeEntry, "startTime" | "endTime"> & {
  timePeriods: Array<{
    startTime: string | null;
    endTime: string | null;
    status: TimeEntryStatus;
  }>;
};

export type AbsenceListEntry = {
  kind: "absence";
  id: string;
  hours: number;
  absence: AbsenceEntry;
  sortTime: number;
  startLabel: string;
  endLabel: string;
  isCapped: boolean;
};

export type TimeEntriesListEntry =
  | TimeEntry
  | MergedTimeEntry
  | AbsenceListEntry;
export type WorkListEntry = TimeEntry | MergedTimeEntry;

export function isMergedTimeEntry(
  entry: TimeEntry | MergedTimeEntry,
): entry is MergedTimeEntry {
  return "timePeriods" in entry;
}

export function isAbsenceListEntry(
  entry: TimeEntriesListEntry,
): entry is AbsenceListEntry {
  return "kind" in entry && entry.kind === "absence";
}

export function isWorkListEntry(
  entry: TimeEntriesListEntry,
): entry is WorkListEntry {
  return !isAbsenceListEntry(entry);
}

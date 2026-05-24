import dayjs from "dayjs";
import type { AbsenceEntry } from "@/lib/api/queries/time-tracking";

export const ABSENCE_TIMELINE_START_HOUR = 8;

export function buildAbsenceSearchText(absence: AbsenceEntry): string {
  return [
    absence.absenceTypeLabel,
    absence.absenceType,
    absence.child,
    absence.comment,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
}

export function buildAbsenceDisplayRange(absence: AbsenceEntry): {
  startTime: string;
  endTime: string;
  startLabel: string;
  endLabel: string;
  isCapped: boolean;
} {
  const start = dayjs(absence.date)
    .hour(ABSENCE_TIMELINE_START_HOUR)
    .minute(0)
    .second(0)
    .millisecond(0);
  const uncappedEnd = start.add(absence.hours, "hour");
  const dayEnd = dayjs(absence.date).endOf("day");
  const isCapped = uncappedEnd.isAfter(dayEnd);
  const displayEnd = isCapped ? dayEnd : uncappedEnd;

  return {
    startTime: start.toISOString(),
    endTime: displayEnd.toISOString(),
    startLabel: start.format("HH:mm"),
    endLabel: isCapped ? "24:00" : displayEnd.format("HH:mm"),
    isCapped,
  };
}

export function formatAbsenceDetails(absence: AbsenceEntry): string | null {
  const details = [absence.child, absence.comment].filter(Boolean);
  return details.length ? details.join(" · ") : null;
}

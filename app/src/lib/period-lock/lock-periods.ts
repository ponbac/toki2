import {
  addDays,
  eachDayOfInterval,
  endOfMonth,
  endOfWeek,
  format,
  getISOWeek,
  isSameMonth,
  parseISO,
  startOfMonth,
  startOfWeek,
} from "date-fns";
import type { TimeEntry, TimeEntryStatus } from "@/lib/api/queries/time-tracking";
import { formatHoursMinutes } from "@/lib/utils";

export type LockRange = { from: string; to: string };

export type LockDay = {
  date: string;
  status: TimeEntryStatus;
  reportedHours: number;
  scheduledHours: number;
  entries: Array<TimeEntry>;
  hasOverlap: boolean;
  isFuture: boolean;
  isToday: boolean;
  /** Nothing reported and nothing scheduled, so the day has nothing to review. */
  isIdle: boolean;
};

const ISO_DATE = "yyyy-MM-dd";
const MONDAY = { weekStartsOn: 1 } as const;

export const toIsoDate = (date: Date) => format(date, ISO_DATE);

export function eachDate({ from, to }: LockRange) {
  return eachDayOfInterval({ start: parseISO(from), end: parseISO(to) }).map(
    toIsoDate,
  );
}

export function weekRange(date: string): LockRange {
  const day = parseISO(date);
  return {
    from: toIsoDate(startOfWeek(day, MONDAY)),
    to: toIsoDate(endOfWeek(day, MONDAY)),
  };
}

export function monthRange(date: string): LockRange {
  const day = parseISO(date);
  return {
    from: toIsoDate(startOfMonth(day)),
    to: toIsoDate(endOfMonth(day)),
  };
}

export const isoWeekOf = (date: string) => getISOWeek(parseISO(date));

export function formatRange({ from, to }: LockRange) {
  const start = parseISO(from);
  const end = parseISO(to);
  if (from === to) return format(start, "d MMM");
  if (isSameMonth(start, end)) {
    return `${format(start, "d")}–${format(end, "d MMM")}`;
  }
  return `${format(start, "d MMM")} – ${format(end, "d MMM")}`;
}

/** A week crossing a month end is also offered split at the boundary. */
export function splitAtMonthEnd(range: LockRange): Array<LockRange> | null {
  const start = parseISO(range.from);
  const end = parseISO(range.to);
  if (isSameMonth(start, end)) return null;

  const monthEnd = endOfMonth(start);
  return [
    { from: range.from, to: toIsoDate(monthEnd) },
    { from: toIsoDate(addDays(monthEnd, 1)), to: range.to },
  ];
}

export const isLocked = (day: LockDay) => day.status !== "open";

/** Every calendar day counts: an empty weekend left open keeps its week open in Kleer. */
export const needsLock = (day: LockDay) => day.status === "open";

export function daysIn(days: Array<LockDay>, range: LockRange) {
  return days.filter((day) => day.date >= range.from && day.date <= range.to);
}

/** Runs of days in one status, so a change never spans days it shouldn't touch. */
export function rangesWithStatus(
  days: Array<LockDay>,
  status: TimeEntryStatus,
): Array<LockRange> {
  const ranges: Array<LockRange> = [];
  let previous: LockDay | undefined;

  for (const day of days) {
    if (day.status === status) {
      const last = ranges.at(-1);
      if (last && previous?.status === status) {
        last.to = day.date;
      } else {
        ranges.push({ from: day.date, to: day.date });
      }
    }
    previous = day;
  }

  return ranges;
}

export type ScopeAction = "lock" | "unlock" | "none";

export function scopeAction(days: Array<LockDay>): ScopeAction {
  if (days.some(needsLock)) return "lock";
  if (days.some((day) => day.status === "approved")) return "unlock";
  return "none";
}

export function summarize(days: Array<LockDay>) {
  const reportedHours = days.reduce((sum, d) => sum + d.reportedHours, 0);
  const scheduledHours = days.reduce((sum, d) => sum + d.scheduledHours, 0);
  const issueDays = days.filter((day) => dayIssues(day).length > 0).length;

  return {
    reportedHours,
    scheduledHours,
    issueDays,
  };
}

export type DayIssue = { kind: "missing" | "short" | "overlap"; text: string };

/** Hours still missing from a finished day; today isn't over, so it has none. */
export function hoursShort(day: LockDay) {
  if (day.isIdle || isLocked(day) || day.isFuture || day.isToday) return 0;
  const shortBy = day.scheduledHours - day.reportedHours;
  return shortBy >= 0.25 ? shortBy : 0;
}

export function dayIssues(day: LockDay): Array<DayIssue> {
  if (day.isIdle || isLocked(day) || day.isFuture) return [];

  const issues: Array<DayIssue> = [];
  const shortBy = hoursShort(day);

  if (shortBy > 0 && day.reportedHours === 0) {
    issues.push({ kind: "missing", text: "No time reported" });
  } else if (shortBy > 0) {
    issues.push({
      kind: "short",
      text: `${formatHoursMinutes(shortBy)} short of schedule`,
    });
  }
  if (day.hasOverlap) {
    issues.push({ kind: "overlap", text: "Overlapping entries" });
  }

  return issues;
}

export function hasOverlap(entries: Array<TimeEntry>) {
  const intervals = entries
    .filter((entry) => entry.startTime && entry.endTime)
    .map((entry) => ({
      start: Math.floor(new Date(entry.startTime!).getTime() / 60_000),
      end: Math.floor(new Date(entry.endTime!).getTime() / 60_000),
    }))
    .sort((a, b) => a.start - b.start);

  return intervals.some(
    (current, index) =>
      index > 0 && current.start < intervals[index - 1].end,
  );
}

export type Checkpoint = {
  kind: "week" | "month";
  label: string;
  /** Last calendar day of the period; locking reaches this far. */
  through: string;
  /** Last scheduled day of the period, when the lock becomes due. */
  dueOn: string;
  state: "done" | "upcoming" | "due" | "overdue";
};

export function listCheckpoints(days: Array<LockDay>, today: string) {
  const checkpoint = (
    kind: Checkpoint["kind"],
    label: string,
    period: LockRange,
  ): Checkpoint => {
    const periodDays = daysIn(days, period);
    const lastScheduled = periodDays.findLast((d) => d.scheduledHours > 0);
    const dueOn = lastScheduled?.date ?? period.to;
    const isDone = !periodDays.some(needsLock);

    return {
      kind,
      label,
      through: period.to,
      dueOn,
      state: isDone
        ? "done"
        : today > period.to
          ? "overdue"
          : today >= dueOn
            ? "due"
            : "upcoming",
    };
  };

  return days.flatMap((day) => {
    const week = weekRange(day.date);
    const month = monthRange(day.date);
    return [
      ...(day.date === month.to
        ? [checkpoint("month", format(parseISO(day.date), "MMMM"), month)]
        : []),
      ...(day.date === week.to
        ? [checkpoint("week", `week ${isoWeekOf(day.date)}`, week)]
        : []),
    ];
  });
}

export type DueLock = LockRange & {
  name: string;
  title: string;
  detail: string;
  severity: "due" | "overdue" | "month-overdue";
};

export function joinNames(required: Array<Checkpoint>) {
  const months = required.filter((c) => c.kind === "month");
  const weeks = required.filter((c) => c.kind === "week");
  const weekName =
    weeks.length > 2
      ? `${weeks.length} weeks`
      : weeks.map((c) => c.label).join(" and ");
  return [weekName, ...months.map((c) => c.label)]
    .filter(Boolean)
    .join(" and ");
}

export const capitalize = (text: string) =>
  text.charAt(0).toUpperCase() + text.slice(1);

export type LockLevel = "clear" | "soon" | DueLock["severity"];

/** How loudly to speak: nothing above "soon" until a lock is actually due. */
export function lockLevel(
  due: DueLock | null,
  nextDueOn: string | undefined,
  today: string,
): LockLevel {
  if (due) return due.severity;
  const tomorrow = toIsoDate(addDays(parseISO(today), 1));
  return nextDueOn === tomorrow ? "soon" : "clear";
}

/** The last day of the unbroken locked run, or null when the first day is open. */
export function lockedThrough(days: Array<LockDay>) {
  const firstOpen = days.findIndex(needsLock);
  const last = firstOpen === -1 ? days.at(-1) : days[firstOpen - 1];
  return last?.date ?? null;
}

/** Everything that has to be locked now, as one range. */
export function findDueLock(
  days: Array<LockDay>,
  checkpoints: Array<Checkpoint>,
  today: string,
): DueLock | null {
  const required = checkpoints.filter(
    (c) => c.state === "due" || c.state === "overdue",
  );
  const last = required.at(-1);
  const first = days.find(needsLock);
  if (!last || !first) return null;

  const range = { from: first.date, to: last.through };
  const overdue = required.filter((c) => c.state === "overdue");
  const name =
    required.length > 2
      ? `everything through ${formatRange({ from: last.through, to: last.through })}`
      : joinNames(required);
  const verb = required.length > 1 || name.endsWith("weeks") ? "are" : "is";
  const summary = summarize(daysIn(days, range));
  const flagged =
    summary.issueDays > 0
      ? `, ${summary.issueDays} ${summary.issueDays === 1 ? "day needs" : "days need"} a look`
      : "";

  if (overdue.length > 0) {
    const overdueName = joinNames(overdue);
    return {
      ...range,
      name,
      title: `${capitalize(overdueName)} ${overdue.length > 1 ? "are" : "is"} overdue`,
      detail: `${formatRange(range)}${flagged}.`,
      severity: overdue.some((c) => c.kind === "month")
        ? "month-overdue"
        : "overdue",
    };
  }

  return {
    ...range,
    name,
    title: `${capitalize(name)} ${verb} due${last.dueOn === today ? " today" : ""}`,
    detail: `${formatRange(range)}${flagged}.`,
    severity: "due",
  };
}

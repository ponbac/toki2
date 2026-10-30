import { useQuery } from "@tanstack/react-query";
import { endOfWeek, startOfMonth, subMonths } from "date-fns";
import { toast } from "sonner";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { apiErrorToast } from "@/lib/api/errors";
import {
  timeTrackingQueries,
  type AbsenceDayDefault,
  type AbsenceEntry,
  type TimeEntry,
  type TimeEntryDayStatus,
} from "@/lib/api/queries/time-tracking";
import {
  eachDate,
  findDueLock,
  formatRange,
  hasOverlap,
  joinNames,
  listCheckpoints,
  toIsoDate,
  type LockDay,
  type LockRange,
} from "./lock-periods";

function buildLockDays(input: {
  range: LockRange;
  today: string;
  statuses: Array<TimeEntryDayStatus> | undefined;
  schedule: Array<AbsenceDayDefault> | undefined;
  entries: Array<TimeEntry>;
  absences: Array<AbsenceEntry>;
}) {
  const { today, statuses, schedule, entries, absences } = input;

  return eachDate(input.range).map((date): LockDay => {
    const isFuture = date > today;
    const dayEntries = isFuture
      ? []
      : entries.filter((entry) => entry.date.slice(0, 10) === date);
    const absenceHours = isFuture
      ? 0
      : absences
          .filter((absence) => absence.date.slice(0, 10) === date)
          .reduce((sum, absence) => sum + absence.hours, 0);
    const reportedHours =
      dayEntries.reduce((sum, entry) => sum + entry.hours, 0) + absenceHours;
    const scheduledHours =
      schedule?.find((day) => day.date === date)?.scheduledHours ?? 0;
    // The provider leaves out days that were never locked.
    const status = statuses?.find((day) => day.date === date)?.status ?? "open";

    return {
      date,
      status,
      reportedHours,
      scheduledHours,
      entries: dayEntries,
      hasOverlap: hasOverlap(dayEntries),
      isFuture,
      isToday: date === today,
      isIdle: reportedHours === 0 && scheduledHours === 0,
    };
  });
}

const UNDO_WINDOW_MS = 10_000;

export function useLockDays(range: LockRange) {
  const today = toIsoDate(new Date());
  const { data: statuses } = useQuery(
    timeTrackingQueries.timeEntryDayStatuses(range),
  );
  const { data: entries } = useQuery(timeTrackingQueries.timeEntries(range));
  const { data: absences } = useQuery(timeTrackingQueries.absenceEntries(range));
  const { data: schedule } = useQuery(
    timeTrackingQueries.absenceDayDefaults(range),
  );

  const isLoaded = Boolean(statuses && entries && absences && schedule);
  const days = buildLockDays({
    range,
    today,
    statuses,
    // Without it a loading day would read as having nothing scheduled or missing.
    schedule: isLoaded ? schedule : undefined,
    entries: entries ?? [],
    absences: absences ?? [],
  });

  return { days, isLoaded };
}

function dueWindow(today: Date): LockRange {
  return {
    from: toIsoDate(startOfMonth(subMonths(today, 1))),
    to: toIsoDate(endOfWeek(today, { weekStartsOn: 1 })),
  };
}

/**
 * What's due, for the app shell. Runs on every page, so it reads statuses and
 * schedule only; weekend hours nobody scheduled go unnoticed until the time
 * tracking page loads the entries.
 */
export function useDueLockAlert() {
  const now = new Date();
  const range = dueWindow(now);
  const today = toIsoDate(now);
  const { data: connection } = useQuery(timeTrackingQueries.connectionStatus());
  const enabled = connection?.connected ?? false;
  const { data: statuses } = useQuery({
    ...timeTrackingQueries.timeEntryDayStatuses(range),
    enabled,
  });
  const { data: schedule } = useQuery({
    ...timeTrackingQueries.absenceDayDefaults(range),
    enabled,
  });

  if (!statuses || !schedule) return null;

  const days = buildLockDays({
    range,
    today,
    statuses,
    schedule,
    entries: [],
    absences: [],
  });
  return findDueLock(days, listCheckpoints(days, today), today);
}

/** From the start of last month, far enough back to find any overdue lock. */
export function useDueLock() {
  const today = new Date();
  const { days, isLoaded } = useLockDays(dueWindow(today));
  const todayKey = toIsoDate(today);
  const checkpoints = listCheckpoints(days, todayKey);
  const upcoming = checkpoints.filter((c) => c.state === "upcoming");
  const nextDueOn = upcoming[0]?.dueOn;

  return {
    days,
    isLoaded,
    checkpoints,
    due: isLoaded ? findDueLock(days, checkpoints, todayKey) : null,
    next: nextDueOn
      ? {
          label: joinNames(upcoming.filter((c) => c.dueOn === nextDueOn)),
          dueOn: nextDueOn,
        }
      : undefined,
  };
}

export function useLockActions() {
  const { mutateAsync: lockPeriod, isPending: isLocking } =
    timeTrackingMutations.useLockTimeEntries({
      onError: apiErrorToast("Failed to lock time"),
    });
  const { mutateAsync: unlockPeriod, isPending: isUnlocking } =
    timeTrackingMutations.useUnlockTimeEntries({
      onError: apiErrorToast("Failed to unlock time"),
    });

  const unlockRanges = async (ranges: Array<LockRange>) => {
    const results = await Promise.all(ranges.map((r) => unlockPeriod(r)));
    return results.flatMap((result) => result.stillLocked);
  };

  // A failed call has already shown its toast.
  const ignoreFailure = () => undefined;

  return {
    isPending: isLocking || isUnlocking,
    /** `openRanges` are the days this lock closes, which Undo reopens. */
    lock: async (range: LockRange, openRanges: Array<LockRange>) => {
      await lockPeriod(range);
      toast.success(`Locked ${formatRange(range)}`, {
        duration: UNDO_WINDOW_MS,
        action: {
          label: "Undo",
          onClick: () => void unlockRanges(openRanges).catch(ignoreFailure),
        },
      });
    },
    unlock: (ranges: Array<LockRange>) =>
      unlockRanges(ranges)
        .then((stillLocked) => {
          if (stillLocked.length > 0) {
            toast.warning(
              `${stillLocked.length} ${stillLocked.length === 1 ? "day stayed" : "days stayed"} locked. Kleer doesn't allow unlocking them.`,
            );
          } else {
            toast.success(`Unlocked ${ranges.map(formatRange).join(", ")}`);
          }
        })
        .catch(ignoreFailure),
  };
}

import {
  AttestLevel,
  milltimeQueries,
  TimeEntry,
} from "@/lib/api/queries/milltime";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useQuery } from "@tanstack/react-query";
import {
  endOfMonth,
  format,
  isAfter,
  isBefore,
  startOfMonth,
  startOfWeek,
} from "date-fns";
import { subMonths } from "date-fns";
import { endOfWeek, subDays } from "date-fns";
import { AlertCircle } from "lucide-react";
import { useMemo } from "react";
import { atomWithStorage } from "jotai/utils";
import { useAtom } from "jotai/react";

const previousWeekAlertDisabledAtom = atomWithStorage(
  "milltime-previous-week-alert-disabled",
  false,
);

export const NotLockedAlert = () => {
  const [isPreviousWeekAlertDisabled, setIsPreviousWeekAlertDisabled] = useAtom(
    previousWeekAlertDisabledAtom,
  );

  const { data: timeEntries } = useQuery({
    ...milltimeQueries.timeEntries({
      from: format(startOfMonth(subMonths(new Date(), 1)), "yyyy-MM-dd"),
      to: format(new Date(), "yyyy-MM-dd"),
    }),
    staleTime: 1000 * 60 * 60 * 2, // 2 hours
    gcTime: 1000 * 60 * 60 * 24, // 24 hours
  });

  const { lastWeekLocked, lastMonthLocked } = useMemo(
    () => lockedStatus(timeEntries ?? []),
    [timeEntries],
  );

  if (lastWeekLocked && lastMonthLocked) {
    return null;
  }

  return (
    <div className="flex w-fit min-w-[25rem] flex-col gap-2 pb-4">
      {!lastWeekLocked && !isPreviousWeekAlertDisabled && (
        <Alert variant="warning" className="relative">
          <AlertCircle className="size-5" />
          <AlertTitle>Previous week unlocked</AlertTitle>
          <AlertDescription>
            You need to lock last week in <MilltimeLink />.
          </AlertDescription>
          <Button
            variant="link"
            onClick={() => setIsPreviousWeekAlertDisabled(true)}
            className="absolute right-3 top-3 h-auto p-0 !pl-0 text-xs text-muted-foreground hover:text-foreground"
          >
            Don't show again
          </Button>
        </Alert>
      )}
      {!lastMonthLocked && (
        <Alert variant="destructive">
          <AlertCircle className="size-5" />
          <AlertTitle>Previous month unlocked</AlertTitle>
          <AlertDescription>
            You need to lock the previous month in <MilltimeLink />.
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
};

function MilltimeLink() {
  return (
    <a
      href={import.meta.env.VITE_MILLTIME_URL}
      className="font-medium underline transition-colors hover:text-primary/50 hover:decoration-primary/50"
      target="_blank"
      rel="noopener noreferrer"
    >
      Milltime
    </a>
  );
}

function lockedStatus(timeEntries: Array<TimeEntry>): {
  lastWeekLocked: boolean;
  lastMonthLocked: boolean;
} {
  if (timeEntries.length === 0) {
    return { lastWeekLocked: true, lastMonthLocked: true };
  }

  const now = new Date();
  const startOfLastWeek = startOfWeek(subDays(now, 7), { weekStartsOn: 1 });
  const endOfLastWeek = endOfWeek(subDays(now, 7), { weekStartsOn: 1 });
  const startOfLastMonth = startOfMonth(subMonths(now, 1));
  const endOfLastMonth = endOfMonth(subMonths(now, 1));

  const entriesLastWeek: Array<TimeEntry> = [];
  const entriesLastMonth: Array<TimeEntry> = [];
  for (let i = 0; i < timeEntries.length; i++) {
    const entry = timeEntries[i];

    const entryDate = new Date(entry.date);

    if (
      isAfter(entryDate, startOfLastWeek) &&
      isBefore(entryDate, endOfLastWeek)
    ) {
      entriesLastWeek.push(entry);
    }

    if (
      isAfter(entryDate, startOfLastMonth) &&
      isBefore(entryDate, endOfLastMonth)
    ) {
      entriesLastMonth.push(entry);
    }
  }

  const lastWeekLocked = entriesLastWeek.every(
    (entry) =>
      entry.attestLevel === AttestLevel.Week ||
      entry.attestLevel === AttestLevel.Month,
  );
  const lastMonthLocked = entriesLastMonth.every(
    (entry) =>
      entry.attestLevel === AttestLevel.Week ||
      entry.attestLevel === AttestLevel.Month,
  );

  return { lastWeekLocked, lastMonthLocked };
}

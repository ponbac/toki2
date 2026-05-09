import { timeTrackingQueries, TimeEntry } from "@/lib/api/queries/time-tracking";
import { TIME_TRACKING_PROVIDER_URL } from "@/lib/time-tracking-provider";
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
  "time-tracking-previous-week-alert-disabled",
  false,
);

export const NotLockedAlert = () => {
  const [isPreviousWeekAlertDisabled, setIsPreviousWeekAlertDisabled] = useAtom(
    previousWeekAlertDisabledAtom,
  );

  const { data: timeEntries } = useQuery({
    ...timeTrackingQueries.timeEntries({
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
            Last week is still open in <ProviderLink />.
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
            The previous month is still open in <ProviderLink />.
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
};

function ProviderLink() {
  return (
    <a
      href={TIME_TRACKING_PROVIDER_URL}
      className="font-medium underline transition-colors hover:text-primary/50 hover:decoration-primary/50"
      target="_blank"
      rel="noopener noreferrer"
    >
      Kleer
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
    (entry) => entry.status === "approved" || entry.status === "certified",
  );
  const lastMonthLocked = entriesLastMonth.every(
    (entry) => entry.status === "approved" || entry.status === "certified",
  );

  return { lastWeekLocked, lastMonthLocked };
}

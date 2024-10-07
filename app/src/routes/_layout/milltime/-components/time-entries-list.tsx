import { useMemo } from "react";
import { format } from "date-fns";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { TimeEntry } from "@/lib/api/queries/milltime";
import { formatHoursAsHoursMinutes } from "@/lib/utils";

type MergedTimeEntry = Omit<TimeEntry, "startTime" | "endTime"> & {
  timePeriods: Array<{
    startTime: string | null;
    endTime: string | null;
  }>;
};

export function TimeEntriesList(props: {
  timeEntries: Array<TimeEntry>;
  mergeSameDay: boolean;
}) {
  const groupedEntries: Array<[string, Array<TimeEntry | MergedTimeEntry>]> =
    useMemo(() => {
      const groups: { [key: string]: Array<TimeEntry> } = {};
      props.timeEntries.forEach((entry) => {
        const dateKey = format(entry.date, "yyyy-MM-dd");
        if (!groups[dateKey]) {
          groups[dateKey] = [];
        }
        groups[dateKey].push(entry);
      });

      if (props.mergeSameDay) {
        const mergedEntries: { [key: string]: MergedTimeEntry[] } = {};
        Object.entries(groups).forEach(([dateKey, dayEntries]) => {
          const mergedByProjectActivityAndNote: {
            [key: string]: MergedTimeEntry;
          } = {};
          dayEntries.forEach((entry) => {
            const key = `${entry.projectName}-${entry.activityName}-${entry.note}`;
            if (!mergedByProjectActivityAndNote[key]) {
              mergedByProjectActivityAndNote[key] = {
                ...entry,
                hours: 0,
                timePeriods: [],
              };
            }
            mergedByProjectActivityAndNote[key].hours += entry.hours;
            mergedByProjectActivityAndNote[key].timePeriods.push({
              startTime: entry.startTime,
              endTime: entry.endTime,
            });
          });
          mergedEntries[dateKey] = Object.values(
            mergedByProjectActivityAndNote,
          );
        });

        // sort merged entries by largest end time
        Object.values(mergedEntries).forEach((entries) => {
          entries.sort((a, b) => {
            const aMaxTime = a.timePeriods.reduce((max, period) => {
              return period.endTime
                ? Math.max(max, new Date(period.endTime).getTime())
                : max;
            }, 0);
            const bMaxTime = b.timePeriods.reduce((max, period) => {
              return period.endTime
                ? Math.max(max, new Date(period.endTime).getTime())
                : max;
            }, 0);
            return bMaxTime - aMaxTime;
          });
        });

        return Object.entries(mergedEntries).sort(
          ([a], [b]) => new Date(b).getTime() - new Date(a).getTime(),
        );
      }

      // sort entries within each day
      Object.values(groups).forEach((dayEntries) => {
        dayEntries.sort((a, b) => {
          const aTime = a.endTime ? new Date(a.endTime).getTime() : 0;
          const bTime = b.endTime ? new Date(b.endTime).getTime() : 0;
          return bTime - aTime;
        });
      });

      return Object.entries(groups).sort(
        ([a], [b]) => new Date(b).getTime() - new Date(a).getTime(),
      );
    }, [props.timeEntries, props.mergeSameDay]);

  return (
    <div className="mt-8 space-y-8">
      {groupedEntries.map(([dateKey, dayEntries]) => (
        <div key={dateKey}>
          <h2 className="mb-4 text-lg font-semibold">
            {format(new Date(dateKey), "EEEE")}
            <span className="ml-2 text-sm text-gray-500 dark:text-gray-400">
              {format(new Date(dateKey), "MMMM d, yyyy")}
            </span>
          </h2>
          <div className="space-y-4">
            {dayEntries.map((entry) => (
              <Card key={entry.registrationId}>
                <div className="flex items-center justify-between gap-2">
                  <CardHeader className="pb-2">
                    <CardTitle>
                      {entry.projectName} - {entry.activityName}
                    </CardTitle>
                    <CardDescription>
                      {formatHoursAsHoursMinutes(entry.hours)}
                    </CardDescription>
                  </CardHeader>
                  {isMergedTimeEntry(entry) ? (
                    <div className="flex max-h-28 flex-col overflow-hidden pr-4 [&:has(>:nth-child(2))]:mt-2">
                      {entry.timePeriods
                        .filter((period) => period.startTime && period.endTime)
                        .map((period, index) => (
                          <p
                            key={index}
                            className="text-sm text-muted-foreground only:text-base"
                          >
                            {period.startTime &&
                              format(new Date(period.startTime), "HH:mm")}
                            {" - "}
                            {period.endTime &&
                              format(new Date(period.endTime), "HH:mm")}
                          </p>
                        ))
                        .reverse()}
                    </div>
                  ) : (
                    entry.endTime && (
                      <div className="flex flex-row gap-2 pr-4">
                        <p className="text-base text-muted-foreground">
                          {entry.startTime &&
                            format(new Date(entry.startTime), "HH:mm")}
                          {" - "}
                          {entry.endTime &&
                            format(new Date(entry.endTime), "HH:mm")}
                        </p>
                      </div>
                    )
                  )}
                </div>
                <CardContent>
                  <p className="font-mono text-base">{entry.note}</p>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function isMergedTimeEntry(
  entry: TimeEntry | MergedTimeEntry,
): entry is MergedTimeEntry {
  return "timePeriods" in entry;
}

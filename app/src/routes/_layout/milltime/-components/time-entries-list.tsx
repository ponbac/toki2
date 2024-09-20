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

export function TimeEntriesList(props: { timeEntries: Array<TimeEntry> }) {
  const groupedEntries = useMemo(() => {
    const groups: { [key: string]: TimeEntry[] } = {};
    props.timeEntries.forEach((entry) => {
      const dateKey = format(entry.date, "yyyy-MM-dd");
      if (!groups[dateKey]) {
        groups[dateKey] = [];
      }
      groups[dateKey].push(entry);
    });
    return Object.entries(groups).sort(
      ([a], [b]) => new Date(b).getTime() - new Date(a).getTime(),
    );
  }, [props.timeEntries]);

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
                <CardHeader>
                  <CardTitle>
                    {entry.projectName} - {entry.activityName}
                  </CardTitle>
                  <CardDescription>
                    {formatHoursAsHoursMinutes(entry.hours)}
                  </CardDescription>
                </CardHeader>
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

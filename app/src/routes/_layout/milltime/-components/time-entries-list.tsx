import { useState, useMemo } from "react";
import { DateRange } from "react-day-picker";
import { format } from "date-fns";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

type TimeEntry = {
  id: number;
  date: Date;
  duration: number;
  project: string;
  activity: string;
  note: string;
};

type TimeEntriesListProps = {
  dateRange: DateRange;
};

export function TimeEntriesList({ dateRange }: TimeEntriesListProps) {
  const [entries, setEntries] = useState<TimeEntry[]>([
    // Sample data
    {
      id: 1,
      date: new Date(2023, 4, 15),
      duration: 120,
      project: "Project A",
      activity: "Development",
      note: "Implemented new feature",
    },
    {
      id: 2,
      date: new Date(2023, 4, 15),
      duration: 60,
      project: "Project B",
      activity: "Meeting",
      note: "Client call",
    },
    {
      id: 3,
      date: new Date(2023, 4, 16),
      duration: 90,
      project: "Project A",
      activity: "Planning",
      note: "Sprint planning",
    },
    {
      id: 4,
      date: new Date(2023, 4, 17),
      duration: 180,
      project: "Project C",
      activity: "Development",
      note: "Bug fixes",
    },
  ]);

  const groupedEntries = useMemo(() => {
    const groups: { [key: string]: TimeEntry[] } = {};
    entries.forEach((entry) => {
      const dateKey = format(entry.date, "yyyy-MM-dd");
      if (!groups[dateKey]) {
        groups[dateKey] = [];
      }
      groups[dateKey].push(entry);
    });
    return Object.entries(groups).sort(
      ([a], [b]) => new Date(b).getTime() - new Date(a).getTime(),
    );
  }, [entries]);

  const formatDuration = (minutes: number) => {
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return `${hours}h ${mins}m`;
  };

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
              <Card key={entry.id}>
                <CardHeader>
                  <CardTitle>
                    {entry.project} - {entry.activity}
                  </CardTitle>
                  <CardDescription>
                    {formatDuration(entry.duration)}
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

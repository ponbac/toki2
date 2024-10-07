import { useMemo, useState } from "react";
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
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

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

  // Add state to manage which entry is being edited
  const [editingEntryId, setEditingEntryId] = useState<string | null>(null);

  // Handler to save the updated entry
  const handleSave = (updatedEntry: TimeEntry | MergedTimeEntry) => {
    // Implement the save logic, possibly using mutations from milltime.ts
    // For example:
    // if (isMergedTimeEntry(updatedEntry)) {
    //   // Call appropriate mutation
    // } else {
    //   // Call appropriate mutation
    // }
    // After saving, reset the editing state
    setEditingEntryId(null);
  };

  // Handler to cancel editing
  const handleCancel = () => {
    setEditingEntryId(null);
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
            {dayEntries.map((entry) =>
              editingEntryId === entry.registrationId ? (
                <EditEntryCard
                  key={entry.registrationId}
                  entry={entry}
                  onSave={handleSave}
                  onCancel={handleCancel}
                />
              ) : (
                <ViewEntryCard
                  key={entry.registrationId}
                  entry={entry}
                  onEdit={() => setEditingEntryId(entry.registrationId)}
                />
              ),
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

function ViewEntryCard(props: {
  entry: TimeEntry | MergedTimeEntry;
  onEdit: () => void;
}) {
  return (
    <Card key={props.entry.registrationId}>
      <div className="flex items-center justify-between gap-2">
        <CardHeader className="pb-2">
          <CardTitle>
            {props.entry.projectName} - {props.entry.activityName}
          </CardTitle>
          <CardDescription>
            {formatHoursAsHoursMinutes(props.entry.hours)}
          </CardDescription>
        </CardHeader>
        {isMergedTimeEntry(props.entry) ? (
          <div className="flex max-h-28 flex-col overflow-hidden pr-4 [&:has(>:nth-child(2))]:mt-2">
            {props.entry.timePeriods
              .filter((period) => period.startTime && period.endTime)
              .map((period, index) => (
                <p
                  key={index}
                  className="text-sm text-muted-foreground only:text-base"
                >
                  {period.startTime &&
                    format(new Date(period.startTime), "HH:mm")}
                  {" - "}
                  {period.endTime && format(new Date(period.endTime), "HH:mm")}
                </p>
              ))
              .reverse()}
          </div>
        ) : (
          props.entry.endTime && (
            <div className="flex flex-row gap-2 pr-4">
              <p className="text-base text-muted-foreground">
                {props.entry.startTime &&
                  format(new Date(props.entry.startTime), "HH:mm")}
                {" - "}
                {props.entry.endTime &&
                  format(new Date(props.entry.endTime), "HH:mm")}
              </p>
            </div>
          )
        )}
        {/* Add Edit button */}
        <Button variant="outline" size="sm" onClick={props.onEdit}>
          Edit
        </Button>
      </div>
      <CardContent>
        <p className="font-mono text-base">{props.entry.note}</p>
      </CardContent>
    </Card>
  );
}

function EditEntryCard(props: {
  entry: TimeEntry | MergedTimeEntry;
  onSave: (updatedEntry: TimeEntry | MergedTimeEntry) => void;
  onCancel: () => void;
}) {
  const [note, setNote] = useState(props.entry.note);
  const [hours, setHours] = useState(props.entry.hours.toString());

  const handleSave = () => {
    const updatedEntry = {
      ...props.entry,
      note,
      hours: parseFloat(hours),
      // Add other fields as necessary
    };
    props.onSave(updatedEntry);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Edit Entry</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="mb-4">
          <label className="block text-sm font-medium text-gray-700">
            Note
          </label>
          <Input
            value={note ?? ""}
            onChange={(e) => setNote(e.target.value)}
            className="mt-1"
          />
        </div>
        <div className="mb-4">
          <label className="block text-sm font-medium text-gray-700">
            Hours
          </label>
          <Input
            type="number"
            value={hours}
            onChange={(e) => setHours(e.target.value)}
            className="mt-1"
          />
        </div>
        {/* Add more fields as necessary */}
      </CardContent>
      <div className="flex justify-end p-4">
        <Button variant="secondary" onClick={props.onCancel}>
          Cancel
        </Button>
        <Button className="ml-2" onClick={handleSave}>
          Save
        </Button>
      </div>
    </Card>
  );
}

function isMergedTimeEntry(
  entry: TimeEntry | MergedTimeEntry,
): entry is MergedTimeEntry {
  return "timePeriods" in entry;
}

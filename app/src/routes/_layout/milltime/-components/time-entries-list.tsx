import { useMemo, useState } from "react";
import { format } from "date-fns";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { AttestLevel, TimeEntry } from "@/lib/api/queries/milltime";
import { formatHoursAsHoursMinutes, formatHoursMinutes } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import { toast } from "sonner";
import { LockIcon, PencilIcon, SaveIcon } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";

type MergedTimeEntry = Omit<TimeEntry, "startTime" | "endTime"> & {
  timePeriods: Array<{
    startTime: string | null;
    endTime: string | null;
    attestLevel: AttestLevel;
  }>;
};

export function TimeEntriesList(props: {
  timeEntries: Array<TimeEntry>;
  mergeSameDay: boolean;
}) {
  const [editingEntryId, setEditingEntryId] = useState<string | null>(null);

  const { mutate: updateTimeEntry, isPending: isUpdatingTimeEntry } =
    milltimeMutations.useEditProjectRegistration({
      onSuccess: () => {
        setEditingEntryId(null);
      },
      onError: () => {
        toast.error(`Failed to update time entry, try again later`);
      },
    });

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
              attestLevel: entry.attestLevel,
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
              <motion.div key={entry.registrationId} layout>
                <Card>
                  <AnimatePresence mode="wait">
                    {editingEntryId === entry.registrationId ? (
                      <motion.div
                        key="edit"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={{ duration: 0.15 }}
                      >
                        <EditEntryCard
                          entry={
                            isMergedTimeEntry(entry)
                              ? {
                                  ...entry,
                                  startTime: entry.timePeriods[0].startTime,
                                  endTime: entry.timePeriods[0].endTime,
                                }
                              : entry
                          }
                          isUpdatingTimeEntry={isUpdatingTimeEntry}
                          onSave={(updatedEntry) => {
                            updateTimeEntry({
                              projectRegistrationId: entry.registrationId,
                              userNote: updatedEntry.note ?? "",
                              projectId: updatedEntry.projectId,
                              projectName: updatedEntry.projectName,
                              activityId: updatedEntry.activityId,
                              activityName: updatedEntry.activityName,
                              totalTime: formatHoursMinutes(updatedEntry.hours),
                              regDay: updatedEntry.date,
                              weekNumber: updatedEntry.weekNumber,
                            });
                          }}
                          onCancel={() => setEditingEntryId(null)}
                        />
                      </motion.div>
                    ) : (
                      <motion.div
                        key="view"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={{ duration: 0.15 }}
                      >
                        <ViewEntryCard
                          entry={entry}
                          onEdit={() => setEditingEntryId(entry.registrationId)}
                        />
                      </motion.div>
                    )}
                  </AnimatePresence>
                </Card>
              </motion.div>
            ))}
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
    <div>
      <div className="flex items-center justify-between gap-2">
        <CardHeader className="pb-2">
          <CardTitle>
            <span>
              {props.entry.projectName} - {props.entry.activityName}
            </span>
          </CardTitle>
          <CardDescription>
            {formatHoursAsHoursMinutes(props.entry.hours)}
          </CardDescription>
        </CardHeader>
        {isMergedTimeEntry(props.entry) ? (
          <>
            {props.entry.timePeriods.length === 1 &&
              props.entry.timePeriods.at(0)?.attestLevel ===
                AttestLevel.None && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={props.onEdit}
                  className="ml-auto size-8"
                >
                  <PencilIcon className="size-4" />
                </Button>
              )}
            {props.entry.timePeriods.every(
              (period) => period.attestLevel !== AttestLevel.None,
            ) && <LockIcon className="ml-auto size-4 text-muted-foreground" />}
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
                    {period.endTime &&
                      format(new Date(period.endTime), "HH:mm")}
                  </p>
                ))}
            </div>
          </>
        ) : (
          <div className="mr-4 mt-4 flex flex-row items-center gap-2">
            {props.entry.attestLevel === AttestLevel.None ? (
              <Button
                variant="ghost"
                size="icon"
                onClick={props.onEdit}
                className="size-8"
              >
                <PencilIcon className="size-4" />
              </Button>
            ) : (
              <LockIcon className="size-4 text-muted-foreground" />
            )}
            {props.entry.endTime && (
              <p className="text-base text-muted-foreground">
                {props.entry.startTime &&
                  format(new Date(props.entry.startTime), "HH:mm")}
                {" - "}
                {props.entry.endTime &&
                  format(new Date(props.entry.endTime), "HH:mm")}
              </p>
            )}
          </div>
        )}
      </div>
      <CardContent>
        <p className="font-mono text-base">{props.entry.note}</p>
      </CardContent>
    </div>
  );
}

function EditEntryCard(props: {
  entry: TimeEntry;
  onSave: (updatedEntry: TimeEntry) => void;
  onCancel: () => void;
  isUpdatingTimeEntry: boolean;
}) {
  const [note, setNote] = useState(props.entry.note);
  const [hours, setHours] = useState(Math.floor(props.entry.hours));
  const [minutes, setMinutes] = useState(
    Math.round((props.entry.hours - Math.floor(props.entry.hours)) * 60),
  );

  const handleSave = () => {
    const updatedEntry = {
      ...props.entry,
      note,
      hours: hours + minutes / 60,
    };
    props.onSave(updatedEntry);
  };

  return (
    <div>
      <CardHeader>
        <CardTitle>
          Edit Entry{" "}
          <span className="text-muted-foreground">
            ({props.entry.projectName} - {props.entry.activityName})
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="mb-4">
          <label className="block text-sm font-medium">Note</label>
          <Input
            value={note ?? ""}
            onChange={(e) => setNote(e.target.value)}
            className="mt-1"
          />
        </div>
        <div className="mb-4 flex w-1/2 items-end gap-4">
          <div className="flex-1">
            <label className="block text-sm font-medium">Hours</label>
            <Input
              type="number"
              value={hours}
              onChange={(e) => setHours(parseInt(e.target.value))}
              className="mt-1"
              min={0}
            />
          </div>
          <div className="flex-1">
            <label className="block text-sm font-medium">Minutes</label>
            <Input
              type="number"
              value={minutes}
              onChange={(e) => setMinutes(parseInt(e.target.value))}
              className="mt-1"
              min={0}
            />
          </div>
        </div>
      </CardContent>
      <div className="flex justify-end gap-4 p-4">
        <Button size="sm" variant="outline" onClick={props.onCancel}>
          Cancel
        </Button>
        <Button
          size="sm"
          onClick={handleSave}
          disabled={props.isUpdatingTimeEntry}
        >
          <SaveIcon className="mr-2 size-4" />
          Save
        </Button>
      </div>
    </div>
  );
}

function isMergedTimeEntry(
  entry: TimeEntry | MergedTimeEntry,
): entry is MergedTimeEntry {
  return "timePeriods" in entry;
}

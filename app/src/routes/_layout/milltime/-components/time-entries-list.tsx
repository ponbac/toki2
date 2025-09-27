import { useMemo, useState } from "react";
import { format } from "date-fns";
import dayjs from "dayjs";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { AttestLevel, TimeEntry } from "@/lib/api/queries/milltime";
import { cn, formatHoursAsHoursMinutes } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import { toast } from "sonner";
import {
  AlertTriangleIcon,
  LockIcon,
  PencilIcon,
  SaveIcon,
  TrashIcon,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

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

  const overlapMap = useMemo(() => {
    // Disable overlap calculation if too many entries (performance safeguard)
    const totalVisible = groupedEntries.reduce(
      (sum, [, entries]) => sum + entries.length,
      0,
    );
    if (totalVisible > 250) return {} as Record<string, boolean>;

    return groupedEntries.reduce<Record<string, boolean>>(
      (acc, [, dayEntries]) => {
        const intervals = dayEntries
          .flatMap((entry) => {
            if (isMergedTimeEntry(entry)) {
              return entry.timePeriods
                .map((p, i) =>
                  p.startTime && p.endTime
                    ? {
                        id: `${entry.registrationId}-p${i}`,
                        start: new Date(p.startTime).getTime(),
                        end: new Date(p.endTime).getTime(),
                      }
                    : null,
                )
                .filter(Boolean) as Array<{
                id: string;
                start: number;
                end: number;
              }>;
            }
            return entry.startTime && entry.endTime
              ? [
                  {
                    id: entry.registrationId,
                    start: new Date(entry.startTime).getTime(),
                    end: new Date(entry.endTime).getTime(),
                  },
                ]
              : [];
          })
          .sort((a, b) => a.start - b.start);

        intervals.forEach((curr, idx) => {
          for (
            let j = idx + 1;
            j < intervals.length && intervals[j].start < curr.end;
            j++
          ) {
            acc[curr.id] = true;
            acc[intervals[j].id] = true;
          }
        });
        return acc;
      },
      {},
    );
  }, [groupedEntries]);

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
                          onSaved={() => {
                            setEditingEntryId(null);
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
                          overlapMap={overlapMap}
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
  overlapMap: Record<string, boolean>;
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
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={props.onEdit}
                      className="ml-auto size-8"
                    >
                      <PencilIcon className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Edit entry</TooltipContent>
                </Tooltip>
              )}
            {props.entry.timePeriods.every(
              (period) => period.attestLevel !== AttestLevel.None,
            ) && <LockIcon className="ml-auto size-4 text-muted-foreground" />}
            <div className="flex max-h-28 flex-col overflow-hidden pr-4 [&:has(>:nth-child(2))]:mt-2">
              {props.entry.timePeriods
                .filter((period) => period.startTime && period.endTime)
                .map((period, index) => {
                  const periodId = `${props.entry.registrationId}-p${index}`;
                  const isOverlap = props.overlapMap[periodId];
                  return (
                    <p
                      key={index}
                      className="flex items-center gap-1 text-sm text-muted-foreground only:text-base"
                    >
                      {period.startTime &&
                        format(new Date(period.startTime), "HH:mm")}
                      {" - "}
                      {period.endTime &&
                        format(new Date(period.endTime), "HH:mm")}
                      {isOverlap && (
                        <OverlapWarning
                          className={
                            (props.entry as MergedTimeEntry).timePeriods
                              .length === 1
                              ? "size-5"
                              : "size-4"
                          }
                        />
                      )}
                    </p>
                  );
                })}
            </div>
          </>
        ) : (
          <div className="mr-4 mt-4 flex flex-row items-center gap-2">
            {props.entry.attestLevel === AttestLevel.None ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={props.onEdit}
                    className="size-8"
                  >
                    <PencilIcon className="size-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Edit entry</TooltipContent>
              </Tooltip>
            ) : (
              <LockIcon className="size-4 text-muted-foreground" />
            )}
            {props.entry.endTime && (
              <p className="flex items-center gap-1 text-base text-muted-foreground">
                {props.entry.startTime &&
                  format(new Date(props.entry.startTime), "HH:mm")}
                {" - "}
                {props.entry.endTime &&
                  format(new Date(props.entry.endTime), "HH:mm")}
                {props.overlapMap[props.entry.registrationId] && (
                  <OverlapWarning />
                )}
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
  onSaved: () => void;
  onCancel: () => void;
}) {
  const [note, setNote] = useState(props.entry.note);
  const [hours, setHours] = useState(Math.floor(props.entry.hours));
  const [minutes, setMinutes] = useState(
    Math.round((props.entry.hours - Math.floor(props.entry.hours)) * 60),
  );
  const [startTime, setStartTime] = useState(
    props.entry.startTime
      ? dayjs(props.entry.startTime).format("HH:mm")
      : "06:00",
  );
  const [endTime, setEndTime] = useState(
    props.entry.endTime ? dayjs(props.entry.endTime).format("HH:mm") : "",
  );

  // Keep time range and total time in sync
  const updateTimeRange = (start: string, end: string) => {
    setStartTime(start);
    setEndTime(end);
    if (start && end) {
      const startDate = dayjs(`2000-01-01T${start}`);
      const endDate = dayjs(`2000-01-01T${end}`);
      const diffHours = endDate.diff(startDate, "hour", true);
      setHours(Math.floor(diffHours));
      setMinutes(Math.round((diffHours - Math.floor(diffHours)) * 60));
    }
  };

  const updateTotalTime = (h: number, m: number) => {
    setHours(h);
    setMinutes(m);
    if (startTime) {
      const startDate = dayjs(`2000-01-01T${startTime}`);
      const endDate = startDate.add(h, "hour").add(m, "minute");
      setEndTime(endDate.format("HH:mm"));
    }
  };

  const { mutate: updateTimeEntry, isPending: isUpdatingTimeEntry } =
    milltimeMutations.useEditProjectRegistration({
      onSuccess: () => {
        props.onSaved();
      },
      onError: () => {
        toast.error(`Failed to update time entry, try again later`);
      },
    });

  const { mutate: deleteTimeEntry, isPending: isDeletingTimeEntry } =
    milltimeMutations.useDeleteProjectRegistration({
      onSuccess: () => {
        props.onSaved();
        toast.success("Time entry deleted successfully");
      },
      onError: () => {
        toast.error("Failed to delete time entry, try again later");
      },
    });

  const handleSave = () => {
    const startDateTime = dayjs(`${props.entry.date}T${startTime}`);
    const endDateTime = dayjs(`${props.entry.date}T${endTime}`);

    updateTimeEntry({
      projectRegistrationId: props.entry.registrationId,
      userNote: note ?? "",
      projectId: props.entry.projectId,
      projectName: props.entry.projectName,
      activityId: props.entry.activityId,
      activityName: props.entry.activityName,
      startTime: startDateTime.toISOString(),
      endTime: endDateTime.toISOString(),
      regDay: props.entry.date,
      weekNumber: props.entry.weekNumber,
    });
  };

  const handleDelete = () => {
    if (
      window.confirm(
        "Are you sure you want to delete this time entry? This action cannot be undone.",
      )
    ) {
      deleteTimeEntry({
        projectRegistrationId: props.entry.registrationId,
      });
    }
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
      <CardContent className="space-y-6">
        <div>
          <label className="block text-sm font-medium">Note</label>
          <Input
            value={note ?? ""}
            onChange={(e) => setNote(e.target.value)}
            className="mt-1"
          />
        </div>

        <div className="relative flex gap-12">
          <div className="space-y-4">
            <h3 className="font-medium">Range</h3>
            <div className="flex gap-4">
              <div className="w-32">
                <label className="block text-sm font-medium text-muted-foreground">
                  Start Time
                </label>
                <Input
                  type="time"
                  value={startTime}
                  onChange={(e) => updateTimeRange(e.target.value, endTime)}
                  className="mt-1"
                />
              </div>
              <div className="w-32">
                <label className="block text-sm font-medium text-muted-foreground">
                  End Time
                </label>
                <Input
                  type="time"
                  value={endTime}
                  onChange={(e) => updateTimeRange(startTime, e.target.value)}
                  className="mt-1"
                />
              </div>
            </div>
          </div>

          <Separator
            orientation="vertical"
            className="mb-[6px] h-[80px] self-end"
          />

          <div className="space-y-4">
            <h3 className="font-medium">Total</h3>
            <div className="flex gap-4">
              <div className="w-24">
                <label className="block text-sm font-medium text-muted-foreground">
                  Hours
                </label>
                <Input
                  type="number"
                  value={hours}
                  onChange={(e) =>
                    updateTotalTime(parseInt(e.target.value), minutes)
                  }
                  className="mt-1"
                  min={0}
                />
              </div>
              <div className="w-24">
                <label className="block text-sm font-medium text-muted-foreground">
                  Minutes
                </label>
                <Input
                  type="number"
                  value={minutes}
                  onChange={(e) =>
                    updateTotalTime(hours, parseInt(e.target.value))
                  }
                  className="mt-1"
                  min={0}
                  max={59}
                />
              </div>
            </div>
          </div>
        </div>
      </CardContent>
      <div className="flex justify-between p-4">
        <Button
          size="sm"
          variant="outline"
          onClick={handleDelete}
          disabled={isDeletingTimeEntry || isUpdatingTimeEntry}
          className="group"
        >
          <TrashIcon className="size-4 transition-colors group-hover:text-destructive" />
          Delete
        </Button>
        <div className="flex gap-4">
          <Button size="sm" variant="outline" onClick={props.onCancel}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={isUpdatingTimeEntry || !startTime || !endTime}
          >
            <SaveIcon className="size-4" />
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}

function isMergedTimeEntry(
  entry: TimeEntry | MergedTimeEntry,
): entry is MergedTimeEntry {
  return "timePeriods" in entry;
}

function OverlapWarning(props: { className?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <AlertTriangleIcon
          className={cn("ml-0.5 size-5 text-primary", props.className)}
        />
      </TooltipTrigger>
      <TooltipContent>
        Overlapping time interval with another entry
      </TooltipContent>
    </Tooltip>
  );
}

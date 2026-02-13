import React, { useMemo, useState } from "react";
import dayjs from "dayjs";
import { AttestLevel, TimeEntry } from "@/lib/api/queries/time-tracking";
import { cn, formatHoursAsHoursMinutes } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { toast } from "sonner";
import {
  AlertTriangleIcon,
  LockIcon,
  PencilIcon,
  PlayIcon,
  ChevronRight,
  Clock,
  Briefcase,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { buildProjectStyleMap, withAlpha } from "./colors";
import { motion, AnimatePresence } from "framer-motion";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { TimeEntryEditContent } from "./time-entry-edit-content";

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
        const dateKey = dayjs(entry.date).format("YYYY-MM-DD");
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
            mergedByProjectActivityAndNote
          );
        });

        Object.values(mergedEntries).forEach((entries) => {
          const maxTimeCache = new Map<MergedTimeEntry, number>();
          entries.forEach((entry) => {
            const maxTime = entry.timePeriods.reduce((max, period) => {
              return period.endTime
                ? Math.max(max, new Date(period.endTime).getTime())
                : max;
            }, 0);
            maxTimeCache.set(entry, maxTime);
          });
          entries.sort((a, b) => maxTimeCache.get(b)! - maxTimeCache.get(a)!);
        });

        return Object.entries(mergedEntries).sort(
          ([a], [b]) => new Date(b).getTime() - new Date(a).getTime()
        );
      }

      Object.values(groups).forEach((dayEntries) => {
        dayEntries.sort((a, b) => {
          const aTime = a.endTime ? new Date(a.endTime).getTime() : 0;
          const bTime = b.endTime ? new Date(b.endTime).getTime() : 0;
          return bTime - aTime;
        });
      });

      return Object.entries(groups).sort(
        ([a], [b]) => new Date(b).getTime() - new Date(a).getTime()
      );
    }, [props.timeEntries, props.mergeSameDay]);

  const projectStyleMap = useMemo(
    () => buildProjectStyleMap(props.timeEntries),
    [props.timeEntries],
  );

  const overlapMap = useMemo(() => {
    const totalVisible = groupedEntries.reduce(
      (sum, [, entries]) => sum + entries.length,
      0
    );
    if (totalVisible > 250) return {};

    const result: Record<string, boolean> = {};

    groupedEntries.forEach(([, dayEntries]) => {
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
                  : null
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

      const dayOverlaps = new Set<string>();

      intervals.forEach((curr, idx) => {
        for (
          let j = idx + 1;
          j < intervals.length && intervals[j].start < curr.end;
          j++
        ) {
          const currEndMinute = Math.floor(curr.end / 60000);
          const nextStartMinute = Math.floor(intervals[j].start / 60000);
          if (nextStartMinute === currEndMinute) continue;

          dayOverlaps.add(curr.id);
          dayOverlaps.add(intervals[j].id);
        }
      });

      if (dayOverlaps.size > 1) {
        dayOverlaps.forEach((id) => {
          result[id] = true;
        });
      }
    });

    return result;
  }, [groupedEntries]);

  return (
    <div className="space-y-10">
      {groupedEntries.map(([dateKey, dayEntries], groupIndex) => (
        <motion.div
          key={dateKey}
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: groupIndex * 0.1 }}
        >
          {/* Day Header */}
          <div className="mb-4 flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
              <span className="font-display text-lg font-bold">
                {dayjs(dateKey).format("DD")}
              </span>
            </div>
            <div>
              <h2 className="font-display text-lg font-semibold leading-tight">
                {dayjs(dateKey).format("dddd")}
              </h2>
              <p className="text-sm text-muted-foreground">
                {dayjs(dateKey).format("MMMM YYYY")}
              </p>
            </div>
            <div className="ml-auto flex items-center gap-2 text-sm text-muted-foreground">
              <Clock className="h-4 w-4" />
              <span className="time-display">
                {formatHoursAsHoursMinutes(
                  dayEntries.reduce((sum, e) => sum + e.hours, 0)
                )}
              </span>
            </div>
          </div>

          {/* Entries for this day */}
          <div className="space-y-3">
            <AnimatePresence mode="popLayout">
              {dayEntries.map((entry, entryIndex) => (
                <motion.div
                  key={entry.registrationId}
                  layout
                  initial={{ opacity: 0, scale: 0.98 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.98 }}
                  transition={{ duration: 0.2, delay: entryIndex * 0.03 }}
                >
                  {editingEntryId === entry.registrationId ? (
                    <TimeEntryEditContent
                      entry={
                        isMergedTimeEntry(entry)
                          ? {
                              ...entry,
                              startTime: entry.timePeriods[0].startTime,
                              endTime: entry.timePeriods[0].endTime,
                            }
                          : entry
                      }
                      onSaved={() => setEditingEntryId(null)}
                      onCancel={() => setEditingEntryId(null)}
                      variant="inline"
                    />
                  ) : (
                    <ViewEntryCard
                      entry={entry}
                      onEdit={() => setEditingEntryId(entry.registrationId)}
                      overlapMap={overlapMap}
                      projectColor={projectStyleMap.get(entry.projectName)?.color}
                      ProjectIcon={projectStyleMap.get(entry.projectName)?.Icon}
                    />
                  )}
                </motion.div>
              ))}
            </AnimatePresence>
          </div>
        </motion.div>
      ))}
    </div>
  );
}

const StartAgainButton = React.memo(function StartAgainButton(props: {
  note: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
}) {
  const { mutateAsync: startTimerAsync, isPending: isStarting } =
    timeTrackingMutations.useStartTimer();
  const { mutateAsync: editTimerAsync } =
    timeTrackingMutations.useEditTimer();

  const { state: timerState } = useTimeTrackingTimer();

  const handleStartAgain = () => {
    const isTimerActive = timerState === "running";

    if (isTimerActive) {
      editTimerAsync({
        userNote: props.note,
        projectId: props.projectId,
        projectName: props.projectName,
        activityId: props.activityId,
        activityName: props.activityName,
      })
        .then(() => toast.success("Timer updated"))
        .catch(() => toast.error("Failed to update timer"));
      return;
    }

    startTimerAsync({ userNote: props.note })
      .then(() =>
        editTimerAsync({
          projectId: props.projectId,
          projectName: props.projectName,
          activityId: props.activityId,
          activityName: props.activityName,
        })
      )
      .then(() => toast.success("Timer started"))
      .catch(() => toast.error("Failed to start timer"));
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleStartAgain}
          disabled={isStarting}
          className="h-7 w-7 rounded-md p-0 hover:bg-primary/10 hover:text-primary"
        >
          <PlayIcon className="h-3.5 w-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>Start again</TooltipContent>
    </Tooltip>
  );
});

function ViewEntryCard(props: {
  entry: TimeEntry | MergedTimeEntry;
  onEdit: () => void;
  overlapMap: Record<string, boolean>;
  projectColor?: string;
  ProjectIcon?: LucideIcon;
}) {
  const entry = props.entry;
  const isMerged = isMergedTimeEntry(entry);
  const mergedPeriodsWithTimes = isMerged
    ? entry.timePeriods.reduce<
        Array<{
          index: number;
          startTime: string;
          endTime: string;
          attestLevel: AttestLevel;
        }>
      >((acc, period, index) => {
        if (period.startTime && period.endTime) {
          acc.push({
            index,
            startTime: period.startTime,
            endTime: period.endTime,
            attestLevel: period.attestLevel,
          });
        }
        return acc;
      }, [])
    : [];
  const isLocked = isMerged
    ? entry.timePeriods.every((p) => p.attestLevel !== AttestLevel.None)
    : entry.attestLevel !== AttestLevel.None;

  const Icon = isLocked ? LockIcon : (props.ProjectIcon ?? Briefcase);

  const renderTimeRange = () => {
    if (isMerged) {
      if (mergedPeriodsWithTimes.length === 0) return null;

      if (mergedPeriodsWithTimes.length === 1) {
        const period = mergedPeriodsWithTimes[0];
        const periodId = `${entry.registrationId}-p${period.index}`;
        const isOverlap = props.overlapMap[periodId];
        return (
          <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
            <span className="time-display">
              {dayjs(period.startTime).format("HH:mm")}
            </span>
            <ChevronRight className="h-3 w-3" />
            <span className="time-display">
              {dayjs(period.endTime).format("HH:mm")}
            </span>
            {isOverlap && <OverlapWarning />}
          </div>
        );
      }

      return (
        <div className="flex max-h-20 flex-col gap-0.5 overflow-hidden text-sm text-muted-foreground">
          {mergedPeriodsWithTimes.slice(0, 3).map((period) => {
            const periodId = `${entry.registrationId}-p${period.index}`;
            const isOverlap = props.overlapMap[periodId];
            return (
              <div key={periodId} className="flex items-center gap-1">
                <span className="time-display text-xs">
                  {dayjs(period.startTime).format("HH:mm")} -{" "}
                  {dayjs(period.endTime).format("HH:mm")}
                </span>
                {isOverlap && <OverlapWarning className="h-3 w-3" />}
              </div>
            );
          })}
          {mergedPeriodsWithTimes.length > 3 && (
            <span className="text-xs text-muted-foreground/70">
              +{mergedPeriodsWithTimes.length - 3} more
            </span>
          )}
        </div>
      );
    }

    if (!entry.endTime) return null;

    return (
      <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
        <span className="time-display">
          {entry.startTime && dayjs(entry.startTime).format("HH:mm")}
        </span>
        <ChevronRight className="h-3 w-3" />
        <span className="time-display">
          {entry.endTime && dayjs(entry.endTime).format("HH:mm")}
        </span>
        {props.overlapMap[entry.registrationId] && <OverlapWarning />}
      </div>
    );
  };

  const timeRange = renderTimeRange();

  return (
    <div
      className={cn(
        "group relative overflow-hidden rounded-xl border border-border/50 bg-card/50 p-4 transition-all duration-300",
        "hover:border-border hover:bg-card hover:shadow-elevated",
        isLocked && "bg-muted/30"
      )}
    >
      {/* Subtle gradient overlay on hover */}
      <div className="pointer-events-none absolute inset-0 bg-gradient-to-r from-primary/0 via-primary/0 to-primary/0 opacity-0 transition-opacity duration-300 group-hover:opacity-100 group-hover:from-primary/[0.02] group-hover:to-transparent" />

      <div className="relative flex gap-4">
        {/* Project indicator */}
        <div className="flex flex-col items-center">
          <div
            className={cn(
              "flex h-10 w-10 shrink-0 items-center justify-center rounded-lg",
              isLocked && "bg-muted text-muted-foreground",
            )}
            style={
              !isLocked && props.projectColor
                ? {
                    backgroundColor: withAlpha(props.projectColor, 0.15),
                    color: props.projectColor,
                  }
                : !isLocked
                  ? { backgroundColor: "hsl(var(--primary) / 0.1)", color: "hsl(var(--primary))" }
                  : undefined
            }
          >
            <Icon className="h-4 w-4" />
          </div>
        </div>

        {/* Main content */}
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0 flex-1">
              {/* Project name */}
              <h3 className="truncate font-semibold leading-tight">
                {entry.projectName}
              </h3>
              {/* Activity */}
              <p className="text-sm text-muted-foreground">
                {entry.activityName}
              </p>
            </div>

            {/* Duration badge - hidden on hover, replaced by action buttons */}
            <div className="shrink-0">
              <div
                className="rounded-lg px-3 py-1.5 text-sm font-semibold transition-opacity group-hover:opacity-0"
                style={
                  props.projectColor
                    ? {
                        backgroundColor: withAlpha(props.projectColor, 0.15),
                        color: props.projectColor,
                      }
                    : {
                        backgroundColor: "hsl(var(--primary) / 0.1)",
                        color: "hsl(var(--primary))",
                      }
                }
              >
                <span className="time-display">
                  {formatHoursAsHoursMinutes(entry.hours)}
                </span>
              </div>
              {/* Action buttons - appear on hover in same position */}
              <div className="absolute right-4 top-4 flex items-center gap-1 rounded-lg border border-border/50 bg-card p-1 opacity-0 shadow-sm transition-opacity group-hover:opacity-100">
                {!isLocked && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={props.onEdit}
                          disabled={isMerged && entry.timePeriods.length > 1}
                          className="h-7 w-7 rounded-md p-0 hover:bg-primary/10 hover:text-primary"
                        >
                          <PencilIcon className="h-3.5 w-3.5" />
                        </Button>
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      {isMerged && entry.timePeriods.length > 1
                        ? "Unmerge to edit"
                        : "Edit entry"}
                    </TooltipContent>
                  </Tooltip>
                )}
                <StartAgainButton
                  note={entry.note ?? ""}
                  projectId={entry.projectId}
                  projectName={entry.projectName}
                  activityId={entry.activityId}
                  activityName={entry.activityName}
                />
              </div>
            </div>
          </div>

          {/* Note */}
          {entry.note && (
            <p className="mt-2 line-clamp-2 font-mono text-sm text-foreground/80">
              {entry.note}
            </p>
          )}

          {/* Time range */}
          {timeRange ? <div className="mt-2">{timeRange}</div> : null}
        </div>
      </div>
    </div>
  );
}

function isMergedTimeEntry(
  entry: TimeEntry | MergedTimeEntry
): entry is MergedTimeEntry {
  return "timePeriods" in entry;
}

function OverlapWarning(props: { className?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <AlertTriangleIcon
          className={cn("h-4 w-4 text-amber-500", props.className)}
        />
      </TooltipTrigger>
      <TooltipContent>Overlapping time interval</TooltipContent>
    </Tooltip>
  );
}
